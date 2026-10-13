use super::*;

#[test]
fn automatic_audio_decoder_handles_fragmented_and_coalesced_demand() {
    let mut decoder = DemandDecoder::default();
    let partial = decoder.push(b"AXSSH_AUDIO_RE").unwrap();
    assert!(!partial.ready && partial.demand.is_none());
    let update = decoder
        .push(b"ADY\nAXSSH_AUDIO_DEMAND 1\nAXSSH_AUDIO_DEMAND 0\n")
        .unwrap();
    assert!(update.ready);
    assert_eq!(update.demand, Some(false));
    assert_eq!(
        decoder.push(b"AXSSH_AUDIO_DEMAND 1\n").unwrap().demand,
        Some(true)
    );
}

#[test]
fn automatic_audio_decoder_rejects_unready_unknown_and_oversized_messages() {
    for message in [
        b"AXSSH_AUDIO_DEMAND 1\n".as_slice(),
        b"anything\n",
        &[b'x'; 65],
        &[b'x'; 4097],
    ] {
        assert!(DemandDecoder::default().push(message).is_err());
    }
    let mut decoder = DemandDecoder::default();
    decoder.push(b"AXSSH_AUDIO_READY\n").unwrap();
    assert!(decoder.push(b"AXSSH_AUDIO_READY\n").is_err());
}

#[test]
fn automatic_audio_command_accepts_only_generated_identity_and_valid_format() {
    assert!(
        helper_command(
            "axssh_00000000000000000000000000000001",
            AudioFormat::SPEECH
        )
        .is_ok()
    );
    assert!(helper_command("axssh_bad'; exit 1", AudioFormat::SPEECH).is_err());
    assert!(
        helper_command(
            "axssh_00000000000000000000000000000001",
            AudioFormat {
                sample_rate: 0,
                channels: 2
            }
        )
        .is_err()
    );
}

#[tokio::test]
async fn automatic_audio_opens_only_on_demand_and_clears_paused_pcm() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let starts = Arc::new(AtomicUsize::new(0));
    let stops = Arc::new(AtomicUsize::new(0));
    let factory = || {
        let starts = starts.clone();
        let stops = stops.clone();
        move |mut stop: watch::Receiver<bool>, _: AudioFormat, pcm: mpsc::Sender<Vec<u8>>| {
            let epoch = starts.fetch_add(1, Ordering::SeqCst) + 1;
            async move {
                pcm.send(vec![epoch as u8]).await?;
                audio::wait_for_stop(&mut stop).await;
                stops.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        }
    };
    let mut controller = CaptureController::new();
    controller
        .set_demand(false, AudioFormat::SPEECH, factory())
        .await;
    assert_eq!(
        starts.load(Ordering::SeqCst),
        0,
        "idle must not open the device"
    );
    controller
        .set_demand(true, AudioFormat::SPEECH, factory())
        .await;
    controller
        .set_demand(true, AudioFormat::SPEECH, factory())
        .await;
    assert_eq!(
        starts.load(Ordering::SeqCst),
        1,
        "duplicate demand must share capture"
    );
    tokio::task::yield_now().await;
    controller
        .set_demand(false, AudioFormat::SPEECH, factory())
        .await;
    assert_eq!(stops.load(Ordering::SeqCst), 1);
    assert!(controller.captures.is_empty());
    assert!(
        controller.packets.try_recv().is_err(),
        "paused PCM must not reach another recording"
    );
    controller
        .set_demand(true, AudioFormat::SPEECH, factory())
        .await;
    assert_eq!(controller.packets.recv().await.unwrap(), [2]);
    stop_capture(&mut controller.stop, &mut controller.captures).await;
    assert_eq!(
        stops.load(Ordering::SeqCst),
        2,
        "session shutdown must revoke capture"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn automatic_audio_helper_tracks_only_its_uncorked_recording_streams() {
    helper_fixture(false, false).await;
}

#[cfg(unix)]
#[tokio::test]
async fn automatic_audio_helper_preserves_an_external_default_source_change() {
    helper_fixture(true, false).await;
}

#[cfg(unix)]
#[tokio::test]
async fn automatic_audio_helper_term_cleans_up_an_idle_fifo_without_a_reader() {
    helper_fixture(false, true).await;
}

#[cfg(unix)]
async fn helper_fixture(external_default: bool, stop_while_idle: bool) {
    #[cfg(target_os = "macos")]
    let _launch_guard = super::super::MACOS_HELPER_FIXTURE_GATE.lock().await;
    use std::os::unix::fs::PermissionsExt;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let root = std::env::temp_dir().join(format!("axssh-automatic-audio-{}", Uuid::new_v4()));
    std::fs::create_dir_all(root.join("bin")).unwrap();
    std::fs::write(root.join("default"), "original").unwrap();
    std::fs::write(root.join("streams"), "").unwrap();
    let source = "axssh_00000000000000000000000000000001";
    let pactl = r#"#!/bin/sh
case "$1" in
info) exit 0 ;;
get-default-source) cat "$AXSSH_TEST_ROOT/default" ;;
set-default-source) printf '%s' "$2" > "$AXSSH_TEST_ROOT/default" ;;
load-module)
    for argument in "$@"; do case "$argument" in file=*) fifo=${argument#file=} ;; esac; done
    if [ -z "$AXSSH_TEST_IDLE" ]; then
        cat "$fifo" > "$AXSSH_TEST_ROOT/received" &
    fi
    printf '123\n'
    ;;
unload-module) printf '%s' "$2" > "$AXSSH_TEST_ROOT/unloaded" ;;
list)
    if [ "$2" = sources ]; then
        printf '23\taxssh_00000000000000000000000000000001\tmodule-pipe-source\ts16le 2ch 48000Hz\tIDLE\n'
    else
        cat "$AXSSH_TEST_ROOT/streams"
    fi
    ;;
subscribe)
    while :; do
        if [ -f "$AXSSH_TEST_ROOT/notify" ]; then
            rm "$AXSSH_TEST_ROOT/notify"
            printf "Event 'change' on source-output #1\n"
        fi
        sleep 0.02
    done
    ;;
esac
"#;
    for (name, script) in [("uname", "#!/bin/sh\necho Linux\n"), ("pactl", pactl)] {
        let path = root.join("bin").join(name);
        std::fs::write(&path, script).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let mut paths = vec![root.join("bin")];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let mut child = tokio::process::Command::new("sh")
        .args(["-c", &helper_command(source, AudioFormat::SPEECH).unwrap()])
        .env("PATH", std::env::join_paths(paths).unwrap())
        .env("TMPDIR", &root)
        .env("AXSSH_TEST_ROOT", &root)
        .env("AXSSH_TEST_IDLE", if stop_while_idle { "1" } else { "" })
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::fs::File::create(root.join("helper-stderr.log")).unwrap())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let ready = timeout(Duration::from_secs(10), lines.next_line()).await;
    assert!(
        ready.is_ok(),
        "helper startup: {}",
        std::fs::read_to_string(root.join("helper-stderr.log")).unwrap()
    );
    assert_eq!(ready.unwrap().unwrap().unwrap(), "AXSSH_AUDIO_READY");
    assert_eq!(
        timeout(Duration::from_secs(10), lines.next_line())
            .await
            .unwrap()
            .unwrap()
            .unwrap(),
        "AXSSH_AUDIO_DEMAND 0"
    );
    if stop_while_idle {
        // EOF cannot unblock a writer opening a FIFO with no reader. The
        // SSH shutdown signal must reclaim that process and its source.
        let signal = tokio::process::Command::new("kill")
            .args(["-TERM", &child.id().unwrap().to_string()])
            .status();
        assert!(
            timeout(Duration::from_secs(10), signal)
                .await
                .unwrap()
                .unwrap()
                .success()
        );
        assert!(
            timeout(Duration::from_secs(10), child.wait())
                .await
                .unwrap()
                .unwrap()
                .success()
        );
        assert_helper_cleanup(&root, external_default);
        return;
    }
    for streams in [
        "Source Output #9\n\tSource: 24\n\tCorked: no\n",
        "Source Output #9\n\tSource: 23\n\tCorked: yes\n",
    ] {
        std::fs::write(root.join("streams"), streams).unwrap();
        std::fs::write(root.join("notify"), "").unwrap();
        assert!(
            timeout(Duration::from_millis(100), lines.next_line())
                .await
                .is_err(),
            "foreign or paused streams must remain idle"
        );
    }
    // No program-name predicate: any standard client using this source works.
    std::fs::write(root.join("streams"), "Source Output #17\n\tSource: 23\n\tCorked: no\n\tProperties:\n\t\tapplication.name = arbitrary-recorder\n").unwrap();
    std::fs::write(root.join("notify"), "").unwrap();
    assert_eq!(
        timeout(Duration::from_secs(10), lines.next_line())
            .await
            .unwrap()
            .unwrap()
            .unwrap(),
        "AXSSH_AUDIO_DEMAND 1"
    );
    let data = vec![0x55; 4096];
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(&data)
        .await
        .unwrap();
    std::fs::write(root.join("streams"), "").unwrap();
    std::fs::write(root.join("notify"), "").unwrap();
    assert_eq!(
        timeout(Duration::from_secs(10), lines.next_line())
            .await
            .unwrap()
            .unwrap()
            .unwrap(),
        "AXSSH_AUDIO_DEMAND 0"
    );
    if external_default {
        std::fs::write(root.join("default"), "external").unwrap();
    }
    drop(child.stdin.take());
    let status = timeout(Duration::from_secs(10), child.wait())
        .await
        .unwrap()
        .unwrap();
    assert!(status.success());
    assert_eq!(std::fs::read(root.join("received")).unwrap(), data);
    assert_helper_cleanup(&root, external_default);
}

#[cfg(unix)]
fn assert_helper_cleanup(root: &std::path::Path, external_default: bool) {
    assert_eq!(
        std::fs::read_to_string(root.join("unloaded")).unwrap(),
        "123"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("default")).unwrap(),
        if external_default {
            "external"
        } else {
            "original"
        }
    );
    assert!(
        !std::fs::read_dir(root).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("axssh-audio.")),
        "owned FIFO directory must be removed"
    );
    std::fs::remove_dir_all(root).unwrap();
}

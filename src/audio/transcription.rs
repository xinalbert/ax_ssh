//! Local whisper.cpp adapter. Arguments are passed directly, without a shell.
use crate::media::PrivateMediaFile;
use anyhow::{Context, Result, ensure};
use std::ffi::OsString;
use std::io::Read;
use std::time::Duration;

pub async fn transcribe_pcm(
    recorded: Vec<u8>,
    executable: OsString,
    model: OsString,
) -> Result<String> {
    transcribe_with_timeout(recorded, executable, model, Duration::from_secs(120)).await
}

async fn transcribe_with_timeout(
    recorded: Vec<u8>,
    executable: OsString,
    model: OsString,
    limit: Duration,
) -> Result<String> {
    ensure!(
        !recorded.is_empty()
            && recorded.len().is_multiple_of(2)
            && recorded.len() <= 60 * super::SAMPLE_RATE as usize * 2,
        "invalid or oversized speech recording"
    );
    let owner = tokio::task::spawn_blocking(move || {
        let (owner, file) = PrivateMediaFile::create("wav")?;
        let mut writer = hound::WavWriter::new(
            file,
            hound::WavSpec {
                channels: 1,
                sample_rate: super::SAMPLE_RATE,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )?;
        for bytes in recorded.as_chunks::<2>().0 {
            writer.write_sample(i16::from_le_bytes(*bytes))?;
        }
        writer.finalize()?;
        Ok::<_, anyhow::Error>(owner)
    })
    .await
    .context("WAV worker failed")??;
    let output = owner.output_path();
    let mut child = tokio::process::Command::new(executable)
        .args(["-m"])
        .arg(model)
        .arg("-f")
        .arg(owner.path())
        .args(["-l", "auto", "-otxt", "-of"])
        .arg(&output)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .context("cannot start whisper-cli; check executable/model configuration")?;
    let status = tokio::time::timeout(limit, child.wait())
        .await
        .context("local transcription timed out")??;
    ensure!(
        status.success(),
        "whisper-cli failed ({status}); check model and engine"
    );
    tokio::task::spawn_blocking(move || {
        let mut text = String::new();
        std::fs::File::open(output.with_extension("txt"))?
            .take(64 * 1024 + 1)
            .read_to_string(&mut text)?;
        ensure!(text.len() <= 64 * 1024, "transcript exceeds 64 KiB");
        drop(owner);
        let text = text.trim().to_owned();
        ensure!(!text.is_empty(), "no speech recognized");
        Ok(text)
    })
    .await
    .context("transcript worker failed")?
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn fake_engine(script: &str) -> (PrivateMediaFile, OsString) {
        let (owner, _) = PrivateMediaFile::create("wav").unwrap();
        std::fs::write(owner.path(), script).unwrap();
        std::fs::set_permissions(owner.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let executable = owner.path().as_os_str().to_owned();
        (owner, executable)
    }

    #[tokio::test]
    async fn whisper_arguments_wav_and_unicode_text_round_trip() {
        let script = r#"#!/bin/sh
while [ "$#" -gt 0 ]; do
    case "$1" in
    -m) shift; [ "$1" = 'model with spaces $(literal)' ] || exit 2 ;;
    -f) shift; input=$1 ;;
    -l) shift; [ "$1" = auto ] || exit 3 ;;
    -of) shift; output=$1 ;;
    -otxt) ;;
    *) exit 4 ;;
    esac
    shift
done
[ "$(head -c 4 "$input")" = RIFF ] || exit 5
printf '你好，终端 agent。\n' > "$output.txt"
"#;
        let (_owner, executable) = fake_engine(script);
        let text = transcribe_pcm(
            vec![0; 3200],
            executable,
            "model with spaces $(literal)".into(),
        )
        .await
        .unwrap();
        assert_eq!(text, "你好，终端 agent。");
    }

    #[tokio::test]
    async fn whisper_timeout_invalid_pcm_and_large_output_are_rejected() {
        assert!(
            transcribe_pcm(vec![0], "missing".into(), "missing".into())
                .await
                .is_err()
        );
        let (_owner, executable) = fake_engine("#!/bin/sh\nexec sleep 5\n");
        assert!(
            transcribe_with_timeout(
                vec![0; 320],
                executable,
                "model".into(),
                Duration::from_millis(100)
            )
            .await
            .is_err()
        );
        let script = r#"#!/bin/sh
while [ "$#" -gt 0 ]; do
    if [ "$1" = -of ]; then shift; output=$1; fi
    shift
done
head -c 65537 /dev/zero | tr '\000' x > "$output.txt"
"#;
        let (_owner, executable) = fake_engine(script);
        assert!(
            transcribe_pcm(vec![0; 320], executable, "model".into())
                .await
                .is_err()
        );
    }
}

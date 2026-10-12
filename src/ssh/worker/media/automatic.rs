//! Standard Pulse recording streams drive capture, independently of CLI text.

use super::super::{SshSessionEvent, bounded_error_message};
use super::{close_audio_channel, extend_error};
use crate::audio::{self, AudioFormat};
use crate::ssh::SshConnection;
use anyhow::{Context, Result, ensure};
use russh::ChannelMsg;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinSet;
use tokio::time::{Duration, timeout};
use uuid::Uuid;

// Fixed POSIX program, with validated PCM numbers and a generated source name.
// Only source index and corked state matter; client names/commands are ignored.
const AUTOMATIC_AUDIO_HELPER: &str = r#"set -eu
export LC_ALL=C
[ "$(uname -s)" = Linux ] || exit 1
command -v pactl >/dev/null || exit 1
pactl info >/dev/null
umask 077
directory=$(mktemp -d "${TMPDIR:-/tmp}/axssh-audio.XXXXXXXX")
module=''
old=''
subscriber=''
monitor=''
writer=''
cleanup() {
    trap - EXIT HUP INT TERM
    for child in "$monitor" "$subscriber" "$writer"; do
        if [ -n "$child" ]; then kill "$child" 2>/dev/null || true; fi
    done
    for child in "$monitor" "$subscriber" "$writer"; do
        if [ -n "$child" ]; then wait "$child" 2>/dev/null || true; fi
    done
    if [ -n "$old" ] && [ "$(pactl get-default-source 2>/dev/null || true)" = "$source" ]; then
        pactl set-default-source "$old" || true
    fi
    if [ -n "$module" ]; then pactl unload-module "$module" || true; fi
    rm -f "$directory/input.pcm" "$directory/events"
    rmdir "$directory" || true
}
trap cleanup EXIT
trap 'exit 0' HUP INT TERM
mkfifo -m 600 "$directory/input.pcm" "$directory/events"
old=$(pactl get-default-source)
module=$(pactl load-module module-pipe-source file="$directory/input.pcm" source_name="$source" format=s16le rate="$rate" channels="$channels" source_properties=device.description=AxSSH_Microphone)
index=$(pactl list sources short | awk -v wanted="$source" '$2 == wanted {print $1; exit}')
case "$index" in ''|*[!0-9]*) exit 1 ;; esac
pactl set-default-source "$source"
printf 'AXSSH_AUDIO_READY\n'
notify_demand() {
    demand=$(pactl list source-outputs | awk -v wanted="$index" '
        function finish() { if (matching && !corked) active=1 }
        /^Source Output #[0-9]+/ { finish(); matching=0; corked=1 }
        $1 == "Source:" { matching=($2 == wanted) }
        $1 == "Corked:" { corked=($2 != "no") }
        END { finish(); print active ? "1" : "0" }
    ')
    if [ "$demand" != "$previous" ]; then
        printf 'AXSSH_AUDIO_DEMAND %s\n' "$demand"
        previous=$demand
    fi
}
pactl subscribe > "$directory/events" &
subscriber=$!
(
    previous=''
    notify_demand
    while IFS= read -r event; do
        case "$event" in *" on source-output "*) notify_demand ;; esac
    done < "$directory/events"
    # A dead subscription must not leave capture enabled indefinitely.
    printf 'AXSSH_AUDIO_DEMAND 0\n'
    kill -TERM "$parent"
) &
monitor=$!
exec 3<&0
cat <&3 > "$directory/input.pcm" &
writer=$!
exec 3<&-
wait "$writer"
"#;

#[derive(Default)]
struct DemandDecoder {
    line: Vec<u8>,
    ready: bool,
}

#[derive(Default)]
struct DemandUpdate {
    ready: bool,
    demand: Option<bool>,
}

impl DemandDecoder {
    fn push(&mut self, bytes: &[u8]) -> Result<DemandUpdate> {
        ensure!(bytes.len() <= 4096, "oversized audio control message");
        let mut update = DemandUpdate::default();
        for &byte in bytes {
            if byte == b'\n' {
                match self.line.as_slice() {
                    b"AXSSH_AUDIO_READY" if !self.ready => {
                        self.ready = true;
                        update.ready = true;
                    }
                    b"AXSSH_AUDIO_DEMAND 0" if self.ready => update.demand = Some(false),
                    b"AXSSH_AUDIO_DEMAND 1" if self.ready => update.demand = Some(true),
                    _ => anyhow::bail!("invalid audio control message"),
                }
                self.line.clear();
            } else {
                ensure!(self.line.len() < 64, "oversized audio control line");
                self.line.push(byte);
            }
        }
        Ok(update)
    }
}

fn helper_command(source: &str, format: AudioFormat) -> Result<String> {
    format.validate()?;
    ensure!(
        source.strip_prefix("axssh_").is_some_and(|id| {
            id.len() == 32 && id.bytes().all(|byte| byte.is_ascii_hexdigit())
        }),
        "invalid audio source identity"
    );
    let script = format!(
        "source='{source}'\nrate={}\nchannels={}\nparent=$$\n{AUTOMATIC_AUDIO_HELPER}",
        format.sample_rate, format.channels,
    );
    Ok(format!("exec sh -c '{}'", script.replace('\'', "'\\''")))
}

struct CaptureController {
    captures: JoinSet<Result<()>>,
    stop: Option<watch::Sender<bool>>,
    pcm: mpsc::Sender<Vec<u8>>,
    packets: mpsc::Receiver<Vec<u8>>,
}

impl CaptureController {
    fn new() -> Self {
        let (pcm, packets) = mpsc::channel(audio::PCM_QUEUE_PACKETS);
        Self {
            captures: JoinSet::new(),
            stop: None,
            pcm,
            packets,
        }
    }

    async fn set_demand<F, Fut>(&mut self, active: bool, format: AudioFormat, capture: F)
    where
        F: FnOnce(watch::Receiver<bool>, AudioFormat, mpsc::Sender<Vec<u8>>) -> Fut,
        Fut: std::future::Future<Output = Result<()>> + Send + 'static,
    {
        if active && self.stop.is_none() {
            while self.packets.try_recv().is_ok() {}
            let (stop, stopped) = watch::channel(false);
            self.stop = Some(stop);
            self.captures
                .spawn(capture(stopped, format, self.pcm.clone()));
        } else if !active {
            stop_capture(&mut self.stop, &mut self.captures).await;
            while self.packets.try_recv().is_ok() {}
        }
    }
}

async fn capture_pcm(
    stop: watch::Receiver<bool>,
    format: AudioFormat,
    pcm: mpsc::Sender<Vec<u8>>,
) -> Result<()> {
    let (started, startup) = oneshot::channel();
    let result = audio::capture(stop, format, pcm, started).await;
    drop(startup);
    result
}

pub(in crate::ssh::worker) async fn forward_automatic_microphone(
    connection: SshConnection,
    format: AudioFormat,
    mut shutdown: watch::Receiver<bool>,
    ready: oneshot::Sender<Result<String>>,
    events: mpsc::Sender<SshSessionEvent>,
) -> Result<()> {
    let source = format!("axssh_{}", Uuid::new_v4().simple());
    let command = helper_command(&source, format)?;
    if *shutdown.borrow() {
        return Ok(());
    }
    let mut channel = connection.open_auxiliary_channel().await?;
    let mut ready = Some(ready);
    let mut capture = CaptureController::new();
    let mut decoder = DemandDecoder::default();
    let mut stderr = Vec::new();
    let mut recording_requested = false;
    let mut helper_started = false;
    let result = async {
        timeout(Duration::from_secs(2), channel.exec(true, command))
            .await.context("audio helper command timed out")??;
        loop {
            tokio::select! {
                biased;
                _ = audio::wait_for_stop(&mut shutdown) => break Ok(()),
                message = channel.wait() => match message {
                    Some(ChannelMsg::Success) => helper_started = true,
                    Some(ChannelMsg::Failure) => anyhow::bail!("remote server rejected the standard audio helper"),
                    Some(ChannelMsg::Data { data }) => {
                        let update = decoder.push(&data)?;
                        if update.ready && ready.take().is_some_and(|reply| reply.send(Ok(source.clone())).is_err()) {
                            break Ok(());
                        }
                        if let Some(active) = update.demand {
                            recording_requested |= active;
                            capture.set_demand(active, format, capture_pcm).await;
                        }
                    }
                    Some(ChannelMsg::ExtendedData { data, .. }) => extend_error(&mut stderr, &data),
                    Some(ChannelMsg::ExitStatus { exit_status }) => {
                        ensure!(exit_status == 0, "audio helper exited ({exit_status}): {}", String::from_utf8_lossy(&stderr));
                        break Ok(());
                    }
                    Some(ChannelMsg::Eof | ChannelMsg::Close) | None => break Ok(()),
                    _ => {}
                },
                finished = capture.captures.join_next(), if !capture.captures.is_empty() => {
                    capture.stop = None;
                    match finished {
                        Some(Ok(result)) => result.context("automatic microphone capture failed")?,
                        Some(Err(error)) => return Err(error.into()),
                        None => {}
                    }
                    break Ok(());
                }
                packet = capture.packets.recv(), if capture.stop.is_some() => {
                    let packet = packet.context("microphone PCM queue closed")?;
                    ensure!(packet.len() == format.packet_bytes(), "invalid microphone PCM packet");
                    timeout(Duration::from_secs(2), channel.data(packet.as_slice()))
                        .await.context("remote audio consumer is too slow")??;
                }
            }
        }
    }.await;
    stop_capture(&mut capture.stop, &mut capture.captures).await;
    if helper_started {
        close_audio_channel(&mut channel).await;
    } else {
        match timeout(Duration::from_secs(1), channel.close()).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => tracing::debug!(%error, "cannot close rejected audio channel"),
            Err(_) => tracing::debug!("rejected audio channel close timed out"),
        }
    }
    if recording_requested && let Err(error) = &result {
        match timeout(
            Duration::from_secs(1),
            events.send(SshSessionEvent::MicrophoneUnavailable(
                bounded_error_message(error),
            )),
        )
        .await
        {
            Ok(Ok(())) => {}
            Ok(Err(_)) => tracing::debug!("microphone error recipient closed"),
            Err(_) => tracing::warn!("microphone error delivery timed out"),
        }
    }
    if let Some(ready) = ready {
        let message = result
            .as_ref()
            .err()
            .map(ToString::to_string)
            .unwrap_or_else(|| "standard audio source is unavailable".to_owned());
        if ready.send(Err(anyhow::anyhow!(message))).is_err() {
            tracing::debug!("automatic audio setup requester cancelled");
        }
    }
    result
}

async fn stop_capture(stop: &mut Option<watch::Sender<bool>>, captures: &mut JoinSet<Result<()>>) {
    if let Some(stop) = stop.take() {
        stop.send_replace(true);
    }
    if timeout(Duration::from_secs(1), async {
        while let Some(result) = captures.join_next().await {
            match result {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    tracing::debug!(%error, "microphone capture stopped with an error")
                }
                Err(error) if error.is_cancelled() => {}
                Err(error) => tracing::warn!(%error, "microphone capture task failed"),
            }
        }
    })
    .await
    .is_err()
    {
        captures.abort_all();
        while captures.join_next().await.is_some() {}
        tracing::warn!("microphone capture shutdown timed out");
    }
}

#[cfg(test)]
mod tests;

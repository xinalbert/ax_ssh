//! User-initiated media operations and owned UI snapshots.

use super::*;
use anyhow::ensure;
use ax_ssh::{
    audio,
    media::{self, PrivateMediaFile},
};
use std::io::Write;
use tokio::sync::{mpsc, oneshot, watch};

pub(super) const AUDIO_RATES: [u32; 9] = [
    8000, 16000, 22050, 24000, 32000, 44100, 48000, 96000, 192000,
];
pub(super) const AUDIO_CHANNELS: [u16; 5] = [1, 2, 4, 6, 8];

pub(super) fn wire_settings(
    ui: &AppWindow,
    state: Arc<Mutex<AppState>>,
    runtime: tokio::runtime::Handle,
) {
    let weak = ui.as_weak();
    ui.on_save_audio_settings(move |rate, channels, executable, model| {
        let selected = usize::try_from(rate)
            .ok()
            .and_then(|index| AUDIO_RATES.get(index))
            .copied()
            .zip(
                usize::try_from(channels)
                    .ok()
                    .and_then(|index| AUDIO_CHANNELS.get(index))
                    .copied(),
            );
        let Some((sample_rate, channels)) = selected else {
            set_status(&weak, "Invalid audio format");
            return;
        };
        if executable.len() > 4096
            || model.len() > 4096
            || executable.contains('\0')
            || model.contains('\0')
        {
            set_status(&weak, "Invalid transcription paths");
            return;
        }
        let settings = ax_ssh::config::AudioSettings {
            sample_rate,
            channels,
            whisper_executable: executable.trim().into(),
            whisper_model: model.trim().into(),
        };
        let state = state.clone();
        let weak = weak.clone();
        let Some(coordinator) = state
            .lock()
            .ok()
            .map(|app| app.persistence_coordinator.clone())
        else {
            return;
        };
        runtime.spawn(async move {
            let _guard = coordinator.gate.lock().await;
            let state_for_save = state.clone();
            let result = tokio::task::spawn_blocking(move || {
                let mut app = state_for_save
                    .lock()
                    .map_err(|_| anyhow::anyhow!("state unavailable"))?;
                let mut candidate = app.sessions.clone();
                candidate.settings.audio = settings;
                app.config.save(&candidate)?;
                app.sessions = candidate;
                Ok::<_, anyhow::Error>(app.sessions.settings.clone())
            })
            .await;
            match result {
                Ok(Ok(settings)) => dispatch_ui(&weak, move |ui| {
                    super::view::apply_settings_to_open_windows(ui, &settings);
                    ui.set_status("Audio settings saved".into());
                }),
                Ok(Err(error)) => {
                    set_status(&weak, &format!("Cannot save audio settings: {error}"))
                }
                Err(error) => set_status(&weak, &format!("Audio settings worker failed: {error}")),
            }
            refresh_workspace(&weak, &state);
        });
    });
}

#[derive(Default)]
pub(super) struct MediaState {
    job: Option<MediaJob>,
    image_token: Option<Uuid>,
    files: Vec<BackgroundFile>,
    status: String,
    transcript: String,
}

struct BackgroundFile {
    file: Option<PrivateMediaFile>,
    runtime: tokio::runtime::Handle,
}
impl Drop for BackgroundFile {
    fn drop(&mut self) {
        if let Some(file) = self.file.take() {
            self.runtime.spawn_blocking(move || drop(file));
        }
    }
}

struct MediaJob {
    id: Uuid,
    mode: &'static str,
    stop: watch::Sender<bool>,
    task: Option<tokio::task::JoinHandle<()>>,
}
impl Drop for MediaJob {
    fn drop(&mut self) {
        self.stop.send_replace(true);
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

#[derive(Clone, Default)]
pub(super) struct MediaSnapshot {
    pub(super) mode: &'static str,
    pub(super) status: String,
    pub(super) transcript: String,
}
impl MediaState {
    pub(super) fn snapshot(&self) -> MediaSnapshot {
        MediaSnapshot {
            mode: self.job.as_ref().map(|job| job.mode).unwrap_or(""),
            status: self.status.clone(),
            transcript: self.transcript.clone(),
        }
    }
    pub(super) fn has_job(&self) -> bool {
        self.job.is_some()
    }

    pub(super) fn cancel(&mut self) {
        self.job = None;
        self.image_token = None;
        self.transcript.clear();
        self.status.clear();
        self.files.clear();
    }
}

pub(super) fn handle_command(
    tab_id: Uuid,
    command: &str,
    state: &Arc<Mutex<AppState>>,
    ui: &slint::Weak<AppWindow>,
    runtime: &tokio::runtime::Handle,
    router: &WindowRouter,
    window_id: Uuid,
) -> bool {
    let owns = state
        .lock()
        .ok()
        .is_some_and(|app| router.owns_terminal_pane(window_id, tab_id, &app));
    if !owns {
        return false;
    }
    if command == "discard-transcript" {
        if let Ok(mut app) = state.lock()
            && let Some(terminal) = app.terminal_mut(tab_id)
        {
            terminal.media.transcript.clear();
        }
        refresh_workspace(ui, state);
        return true;
    }
    if let Some(text) = command.strip_prefix("insert-transcript\n") {
        // Speech segments often contain line breaks. Terminals without
        // bracketed paste interpret those as Return, so insert one line.
        let text = single_line_transcript(text);
        let result = paste_text(state, tab_id, &text);
        if result.is_ok() {
            if let Ok(mut app) = state.lock()
                && let Some(terminal) = app.terminal_mut(tab_id)
            {
                terminal.media.transcript.clear();
            }
            refresh_workspace(ui, state);
        }
        return report(ui, result);
    }
    if command == "paste-clipboard" {
        return paste_clipboard(tab_id, state, ui, runtime);
    }
    if let Ok(mut app) = state.lock()
        && let Some(terminal) = app.terminal_mut(tab_id)
        && let Some(job) = terminal.media.job.as_mut()
    {
        if job.mode == "transcribing" {
            terminal.media.job = None;
            terminal.media.status = "Transcription cancelled".into();
        } else {
            job.stop.send_replace(true);
            terminal.media.status = "Stopping microphone…".into();
        }
        drop(app);
        refresh_workspace(ui, state);
        return true;
    }
    start_audio(tab_id, command, state, ui, runtime)
}

fn report(ui: &slint::Weak<AppWindow>, result: Result<()>) -> bool {
    match result {
        Ok(()) => true,
        Err(error) => {
            set_status(ui, &format!("Media operation failed: {error}"));
            false
        }
    }
}

fn single_line_transcript(text: &str) -> String {
    text.replace("\r\n", " ").replace(['\r', '\n'], " ")
}

fn paste_text(state: &Arc<Mutex<AppState>>, tab_id: Uuid, text: &str) -> Result<()> {
    ensure!(
        text.len() <= 4 * 1024 * 1024,
        "clipboard text exceeds 4 MiB"
    );
    let mut app = state
        .lock()
        .map_err(|_| anyhow::anyhow!("cannot access terminal state"))?;
    let terminal = app.terminal_mut(tab_id).context("terminal closed")?;
    ensure!(
        terminal.connected && terminal.worker_running,
        "terminal is disconnected"
    );
    let data = terminal
        .terminal
        .as_mut()
        .context("terminal unavailable")?
        .encode_paste(text)
        .context("empty paste")?;
    terminal
        .worker
        .as_ref()
        .context("terminal worker stopped")?
        .request_send_paste(data)
}

enum ClipboardContent {
    Text(String),
    Image(arboard::ImageData<'static>),
}
fn read_clipboard() -> Result<ClipboardContent> {
    with_clipboard(|clipboard| match clipboard.get_image() {
        Ok(image) => {
            ensure!(
                image.width > 0
                    && image.height > 0
                    && image.width <= media::MAX_IMAGE_SIDE
                    && image.height <= media::MAX_IMAGE_SIDE,
                "clipboard image exceeds 4096 × 4096"
            );
            ensure!(
                image.bytes.len() == image.width * image.height * 4,
                "invalid clipboard image"
            );
            Ok(ClipboardContent::Image(image))
        }
        Err(arboard::Error::ContentNotAvailable) => {
            Ok(ClipboardContent::Text(clipboard.get_text()?))
        }
        Err(error) => Err(error.into()),
    })
}

fn paste_clipboard(
    tab_id: Uuid,
    state: &Arc<Mutex<AppState>>,
    ui: &slint::Weak<AppWindow>,
    runtime: &tokio::runtime::Handle,
) -> bool {
    paste_clipboard_with(tab_id, state, ui, runtime, read_clipboard)
}

fn paste_clipboard_with(
    tab_id: Uuid,
    state: &Arc<Mutex<AppState>>,
    ui: &slint::Weak<AppWindow>,
    runtime: &tokio::runtime::Handle,
    read: impl FnOnce() -> Result<ClipboardContent>,
) -> bool {
    let content = match read() {
        Ok(content) => content,
        Err(error) => return report(ui, Err(error)),
    };
    if let ClipboardContent::Text(text) = content {
        return report(ui, paste_text(state, tab_id, &text));
    }
    let ClipboardContent::Image(image) = content else {
        return false;
    };
    let token = Uuid::new_v4();
    let generation = match state.lock() {
        Ok(mut app) => {
            let Some(terminal) = app.terminal_mut(tab_id) else {
                return false;
            };
            if !terminal.connected || terminal.media.image_token.is_some() {
                return report(
                    ui,
                    Err(anyhow::anyhow!(
                        "terminal disconnected or another image is uploading"
                    )),
                );
            }
            if !matches!(
                terminal.worker,
                Some(TerminalWorker::Ssh(_) | TerminalWorker::Local(_))
            ) {
                return report(
                    ui,
                    Err(anyhow::anyhow!(
                        "image attachments require SSH or Local Shell"
                    )),
                );
            }
            if terminal.media.files.len() >= media::MAX_ATTACHMENTS {
                return report(
                    ui,
                    Err(anyhow::anyhow!("session attachment limit reached (32)")),
                );
            }
            terminal.media.image_token = Some(token);
            terminal.media.status = "Preparing clipboard image…".into();
            terminal.reconnect_generation()
        }
        Err(_) => return false,
    };
    refresh_workspace(ui, state);
    let state = state.clone();
    let ui = ui.clone();
    let handle = runtime.clone();
    runtime.spawn(async move {
        let result = prepare_image(&state, tab_id, generation, token, image).await;
        match result {
            Ok((path, file)) => {
                let current = state.lock().ok().is_some_and(|mut app| {
                    let Some(terminal) = app.terminal_mut(tab_id) else {
                        return false;
                    };
                    if terminal.reconnect_generation() != generation
                        || terminal.media.image_token != Some(token)
                        || !terminal.connected
                    {
                        return false;
                    }
                    if let Some(file) = file {
                        terminal.media.files.push(BackgroundFile {
                            file: Some(file),
                            runtime: handle,
                        });
                    }
                    terminal.media.image_token = None;
                    terminal.media.status = format!("Image ready: {path}");
                    true
                });
                if current {
                    report(&ui, paste_text(&state, tab_id, &attachment_text(&path)));
                }
            }
            Err(error) => {
                let current = if let Ok(mut app) = state.lock()
                    && let Some(terminal) = app.terminal_mut(tab_id)
                    && terminal.media.image_token == Some(token)
                {
                    terminal.media.image_token = None;
                    terminal.media.status = format!("Image failed: {error}");
                    true
                } else {
                    false
                };
                if current {
                    report(&ui, Err(error));
                }
            }
        }
        refresh_workspace(&ui, &state);
    });
    true
}

fn attachment_text(path: &str) -> String {
    // Valid for shell commands and path-aware CLI paste, including whitespace.
    if !path
        .chars()
        .all(|c| c.is_alphanumeric() || "/._-".contains(c))
    {
        format!("'{}'", path.replace('\'', "'\\''"))
    } else {
        path.into()
    }
}

async fn prepare_image(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    generation: u64,
    token: Uuid,
    image: arboard::ImageData<'static>,
) -> Result<(String, Option<PrivateMediaFile>)> {
    let png = tokio::task::spawn_blocking(move || {
        media::encode_clipboard_png(image.width, image.height, &image.bytes)
    })
    .await
    .context("PNG worker failed")??;
    let request = {
        let app = state
            .lock()
            .map_err(|_| anyhow::anyhow!("state unavailable"))?;
        let terminal = app.terminal(tab_id).context("terminal closed")?;
        ensure!(
            terminal.reconnect_generation() == generation
                && terminal.media.image_token == Some(token)
                && terminal.connected,
            "image operation cancelled"
        );
        match terminal
            .worker
            .as_ref()
            .context("terminal worker stopped")?
        {
            TerminalWorker::Ssh(worker) => Some(worker.request_clipboard_image(png.clone())?),
            TerminalWorker::Local(_) => None,
            _ => anyhow::bail!("image attachments require SSH or Local Shell"),
        }
    };
    if let Some(request) = request {
        return Ok((
            request.await.context("SSH attachment worker stopped")??,
            None,
        ));
    }
    tokio::task::spawn_blocking(move || {
        let (owner, mut file) = PrivateMediaFile::create("png")?;
        file.write_all(&png).context("cannot save clipboard PNG")?;
        file.flush()?;
        Ok((owner.path().to_string_lossy().into_owned(), Some(owner)))
    })
    .await
    .context("image file worker failed")?
}

fn start_audio(
    tab_id: Uuid,
    command: &str,
    state: &Arc<Mutex<AppState>>,
    ui: &slint::Weak<AppWindow>,
    runtime: &tokio::runtime::Handle,
) -> bool {
    let id = Uuid::new_v4();
    let (stop, stopped) = watch::channel(false);
    let (pcm, packets) = mpsc::channel(audio::PCM_QUEUE_PACKETS);
    let format = state
        .lock()
        .ok()
        .map(|app| app.sessions.settings.audio.format())
        .unwrap_or(audio::AudioFormat::SPEECH);
    let remote = match state.lock() {
        Ok(mut app) => {
            if app.microphone_in_use() {
                return report(
                    ui,
                    Err(anyhow::anyhow!(
                        "stop the current microphone or transcription first"
                    )),
                );
            }
            let Some(terminal) = app.terminal_mut(tab_id) else {
                return false;
            };
            if !terminal.connected {
                return report(ui, Err(anyhow::anyhow!("terminal disconnected")));
            }
            let remote = if command == "microphone" {
                let Some(TerminalWorker::Ssh(worker)) = terminal.worker.as_ref() else {
                    return report(
                        ui,
                        Err(anyhow::anyhow!("remote microphone requires SSH to Linux")),
                    );
                };
                match worker.request_microphone(format, packets, stopped.clone()) {
                    Ok(remote) => Some(remote),
                    Err(error) => return report(ui, Err(error)),
                }
            } else {
                None
            };
            terminal.media.job = Some(MediaJob {
                id,
                mode: if remote.is_some() {
                    "microphone"
                } else {
                    "dictation"
                },
                stop: stop.clone(),
                task: None,
            });
            terminal.media.status = "Starting microphone…".into();
            remote
        }
        Err(_) => return false,
    };
    let state = state.clone();
    let ui = ui.clone();
    let state_for_job = state.clone();
    let ui_for_job = ui.clone();
    // Dictation owns a separate receiver; remote mode moved its receiver into SSH.
    let task = runtime.spawn(async move {
        let result = if let Some(remote) = remote {
            run_remote_audio(
                &state_for_job,
                &ui_for_job,
                (tab_id, id),
                format,
                pcm,
                stopped,
                remote,
            )
            .await
            .map(|()| None)
        } else {
            // This branch receives dictation through its own bounded capture below.
            drop(pcm);
            run_dictation(&state_for_job, &ui_for_job, tab_id, id, stop, stopped)
                .await
                .map(Some)
        };
        if let Ok(mut app) = state_for_job.lock()
            && let Some(terminal) = app.terminal_mut(tab_id)
            && terminal.media.job.as_ref().is_some_and(|job| job.id == id)
        {
            if let Some(mut job) = terminal.media.job.take() {
                job.task.take();
            }
            match result {
                Ok(Some(transcript)) => {
                    terminal.media.transcript = transcript;
                    terminal.media.status = "Review the transcript before inserting".into();
                }
                Ok(None) => terminal.media.status = "Microphone stopped".into(),
                Err(error) => terminal.media.status = format!("Microphone failed: {error}"),
            }
        }
        refresh_workspace(&ui_for_job, &state_for_job);
    });
    let mut task = Some(task);
    if let Ok(mut app) = state.lock()
        && let Some(terminal) = app.terminal_mut(tab_id)
        && let Some(job) = terminal.media.job.as_mut()
        && job.id == id
    {
        job.task = task.take();
    }
    if let Some(task) = task {
        task.abort();
    }
    refresh_workspace(&ui, &state);
    true
}

fn audio_status(
    state: &Arc<Mutex<AppState>>,
    ui: &slint::Weak<AppWindow>,
    tab_id: Uuid,
    id: Uuid,
    message: &str,
) {
    if let Ok(mut app) = state.lock()
        && let Some(terminal) = app.terminal_mut(tab_id)
        && terminal.media.job.as_ref().is_some_and(|job| job.id == id)
    {
        terminal.media.status = message.into();
    }
    refresh_workspace(ui, state);
}

async fn run_remote_audio(
    state: &Arc<Mutex<AppState>>,
    ui: &slint::Weak<AppWindow>,
    identity: (Uuid, Uuid),
    format: audio::AudioFormat,
    pcm: mpsc::Sender<Vec<u8>>,
    stopped: watch::Receiver<bool>,
    remote: ax_ssh::ssh::MicrophoneRequest,
) -> Result<()> {
    let (tab_id, id) = identity;
    let source = remote
        .ready
        .await
        .context("remote microphone worker stopped")??;
    let (ready, started) = oneshot::channel();
    let capture = audio::capture(stopped, format, pcm, ready);
    tokio::pin!(capture);
    let mut finished = remote.finished;
    tokio::select! {
        result = started => { result.context("microphone startup stopped")??; }
        result = &mut capture => { return result; }
        result = &mut finished => { return result.context("remote microphone stopped")?; }
    }
    audio_status(
        state,
        ui,
        tab_id,
        id,
        &format!("Microphone live → {source}"),
    );
    tokio::select! {
        result = &mut capture => { result?; finished.await.context("remote microphone shutdown failed")? }
        result = &mut finished => result.context("remote microphone stopped")?,
    }
}

async fn run_dictation(
    state: &Arc<Mutex<AppState>>,
    ui: &slint::Weak<AppWindow>,
    tab_id: Uuid,
    id: Uuid,
    stop: watch::Sender<bool>,
    stopped: watch::Receiver<bool>,
) -> Result<String> {
    let settings = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state unavailable"))?
        .sessions
        .settings
        .audio
        .clone();
    let executable = (!settings.whisper_executable.is_empty())
        .then(|| std::ffi::OsString::from(&settings.whisper_executable))
        .or_else(|| std::env::var_os("AXSSH_WHISPER_CLI"))
        .context("configure whisper-cli in Settings → Terminal → Microphone & Dictation")?;
    let model = (!settings.whisper_model.is_empty())
        .then(|| std::ffi::OsString::from(&settings.whisper_model))
        .or_else(|| std::env::var_os("AXSSH_WHISPER_MODEL"))
        .context("configure a multilingual Whisper model in Settings → Terminal")?;
    let (pcm, mut packets) = mpsc::channel(audio::PCM_QUEUE_PACKETS);
    let (ready, started) = oneshot::channel();
    let capture = audio::capture(stopped, audio::AudioFormat::SPEECH, pcm, ready);
    tokio::pin!(capture);
    tokio::select! {
        result = started => { result.context("microphone startup stopped")??; }
        result = &mut capture => { result?; anyhow::bail!("recording cancelled"); }
    }
    audio_status(
        state,
        ui,
        tab_id,
        id,
        "Recording voice (maximum 60 seconds)…",
    );
    let mut recorded = Vec::new();
    let deadline = tokio::time::sleep(Duration::from_secs(60));
    tokio::pin!(deadline);
    let mut time_limit_reached = false;
    loop {
        tokio::select! {
            result = &mut capture => { result?; break; }
            _ = &mut deadline, if !time_limit_reached => { time_limit_reached = true; stop.send_replace(true); }
            packet = packets.recv() => {
                let Some(packet) = packet else { capture.await?; break; };
                recorded.extend(packet);
                if recorded.len() >= 60 * audio::SAMPLE_RATE as usize * 2 { stop.send_replace(true); }
            }
        }
    }
    while let Ok(packet) = packets.try_recv() {
        recorded.extend(packet);
    }
    ensure!(!recorded.is_empty(), "no speech recorded");
    recorded.truncate(60 * audio::SAMPLE_RATE as usize * 2);
    if let Ok(mut app) = state.lock()
        && let Some(terminal) = app.terminal_mut(tab_id)
        && let Some(job) = terminal.media.job.as_mut()
        && job.id == id
    {
        job.mode = "transcribing";
    }
    audio_status(state, ui, tab_id, id, "Transcribing locally…");
    audio::transcribe_pcm(recorded, executable, model).await
}

#[cfg(test)]
mod tests;

//! Bounded local pseudo-terminal discovery and worker lifecycle.

use std::collections::BTreeSet;
use std::env;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{Context, Result};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use tokio::sync::mpsc;
use tokio::time::timeout;

pub const SYSTEM_SHELL: &str = "System default";

const COMMAND_CAPACITY: usize = 32;
const EVENT_CAPACITY: usize = 32;
const MAX_INPUT_BYTES: usize = 16 * 1024;
const MAX_OUTPUT_BATCH_BYTES: usize = 16 * 1024;
const MAX_COLUMNS: u32 = 300;
const MAX_ROWS: u32 = 100;
const MAX_ERROR_CHARS: usize = 512;
const COMMAND_POLL_INTERVAL: Duration = Duration::from_millis(25);
const WORKER_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(7);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocalShellEvent {
    Started { shell: String },
    Resized { columns: u32, rows: u32 },
    Output(Vec<u8>),
    Exited { status: String },
    Failed(String),
}

enum LocalShellCommand {
    Send(Vec<u8>),
    Wake,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TerminalSize {
    columns: u32,
    rows: u32,
}

/// UI-adjacent controller for one worker-owned local PTY process.
pub struct LocalShellHandle {
    command_tx: SyncSender<LocalShellCommand>,
    pending_resize: Arc<Mutex<Option<TerminalSize>>>,
    shutdown_requested: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl LocalShellHandle {
    pub fn spawn(
        shell: String,
        columns: u32,
        rows: u32,
    ) -> (Self, mpsc::Receiver<LocalShellEvent>) {
        let initial_size = TerminalSize {
            columns: columns.clamp(1, MAX_COLUMNS),
            rows: rows.clamp(1, MAX_ROWS),
        };
        let (command_tx, command_rx) = sync_channel(COMMAND_CAPACITY);
        let (event_tx, event_rx) = mpsc::channel(EVENT_CAPACITY);
        let pending_resize = Arc::new(Mutex::new(None));
        let shutdown_requested = Arc::new(AtomicBool::new(false));
        let worker_resize = pending_resize.clone();
        let worker_shutdown = shutdown_requested.clone();
        let worker_event_tx = event_tx.clone();
        let thread = thread::Builder::new()
            .name("axssh-local-pty".to_owned())
            .spawn(move || {
                if let Err(error) = run_local_shell(
                    &shell,
                    initial_size,
                    command_rx,
                    worker_resize,
                    worker_shutdown,
                    &worker_event_tx,
                ) {
                    let _ = worker_event_tx
                        .blocking_send(LocalShellEvent::Failed(bounded_error(&error)));
                }
            });

        let thread = match thread {
            Ok(thread) => Some(thread),
            Err(error) => {
                let _ = event_tx.try_send(LocalShellEvent::Failed(bounded_text(format!(
                    "cannot start local PTY worker thread: {error}"
                ))));
                None
            }
        };
        (
            Self {
                command_tx,
                pending_resize,
                shutdown_requested,
                thread,
            },
            event_rx,
        )
    }

    pub fn is_finished(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }

    pub fn request_disconnect(&self) -> Result<()> {
        self.shutdown_requested.store(true, Ordering::Release);
        wake_worker(&self.command_tx)
    }

    pub fn request_send(&self, data: Vec<u8>) -> Result<()> {
        if data.is_empty() {
            return Ok(());
        }
        if data.len() > MAX_INPUT_BYTES {
            anyhow::bail!("terminal input cannot exceed {MAX_INPUT_BYTES} bytes");
        }
        self.command_tx
            .try_send(LocalShellCommand::Send(data))
            .map_err(|error| anyhow::anyhow!("cannot queue local terminal input: {error}"))
    }

    pub fn request_resize(&self, columns: u32, rows: u32) -> Result<()> {
        validate_terminal_size(columns, rows)?;
        *self
            .pending_resize
            .lock()
            .map_err(|_| anyhow::anyhow!("local terminal resize state lock poisoned"))? =
            Some(TerminalSize { columns, rows });
        wake_worker(&self.command_tx)
    }

    pub async fn shutdown(mut self) -> Result<()> {
        self.shutdown_requested.store(true, Ordering::Release);
        let _ = wake_worker(&self.command_tx);
        let Some(thread) = self.thread.take() else {
            return Ok(());
        };
        let join = tokio::task::spawn_blocking(move || thread.join());
        match timeout(WORKER_SHUTDOWN_TIMEOUT, join).await {
            Ok(joined) => joined
                .context("local PTY join task failed")?
                .map_err(|panic| {
                    anyhow::anyhow!("local PTY worker panicked: {}", panic_message(panic))
                }),
            Err(_) => anyhow::bail!("local PTY worker exceeded shutdown timeout"),
        }
    }
}

/// Returns platform-appropriate shell names that currently resolve on `PATH`.
pub fn discover_shells() -> Vec<String> {
    let mut candidates = platform_shell_candidates();
    if let Some(shell) = platform_default_shell()
        && let Some(name) = Path::new(&shell).file_name().and_then(|name| name.to_str())
    {
        candidates.insert(0, name.to_owned());
    }

    let mut seen = BTreeSet::new();
    candidates
        .into_iter()
        .filter(|candidate| seen.insert(candidate.to_ascii_lowercase()))
        .filter(|candidate| find_executable(candidate).is_some())
        .collect()
}

pub fn resolve_shell(selection: &str) -> Result<PathBuf> {
    let selection = selection.trim();
    if selection.is_empty() || selection.eq_ignore_ascii_case(SYSTEM_SHELL) {
        if let Some(shell) = platform_default_shell() {
            let path = PathBuf::from(&shell);
            if is_executable(&path) {
                return Ok(path);
            }
            if let Some(path) = find_executable(&shell) {
                return Ok(path);
            }
        }
        return default_shell_fallback()
            .and_then(|shell| find_executable(shell))
            .context("no platform default shell is available");
    }
    if selection.chars().count() > 256 || selection.chars().any(char::is_control) {
        anyhow::bail!("configured local shell is invalid");
    }
    find_executable(selection)
        .with_context(|| format!("configured local shell `{selection}` is not available"))
}

fn run_local_shell(
    shell: &str,
    initial_size: TerminalSize,
    command_rx: Receiver<LocalShellCommand>,
    pending_resize: Arc<Mutex<Option<TerminalSize>>>,
    shutdown_requested: Arc<AtomicBool>,
    event_tx: &mpsc::Sender<LocalShellEvent>,
) -> Result<()> {
    let shell_path = resolve_shell(shell)?;
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(pty_size(initial_size))
        .context("failed to open local pseudo-terminal")?;
    let mut command = CommandBuilder::new(&shell_path);
    command.env("TERM", "xterm-256color");
    let mut child = pair
        .slave
        .spawn_command(command)
        .with_context(|| format!("failed to start local shell {}", shell_path.display()))?;
    drop(pair.slave);
    let mut writer = pair
        .master
        .take_writer()
        .context("failed to open local PTY input")?;
    let reader = pair
        .master
        .try_clone_reader()
        .context("failed to open local PTY output")?;
    let reader_tx = event_tx.clone();
    let reader_thread = thread::Builder::new()
        .name("axssh-local-pty-reader".to_owned())
        .spawn(move || read_output(reader, &reader_tx))
        .context("failed to start local PTY reader")?;

    let outcome = drive_local_shell(
        shell_path.display().to_string(),
        child.as_mut(),
        writer.as_mut(),
        pair.master.as_ref(),
        &command_rx,
        &pending_resize,
        &shutdown_requested,
        event_tx,
    );
    if !matches!(outcome, Ok(Some(_))) {
        let _ = child.kill();
        let _ = child.wait();
    }
    drop(writer);
    drop(pair.master);
    let _ = reader_thread.join();
    if let Some(status) = outcome? {
        let _ = event_tx.blocking_send(LocalShellEvent::Exited { status });
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn drive_local_shell(
    shell: String,
    child: &mut dyn portable_pty::Child,
    writer: &mut dyn Write,
    master: &(dyn portable_pty::MasterPty + Send),
    command_rx: &Receiver<LocalShellCommand>,
    pending_resize: &Arc<Mutex<Option<TerminalSize>>>,
    shutdown_requested: &Arc<AtomicBool>,
    event_tx: &mpsc::Sender<LocalShellEvent>,
) -> Result<Option<String>> {
    event_tx
        .blocking_send(LocalShellEvent::Started { shell })
        .context("local shell event receiver closed during startup")?;
    while !shutdown_requested.load(Ordering::Acquire) {
        apply_pending_resize(master, pending_resize, event_tx)?;
        match child.try_wait().context("failed to poll local shell")? {
            Some(status) => return Ok(Some(status.to_string())),
            None => {}
        }
        match command_rx.recv_timeout(COMMAND_POLL_INTERVAL) {
            Ok(LocalShellCommand::Send(data)) => {
                writer
                    .write_all(&data)
                    .and_then(|_| writer.flush())
                    .context("failed to write local PTY input")?;
            }
            Ok(LocalShellCommand::Wake) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                shutdown_requested.store(true, Ordering::Release);
            }
        }
    }
    Ok(None)
}

fn apply_pending_resize(
    master: &(dyn portable_pty::MasterPty + Send),
    pending_resize: &Arc<Mutex<Option<TerminalSize>>>,
    event_tx: &mpsc::Sender<LocalShellEvent>,
) -> Result<()> {
    let size = pending_resize
        .lock()
        .map_err(|_| anyhow::anyhow!("local terminal resize state lock poisoned"))?
        .take();
    let Some(size) = size else {
        return Ok(());
    };
    master
        .resize(pty_size(size))
        .context("failed to resize local pseudo-terminal")?;
    let _ = event_tx.blocking_send(LocalShellEvent::Resized {
        columns: size.columns,
        rows: size.rows,
    });
    Ok(())
}

fn read_output(mut reader: Box<dyn Read + Send>, event_tx: &mpsc::Sender<LocalShellEvent>) {
    let mut buffer = vec![0; MAX_OUTPUT_BATCH_BYTES];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => return,
            Ok(read) => {
                if event_tx
                    .blocking_send(LocalShellEvent::Output(buffer[..read].to_vec()))
                    .is_err()
                {
                    return;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return,
        }
    }
}

fn wake_worker(command_tx: &SyncSender<LocalShellCommand>) -> Result<()> {
    match command_tx.try_send(LocalShellCommand::Wake) {
        Ok(()) | Err(TrySendError::Full(_)) => Ok(()),
        Err(TrySendError::Disconnected(_)) => {
            anyhow::bail!("local PTY worker is no longer running")
        }
    }
}

fn validate_terminal_size(columns: u32, rows: u32) -> Result<()> {
    if columns == 0 || rows == 0 {
        anyhow::bail!("terminal dimensions must be greater than zero");
    }
    if columns > MAX_COLUMNS || rows > MAX_ROWS {
        anyhow::bail!("terminal dimensions cannot exceed {MAX_COLUMNS}x{MAX_ROWS}");
    }
    Ok(())
}

fn pty_size(size: TerminalSize) -> PtySize {
    PtySize {
        rows: size.rows as u16,
        cols: size.columns as u16,
        pixel_width: 0,
        pixel_height: 0,
    }
}

fn platform_shell_candidates() -> Vec<String> {
    #[cfg(windows)]
    {
        vec!["pwsh.exe".into(), "powershell.exe".into(), "cmd.exe".into()]
    }
    #[cfg(not(windows))]
    {
        vec![
            "zsh".into(),
            "bash".into(),
            "sh".into(),
            "fish".into(),
            "nu".into(),
        ]
    }
}

fn platform_default_shell() -> Option<String> {
    #[cfg(windows)]
    let key = "COMSPEC";
    #[cfg(not(windows))]
    let key = "SHELL";
    env::var_os(key).and_then(|value| value.into_string().ok())
}

fn default_shell_fallback() -> Option<&'static str> {
    #[cfg(windows)]
    {
        Some("cmd.exe")
    }
    #[cfg(not(windows))]
    {
        Some("sh")
    }
}

fn find_executable(program: &str) -> Option<PathBuf> {
    let direct = PathBuf::from(program);
    if direct.components().count() > 1 {
        return is_executable(&direct).then_some(direct);
    }
    let path = env::var_os("PATH")?;
    for directory in env::split_paths(&path) {
        let candidate = directory.join(program);
        if is_executable(&candidate) {
            return Some(candidate);
        }
        #[cfg(windows)]
        for extension in ["exe", "cmd", "bat"] {
            let candidate = directory.join(format!("{program}.{extension}"));
            if is_executable(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;

    path.metadata()
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

fn bounded_error(error: &anyhow::Error) -> String {
    bounded_text(format!("{error:#}"))
}

fn bounded_text(value: String) -> String {
    value.chars().take(MAX_ERROR_CHARS).collect()
}

fn panic_message(panic: Box<dyn std::any::Any + Send>) -> String {
    panic
        .downcast_ref::<&str>()
        .map(|message| (*message).to_owned())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic payload".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_shell_and_explicit_shell_resolve() {
        assert!(resolve_shell(SYSTEM_SHELL).is_ok());
        let shells = discover_shells();
        assert!(!shells.is_empty());
        assert!(shells.iter().all(|shell| resolve_shell(shell).is_ok()));
    }

    #[test]
    fn invalid_shell_is_rejected_without_fallback() {
        assert!(resolve_shell("axssh-shell-that-does-not-exist").is_err());
        assert!(resolve_shell("bad\nshell").is_err());
    }

    #[cfg(not(windows))]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn local_pty_accepts_input_and_streams_output() {
        let (worker, mut events) = LocalShellHandle::spawn("sh".into(), 80, 24);
        let output = timeout(Duration::from_secs(5), async {
            let mut output = Vec::new();
            let mut sent = false;
            while let Some(event) = events.recv().await {
                match event {
                    LocalShellEvent::Started { .. } => {
                        worker
                            .request_send(b"printf 'AXSSH_LOCAL_PTY_OK\\n'; exit\n".to_vec())
                            .expect("local PTY should accept input");
                        sent = true;
                    }
                    LocalShellEvent::Output(data) => output.extend(data),
                    LocalShellEvent::Exited { .. } => break,
                    LocalShellEvent::Failed(message) => {
                        panic!("local PTY failed: {message}");
                    }
                    LocalShellEvent::Resized { .. } => {}
                }
            }
            assert!(sent, "local shell should report startup");
            output
        })
        .await
        .expect("local PTY should finish within the test timeout");
        worker
            .shutdown()
            .await
            .expect("finished local PTY should join cleanly");
        let output = String::from_utf8_lossy(&output);
        assert!(
            output.contains("AXSSH_LOCAL_PTY_OK"),
            "output was {output:?}"
        );
    }
}

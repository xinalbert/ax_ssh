//! Bounded local pseudo-terminal discovery and worker lifecycle.

use std::collections::BTreeSet;
use std::env;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{Context, Result};
use portable_pty::{ChildKiller, CommandBuilder, PtySize, native_pty_system};
use tokio::sync::mpsc;
use tokio::time::timeout;

use crate::terminal_dimensions::{TerminalSize, validate_backend_size};
use crate::terminal_input::try_queue_sync_motion;

pub const SYSTEM_SHELL: &str = "System default";

const COMMAND_CAPACITY: usize = 32;
const EVENT_CAPACITY: usize = 32;
const MAX_INPUT_BYTES: usize = 16 * 1024;
const MAX_OUTPUT_BATCH_BYTES: usize = 16 * 1024;
const MAX_ERROR_CHARS: usize = 512;
const COMMAND_POLL_INTERVAL: Duration = Duration::from_millis(25);
const EVENT_BACKPRESSURE_INTERVAL: Duration = Duration::from_millis(5);
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

struct LocalShellTask {
    shell: String,
    initial_size: TerminalSize,
    command_rx: Receiver<LocalShellCommand>,
    pending_resize: Arc<Mutex<Option<TerminalSize>>>,
    shutdown_requested: Arc<AtomicBool>,
    child_killer: Arc<Mutex<Option<Box<dyn ChildKiller + Send + Sync>>>>,
    process_group: Arc<AtomicI32>,
    event_tx: mpsc::Sender<LocalShellEvent>,
}

/// UI-adjacent controller for one worker-owned local PTY process.
pub struct LocalShellHandle {
    command_tx: SyncSender<LocalShellCommand>,
    pending_resize: Arc<Mutex<Option<TerminalSize>>>,
    shutdown_requested: Arc<AtomicBool>,
    child_killer: Arc<Mutex<Option<Box<dyn ChildKiller + Send + Sync>>>>,
    process_group: Arc<AtomicI32>,
    thread: Option<JoinHandle<()>>,
}

impl LocalShellHandle {
    pub fn spawn(
        shell: String,
        columns: u32,
        rows: u32,
    ) -> (Self, mpsc::Receiver<LocalShellEvent>) {
        let initial_size = TerminalSize::backend(columns, rows);
        let (command_tx, command_rx) = sync_channel(COMMAND_CAPACITY);
        let (event_tx, event_rx) = mpsc::channel(EVENT_CAPACITY);
        let pending_resize = Arc::new(Mutex::new(None));
        let shutdown_requested = Arc::new(AtomicBool::new(false));
        let child_killer = Arc::new(Mutex::new(None));
        let process_group = Arc::new(AtomicI32::new(0));
        let task = LocalShellTask {
            shell,
            initial_size,
            command_rx,
            pending_resize: pending_resize.clone(),
            shutdown_requested: shutdown_requested.clone(),
            child_killer: child_killer.clone(),
            process_group: process_group.clone(),
            event_tx: event_tx.clone(),
        };
        let failure_shutdown = shutdown_requested.clone();
        let failure_event_tx = event_tx.clone();
        let thread = thread::Builder::new()
            .name("axssh-local-pty".to_owned())
            .spawn(move || {
                if let Err(error) = run_local_shell(task) {
                    let _ = send_event_with_cancellation(
                        &failure_event_tx,
                        LocalShellEvent::Failed(bounded_error(&error)),
                        &failure_shutdown,
                    );
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
                child_killer,
                process_group,
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

    /// Returns `false` when a pointer-motion frame is dropped under normal backpressure.
    pub fn request_send_motion(&self, data: Vec<u8>) -> Result<bool> {
        if data.is_empty() {
            return Ok(true);
        }
        if data.len() > MAX_INPUT_BYTES {
            anyhow::bail!("terminal input cannot exceed {MAX_INPUT_BYTES} bytes");
        }
        try_queue_sync_motion(
            &self.command_tx,
            LocalShellCommand::Send(data),
            "cannot queue local mouse motion after PTY worker stopped",
        )
    }

    pub fn request_resize(&self, columns: u32, rows: u32) -> Result<()> {
        validate_terminal_size(columns, rows)?;
        *self
            .pending_resize
            .lock()
            .map_err(|_| anyhow::anyhow!("local terminal resize state lock poisoned"))? =
            Some(TerminalSize::backend(columns, rows));
        wake_worker(&self.command_tx)
    }

    pub async fn shutdown(mut self) -> Result<()> {
        self.shutdown_requested.store(true, Ordering::Release);
        let _ = wake_worker(&self.command_tx);
        let Some(thread) = self.thread.take() else {
            return Ok(());
        };
        let termination_error =
            force_kill_child(self.child_killer.clone(), self.process_group.clone())
                .await
                .err();
        let exceeded_timeout = timeout(WORKER_SHUTDOWN_TIMEOUT, wait_for_thread(&thread))
            .await
            .is_err();
        if exceeded_timeout {
            if let Some(error) = termination_error {
                anyhow::bail!(
                    "local PTY worker exceeded shutdown timeout after child termination failed: {error:#}"
                );
            }
            anyhow::bail!("local PTY worker exceeded shutdown timeout");
        }
        thread.join().map_err(|panic| {
            anyhow::anyhow!("local PTY worker panicked: {}", panic_message(panic))
        })?;
        Ok(())
    }
}

impl Drop for LocalShellHandle {
    fn drop(&mut self) {
        self.shutdown_requested.store(true, Ordering::Release);
        let _ = wake_worker(&self.command_tx);
        let _ = force_kill_child_blocking(&self.child_killer, &self.process_group);
    }
}

async fn wait_for_thread(thread: &JoinHandle<()>) {
    while !thread.is_finished() {
        tokio::time::sleep(COMMAND_POLL_INTERVAL).await;
    }
}

async fn force_kill_child(
    child_killer: Arc<Mutex<Option<Box<dyn ChildKiller + Send + Sync>>>>,
    process_group: Arc<AtomicI32>,
) -> Result<()> {
    tokio::task::spawn_blocking(move || force_kill_child_blocking(&child_killer, &process_group))
        .await
        .context("local PTY child termination task failed")?
}

fn force_kill_child_blocking(
    child_killer: &Arc<Mutex<Option<Box<dyn ChildKiller + Send + Sync>>>>,
    process_group: &Arc<AtomicI32>,
) -> Result<()> {
    let process_group = process_group.load(Ordering::Acquire);
    match kill_local_process_group(process_group) {
        Ok(()) => Ok(()),
        Err(group_error) => {
            let mut killer = child_killer
                .lock()
                .map_err(|_| anyhow::anyhow!("local PTY child killer lock poisoned"))?;
            match killer.as_mut() {
                Some(killer) => killer.kill().context("failed to terminate local PTY child"),
                None if process_group <= 0 => Ok(()),
                None => Err(group_error).context("failed to terminate local PTY child"),
            }
        }
    }
}

#[cfg(unix)]
fn kill_local_process_group(process_group: i32) -> std::io::Result<()> {
    const SIGKILL: i32 = 9;

    unsafe extern "C" {
        fn getpgrp() -> i32;
        fn kill(pid: i32, signal: i32) -> i32;
    }

    // SAFETY: getpgrp takes no arguments and has no memory-safety preconditions.
    let own_process_group = unsafe { getpgrp() };
    if process_group <= 0 || process_group == own_process_group {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "local PTY process group is unavailable",
        ));
    }
    // SAFETY: a negative, validated process-group ID targets only that PTY
    // group, and SIGKILL carries no pointer or lifetime requirements.
    if unsafe { kill(-process_group, SIGKILL) } == 0 {
        Ok(())
    } else {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(3) {
            Ok(())
        } else {
            Err(error)
        }
    }
}

#[cfg(not(unix))]
fn kill_local_process_group(_: i32) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "process-group termination is unavailable",
    ))
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
            .and_then(find_executable)
            .context("no platform default shell is available");
    }
    if selection.chars().count() > 256 || selection.chars().any(char::is_control) {
        anyhow::bail!("configured local shell is invalid");
    }
    find_executable(selection)
        .with_context(|| format!("configured local shell `{selection}` is not available"))
}

fn run_local_shell(task: LocalShellTask) -> Result<()> {
    let LocalShellTask {
        shell,
        initial_size,
        command_rx,
        pending_resize,
        shutdown_requested,
        child_killer,
        process_group,
        event_tx,
    } = task;
    let shell_path = resolve_shell(&shell)?;
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
    *child_killer
        .lock()
        .map_err(|_| anyhow::anyhow!("local PTY child killer lock poisoned"))? =
        Some(child.clone_killer());
    #[cfg(unix)]
    if let Some(group) = pair.master.process_group_leader() {
        process_group.store(group, Ordering::Release);
    }
    drop(pair.slave);
    let mut writer = match pair.master.take_writer() {
        Ok(writer) => writer,
        Err(error) => {
            let _ = terminate_child(child.as_mut(), process_group.load(Ordering::Acquire));
            clear_child_killer(&child_killer);
            return Err(error).context("failed to open local PTY input");
        }
    };
    let reader = match pair.master.try_clone_reader() {
        Ok(reader) => reader,
        Err(error) => {
            let _ = terminate_child(child.as_mut(), process_group.load(Ordering::Acquire));
            clear_child_killer(&child_killer);
            return Err(error).context("failed to open local PTY output");
        }
    };
    let reader_tx = event_tx.clone();
    let reader_shutdown = shutdown_requested.clone();
    let reader_thread = thread::Builder::new()
        .name("axssh-local-pty-reader".to_owned())
        .spawn(move || read_output(reader, &reader_tx, &reader_shutdown));
    let reader_thread = match reader_thread {
        Ok(reader_thread) => reader_thread,
        Err(error) => {
            let _ = terminate_child(child.as_mut(), process_group.load(Ordering::Acquire));
            clear_child_killer(&child_killer);
            return Err(error).context("failed to start local PTY reader");
        }
    };

    let outcome = drive_local_shell(
        shell_path.display().to_string(),
        initial_size,
        child.as_mut(),
        writer.as_mut(),
        pair.master.as_ref(),
        &command_rx,
        &pending_resize,
        &shutdown_requested,
        &event_tx,
    );
    if !matches!(outcome, Ok(Some(_))) {
        let _ = terminate_child(child.as_mut(), process_group.load(Ordering::Acquire));
    }
    drop(writer);
    drop(pair.master);
    reader_thread
        .join()
        .map_err(|panic| anyhow::anyhow!("local PTY reader panicked: {}", panic_message(panic)))?;
    clear_child_killer(&child_killer);
    process_group.store(0, Ordering::Release);
    if let Some(status) = outcome? {
        let _ = send_event_with_cancellation(
            &event_tx,
            LocalShellEvent::Exited { status },
            &shutdown_requested,
        );
    }
    Ok(())
}

fn terminate_child(child: &mut dyn portable_pty::Child, process_group: i32) -> Result<()> {
    if child
        .try_wait()
        .context("failed to poll local PTY child")?
        .is_some()
    {
        return Ok(());
    }
    let group_result = kill_local_process_group(process_group);
    let child_result = child.kill();
    if group_result.is_err() && child_result.is_err() {
        return group_result.context("failed to terminate local PTY process group");
    }
    for _ in 0..40 {
        if child
            .try_wait()
            .context("failed to reap local PTY child")?
            .is_some()
        {
            return Ok(());
        }
        thread::sleep(COMMAND_POLL_INTERVAL);
    }
    anyhow::bail!("local PTY child did not exit after forced termination")
}

fn clear_child_killer(child_killer: &Arc<Mutex<Option<Box<dyn ChildKiller + Send + Sync>>>>) {
    if let Ok(mut killer) = child_killer.lock() {
        *killer = None;
    }
}

#[allow(clippy::too_many_arguments)]
fn drive_local_shell(
    shell: String,
    initial_size: TerminalSize,
    child: &mut dyn portable_pty::Child,
    writer: &mut dyn Write,
    master: &(dyn portable_pty::MasterPty + Send),
    command_rx: &Receiver<LocalShellCommand>,
    pending_resize: &Arc<Mutex<Option<TerminalSize>>>,
    shutdown_requested: &Arc<AtomicBool>,
    event_tx: &mpsc::Sender<LocalShellEvent>,
) -> Result<Option<String>> {
    send_event_with_cancellation(
        event_tx,
        LocalShellEvent::Started { shell },
        shutdown_requested,
    )
    .context("local shell event receiver closed during startup")?;
    let mut applied_size = initial_size;
    while !shutdown_requested.load(Ordering::Acquire) {
        apply_pending_resize(
            master,
            pending_resize,
            &mut applied_size,
            shutdown_requested,
            event_tx,
        )?;
        if let Some(status) = child.try_wait().context("failed to poll local shell")? {
            return Ok(Some(status.to_string()));
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
    applied_size: &mut TerminalSize,
    shutdown_requested: &Arc<AtomicBool>,
    event_tx: &mpsc::Sender<LocalShellEvent>,
) -> Result<()> {
    let size = pending_resize
        .lock()
        .map_err(|_| anyhow::anyhow!("local terminal resize state lock poisoned"))?
        .take();
    let Some(size) = size else {
        return Ok(());
    };
    if size == *applied_size {
        return Ok(());
    }
    master
        .resize(pty_size(size))
        .context("failed to resize local pseudo-terminal")?;
    *applied_size = size;
    let _ = send_event_with_cancellation(
        event_tx,
        LocalShellEvent::Resized {
            columns: size.columns(),
            rows: size.rows(),
        },
        shutdown_requested,
    );
    Ok(())
}

fn read_output(
    mut reader: Box<dyn Read + Send>,
    event_tx: &mpsc::Sender<LocalShellEvent>,
    shutdown_requested: &Arc<AtomicBool>,
) {
    let mut buffer = vec![0; MAX_OUTPUT_BATCH_BYTES];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => return,
            Ok(read) => {
                if send_event_with_cancellation(
                    event_tx,
                    LocalShellEvent::Output(buffer[..read].to_vec()),
                    shutdown_requested,
                )
                .is_err()
                {
                    return;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => {
                if shutdown_requested.load(Ordering::Acquire) {
                    return;
                }
                // ConPTY can close its read handle before the child reports an
                // exit.  Treat that as a worker failure instead of silently
                // leaving the UI in a connected-but-empty state.
                let message = bounded_text(format!("local PTY output read failed: {error}"));
                let _ = send_event_with_cancellation(
                    event_tx,
                    LocalShellEvent::Failed(message),
                    shutdown_requested,
                );
                shutdown_requested.store(true, Ordering::Release);
                return;
            }
        }
    }
}

fn send_event_with_cancellation(
    event_tx: &mpsc::Sender<LocalShellEvent>,
    mut event: LocalShellEvent,
    shutdown_requested: &Arc<AtomicBool>,
) -> Result<()> {
    loop {
        if shutdown_requested.load(Ordering::Acquire) {
            anyhow::bail!("local PTY shutdown cancelled pending event delivery");
        }
        match event_tx.try_send(event) {
            Ok(()) => return Ok(()),
            Err(mpsc::error::TrySendError::Full(returned)) => {
                event = returned;
                thread::sleep(EVENT_BACKPRESSURE_INTERVAL);
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                anyhow::bail!("local shell event receiver closed");
            }
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
    validate_backend_size(columns, rows)
}

fn pty_size(size: TerminalSize) -> PtySize {
    PtySize {
        rows: size.rows() as u16,
        cols: size.columns() as u16,
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
    use std::sync::atomic::AtomicUsize;

    use super::*;

    struct CountingMasterPty {
        resize_calls: AtomicUsize,
    }

    impl portable_pty::MasterPty for CountingMasterPty {
        fn resize(&self, _size: PtySize) -> Result<()> {
            self.resize_calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn get_size(&self) -> Result<PtySize> {
            anyhow::bail!("resize deduplication must not query the platform PTY")
        }

        fn try_clone_reader(&self) -> Result<Box<dyn Read + Send>> {
            Ok(Box::new(std::io::empty()))
        }

        fn take_writer(&self) -> Result<Box<dyn Write + Send>> {
            Ok(Box::new(std::io::sink()))
        }

        #[cfg(unix)]
        fn process_group_leader(&self) -> Option<i32> {
            None
        }

        #[cfg(unix)]
        fn as_raw_fd(&self) -> Option<portable_pty::unix::RawFd> {
            None
        }

        #[cfg(unix)]
        fn tty_name(&self) -> Option<PathBuf> {
            None
        }
    }

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

    #[test]
    fn repeated_local_pty_size_does_not_call_platform_resize() {
        let master = CountingMasterPty {
            resize_calls: AtomicUsize::new(0),
        };
        let initial_size = TerminalSize::backend(80, 24);
        let pending_resize = Arc::new(Mutex::new(Some(initial_size)));
        let shutdown_requested = Arc::new(AtomicBool::new(false));
        let (event_tx, mut event_rx) = mpsc::channel(1);
        let mut applied_size = initial_size;

        apply_pending_resize(
            &master,
            &pending_resize,
            &mut applied_size,
            &shutdown_requested,
            &event_tx,
        )
        .expect("same-size resize should be ignored");
        assert_eq!(master.resize_calls.load(Ordering::SeqCst), 0);
        assert!(event_rx.try_recv().is_err());

        *pending_resize.lock().expect("resize state should lock") =
            Some(TerminalSize::backend(100, 30));
        apply_pending_resize(
            &master,
            &pending_resize,
            &mut applied_size,
            &shutdown_requested,
            &event_tx,
        )
        .expect("changed size should reach the platform PTY");
        assert_eq!(master.resize_calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            event_rx.try_recv(),
            Ok(LocalShellEvent::Resized {
                columns: 100,
                rows: 30,
            })
        );
    }

    #[test]
    fn cancellation_does_not_enqueue_pending_local_pty_events() {
        let shutdown_requested = Arc::new(AtomicBool::new(true));
        let (event_tx, mut event_rx) = mpsc::channel(1);

        assert!(
            send_event_with_cancellation(
                &event_tx,
                LocalShellEvent::Output(b"discard on shutdown".to_vec()),
                &shutdown_requested,
            )
            .is_err()
        );
        assert!(event_rx.try_recv().is_err());
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

    #[cfg(not(windows))]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn shutdown_terminates_a_running_shell_with_a_full_event_queue() {
        let (worker, mut events) = LocalShellHandle::spawn("sh".into(), 80, 24);
        let started = timeout(Duration::from_secs(2), events.recv())
            .await
            .expect("local shell should start promptly");
        assert!(matches!(started, Some(LocalShellEvent::Started { .. })));
        worker
            .request_send(b"while :; do echo AXSSH_LOCAL_PTY_BUSY; done\n".to_vec())
            .expect("local PTY should accept the output loop");
        tokio::time::sleep(Duration::from_millis(100)).await;

        timeout(Duration::from_secs(5), worker.shutdown())
            .await
            .expect("shutdown must not leave a detached blocking join")
            .expect("running local PTY should shut down cleanly");
    }

    #[cfg(not(windows))]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn dropping_handle_terminates_the_owned_shell() {
        let (worker, mut events) = LocalShellHandle::spawn("sh".into(), 80, 24);
        let started = timeout(Duration::from_secs(2), events.recv())
            .await
            .expect("local shell should start promptly");
        assert!(matches!(started, Some(LocalShellEvent::Started { .. })));

        drop(worker);

        timeout(Duration::from_secs(5), async {
            while events.recv().await.is_some() {}
        })
        .await
        .expect("dropping the owner must terminate the local PTY worker");
    }
}

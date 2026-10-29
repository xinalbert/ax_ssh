//! SFTP application facade: shared context and callback composition.

mod drag;
mod files;
mod navigation;
mod transfers;

pub(super) use self::drag::handle_native_dropped_file_on_remote_pane;
use self::drag::*;
use self::files::*;
pub(super) use self::navigation::{load_local_directory, navigate_sftp_tab_to_path};
use self::transfers::*;

use super::local_files::{
    LocalDirectoryEntry, PreparedLocalUpload, read_local_directory, validate_local_file_for_open,
};
use super::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

const LOCAL_DIRECTORY_TIMEOUT: Duration = Duration::from_secs(5);
const LOCAL_OPEN_TIMEOUT: Duration = Duration::from_secs(5);
const LOCAL_SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const LOCAL_DRAG_PREFIX: &str = "axssh-local-path:";
const REMOTE_DRAG_PREFIX: &str = "axssh-remote-path:";
const SFTP_DRAG_TARGET: &str = "ax_ssh::sftp_drag";

struct LocalSnapshotCancelGuard(Arc<AtomicBool>);

impl Drop for LocalSnapshotCancelGuard {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

fn with_sftp_terminal_for_tab<T>(
    state: &Arc<Mutex<AppState>>,
    tab_id: Uuid,
    action: impl FnOnce(&mut TerminalTabState) -> Result<T>,
) -> Result<T> {
    let mut app = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    let terminal = app
        .terminal_mut(tab_id)
        .context("SFTP tab is no longer available")?;
    if !terminal.is_sftp() {
        anyhow::bail!("upload target is no longer an SFTP tab");
    }
    if !terminal.connected {
        anyhow::bail!("SFTP session is no longer connected");
    }
    action(terminal)
}

fn active_sftp_upload_target(
    state: &Arc<Mutex<AppState>>,
    router: &WindowRouter,
    window_id: Uuid,
) -> Result<(Uuid, String)> {
    let tab_id = router.active_tab(window_id).context("no active terminal")?;
    let mut remote_directory = String::new();
    with_sftp_terminal_for_tab(state, tab_id, |terminal| {
        remote_directory = terminal
            .sftp
            .upload_directory()
            .context("remote SFTP upload destination is not stable")?
            .to_owned();
        Ok(())
    })?;
    Ok((tab_id, remote_directory))
}

fn active_sftp_transfer_filter_patterns(state: &Arc<Mutex<AppState>>) -> Result<Vec<String>> {
    let app = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    Ok(app
        .sessions
        .settings
        .sftp_transfer_filters
        .effective_patterns())
}

fn prepare_selected_local_upload(
    state: &Arc<Mutex<AppState>>,
    router: &WindowRouter,
    window_id: Uuid,
) -> Result<(Uuid, String, Vec<PathBuf>)> {
    let mut app = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    let tab_id = router.active_tab(window_id).context("no active terminal")?;
    let terminal = app.terminal_mut(tab_id).context("no active terminal")?;
    if !terminal.is_sftp() {
        anyhow::bail!("SFTP is available only in an SFTP tab");
    }
    if !terminal.connected {
        anyhow::bail!("SFTP session is not connected");
    }
    let remote_directory = terminal
        .sftp
        .upload_directory()
        .context("remote SFTP upload destination is not stable")?
        .to_owned();
    if !terminal.sftp.local.upload_selection_ready() {
        anyhow::bail!("local SFTP upload selection is not ready");
    }
    let selected = terminal
        .sftp
        .local
        .entries
        .iter()
        .filter(|entry| terminal.sftp.local.selected.contains(&entry.path))
        .map(|entry| PathBuf::from(&entry.path))
        .collect::<Vec<_>>();
    if selected.is_empty() {
        anyhow::bail!("select at least one local file or folder to upload");
    }
    Ok((tab_id, remote_directory, selected))
}

pub(super) fn wire_sftp(
    ui: &AppWindow,
    state: Arc<Mutex<AppState>>,
    runtime: Handle,
    window_router: WindowRouter,
    window_id: Uuid,
) {
    navigation::wire_navigation(
        ui,
        state.clone(),
        runtime.clone(),
        window_router.clone(),
        window_id,
    );
    files::wire_files(
        ui,
        state.clone(),
        runtime.clone(),
        window_router.clone(),
        window_id,
    );
    drag::wire_drag(
        ui,
        state.clone(),
        runtime.clone(),
        window_router.clone(),
        window_id,
    );
    transfers::wire_transfers(ui, state, runtime, window_router, window_id);
}

/// Resolve the SFTP operation target from the originating window route.
/// `AppState`'s process-wide active Tab is presentation state, not an intent
/// context, because another window may be focused at the same time.
fn with_window_sftp_terminal<T>(
    state: &Arc<Mutex<AppState>>,
    router: &WindowRouter,
    window_id: Uuid,
    action: impl FnOnce(&mut TerminalTabState) -> Result<T>,
) -> Result<T> {
    let tab_id = router
        .active_tab(window_id)
        .context("no active SFTP tab for this window")?;
    with_sftp_terminal_for_tab(state, tab_id, action)
}

// A modal captures its original target. Revalidate it before enqueueing a
// write, including when a native menu or another window changes live state.
fn parse_transfer_id(value: &str) -> Result<uuid::Uuid> {
    value
        .parse::<uuid::Uuid>()
        .context("invalid SFTP transfer id")
}

#[cfg(test)]
mod tests;

//! Startup workspace recovery and recent-file reconciliation.

use ax_ssh::config::{ConfigStore, SessionStore};
use std::path::Path;
use tracing::warn;

pub(in crate::app) fn load_startup_workspace(
    config: &ConfigStore,
    sessions: &mut SessionStore,
) -> Option<ax_ssh::config::WorkspaceSnapshot> {
    // The recovery checkpoint reflects the most recent running workspace.
    // A manually opened file is a template until the user explicitly saves it.
    match config.load_workspace() {
        Ok(Some(snapshot)) => return Some(snapshot),
        Ok(None) => {}
        Err(error) => warn!(%error, "automatic workspace checkpoint could not be loaded"),
    }
    let recent_paths = sessions.recent_workspace_paths().to_vec();
    let mut removed_unavailable_path = false;
    for path in recent_paths {
        match ConfigStore::load_workspace_file(Path::new(&path)) {
            Ok(snapshot) => {
                if removed_unavailable_path && let Err(error) = config.save(sessions) {
                    warn!(%error, "failed to persist cleaned recent workspace history");
                }
                return Some(snapshot);
            }
            Err(error) => {
                warn!(%error, path, "recent workspace could not be restored; removing it from history");
                removed_unavailable_path |= sessions.remove_workspace_path(Path::new(&path));
            }
        }
    }
    if removed_unavailable_path && let Err(error) = config.save(sessions) {
        warn!(%error, "failed to persist cleaned recent workspace history");
    }
    None
}

#[cfg(test)]
mod tests;

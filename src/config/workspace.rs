use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const WORKSPACE_SNAPSHOT_VERSION: u32 = 1;
const MAX_TABS: usize = 256;
const MAX_WINDOWS: usize = 32;
const MAX_PANES: usize = 8;
const MAX_TEXT_BYTES: usize = 256 * 1024;
const MAX_PATH_BYTES: usize = 4 * 1024;
const MAX_TITLE_BYTES: usize = 512;
const MAX_STATUS_BYTES: usize = 2 * 1024;

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct WorkspaceSnapshot {
    pub version: u32,
    #[serde(default)]
    pub tabs: Vec<WorkspaceTabSnapshot>,
    #[serde(default)]
    pub active_tab_id: Option<Uuid>,
    #[serde(default)]
    pub windows: Vec<WorkspaceWindowSnapshot>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct WorkspaceTabSnapshot {
    pub id: Uuid,
    pub title: String,
    /// `terminal`, `sftp`, `settings`, or `session-editor`.
    pub kind: String,
    #[serde(default)]
    pub profile_id: Option<Uuid>,
    #[serde(default)]
    pub companion_tab_id: Option<Uuid>,
    #[serde(default)]
    pub terminal_text: String,
    #[serde(default)]
    pub sftp_remote_path: String,
    #[serde(default)]
    pub sftp_local_path: String,
    #[serde(default)]
    pub status: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct WorkspaceWindowSnapshot {
    /// The main window uses `Uuid::nil()`. Detached IDs are layout labels only
    /// and are regenerated when native windows are recreated.
    pub id: Uuid,
    #[serde(default)]
    pub tab_ids: Vec<Uuid>,
    #[serde(default)]
    pub active_tab_id: Option<Uuid>,
    #[serde(default)]
    pub focused_tab_id: Option<Uuid>,
    #[serde(default)]
    pub panes: Vec<PaneNodeSnapshot>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum PaneNodeSnapshot {
    Leaf(Uuid),
    Split {
        axis: String,
        ratio_milli: u16,
        first: Box<Self>,
        second: Box<Self>,
    },
}

impl WorkspaceSnapshot {
    pub fn validate(&self) -> Result<()> {
        if self.version != WORKSPACE_SNAPSHOT_VERSION {
            bail!("unsupported workspace snapshot version {}", self.version);
        }
        if self.tabs.len() > MAX_TABS {
            bail!("workspace snapshot cannot exceed {MAX_TABS} tabs");
        }
        if self.windows.len() > MAX_WINDOWS {
            bail!("workspace snapshot cannot exceed {MAX_WINDOWS} windows");
        }
        let ids = self
            .tabs
            .iter()
            .map(|tab| tab.id)
            .collect::<std::collections::HashSet<_>>();
        if ids.len() != self.tabs.len() {
            bail!("workspace snapshot contains duplicate tab IDs");
        }
        for tab in &self.tabs {
            validate_text(&tab.title, MAX_TITLE_BYTES, "tab title", true)?;
            validate_text(&tab.terminal_text, MAX_TEXT_BYTES, "terminal text", false)?;
            validate_text(&tab.sftp_remote_path, MAX_PATH_BYTES, "remote path", true)?;
            validate_text(&tab.sftp_local_path, MAX_PATH_BYTES, "local path", true)?;
            validate_text(&tab.status, MAX_STATUS_BYTES, "tab status", true)?;
            if let Some(companion) = tab.companion_tab_id
                && !ids.contains(&companion)
            {
                bail!("tab companion does not exist");
            }
        }
        if let Some(active) = self.active_tab_id
            && !ids.contains(&active)
        {
            bail!("active tab does not exist");
        }
        for window in &self.windows {
            if window.tab_ids.len() > MAX_TABS {
                bail!("workspace window contains too many tabs");
            }
            if window.tab_ids.iter().any(|id| !ids.contains(id)) {
                bail!("workspace window references an unknown tab");
            }
            if let Some(active) = window.active_tab_id
                && !window.tab_ids.contains(&active)
            {
                bail!("workspace window active tab is not in that window");
            }
            if let Some(focused) = window.focused_tab_id
                && !window.tab_ids.contains(&focused)
            {
                bail!("workspace window focused tab is not in that window");
            }
            for pane in &window.panes {
                let mut pane_ids = Vec::new();
                pane.validate(&ids, 0, &mut pane_ids)?;
                let mut seen = std::collections::HashSet::new();
                if pane_ids
                    .iter()
                    .any(|id| !seen.insert(*id) || !window.tab_ids.contains(id))
                {
                    bail!("workspace pane tree contains duplicate or hidden tab IDs");
                }
            }
        }
        Ok(())
    }
}

impl PaneNodeSnapshot {
    fn validate(
        &self,
        ids: &std::collections::HashSet<Uuid>,
        depth: usize,
        pane_ids: &mut Vec<Uuid>,
    ) -> Result<usize> {
        if depth > MAX_PANES {
            bail!("pane tree is too deep");
        }
        match self {
            Self::Leaf(id) => {
                if !ids.contains(id) {
                    bail!("pane references an unknown tab");
                }
                pane_ids.push(*id);
                Ok(1)
            }
            Self::Split {
                axis,
                ratio_milli,
                first,
                second,
            } => {
                if axis != "columns" && axis != "rows" {
                    bail!("pane split axis is invalid");
                }
                if !(100..=900).contains(ratio_milli) {
                    bail!("pane split ratio is outside 0.1..0.9");
                }
                let count = first.validate(ids, depth + 1, pane_ids)?
                    + second.validate(ids, depth + 1, pane_ids)?;
                if count > MAX_PANES {
                    bail!("workspace pane count exceeds {MAX_PANES}");
                }
                Ok(count)
            }
        }
    }
}

fn validate_text(value: &str, max_bytes: usize, label: &str, reject_controls: bool) -> Result<()> {
    if value.len() > max_bytes {
        bail!("{label} exceeds {max_bytes} bytes");
    }
    if reject_controls && value.chars().any(char::is_control) {
        bail!("{label} contains control characters");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unknown_version_and_duplicate_ids() {
        let id = Uuid::new_v4();
        let mut snapshot = WorkspaceSnapshot {
            version: WORKSPACE_SNAPSHOT_VERSION,
            tabs: vec![
                WorkspaceTabSnapshot {
                    id,
                    ..Default::default()
                },
                WorkspaceTabSnapshot {
                    id,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        assert!(snapshot.validate().is_err());
        snapshot.tabs.pop();
        snapshot.version = 999;
        assert!(snapshot.validate().is_err());
    }
}

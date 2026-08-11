use super::*;

impl SftpBrowserState {
    pub(super) fn for_standalone_tab(local_path: &str) -> Self {
        let local_path = local_path.trim();
        Self {
            local: LocalDirectoryState {
                path: if local_path.is_empty() {
                    default_local_directory()
                } else {
                    local_path.to_owned()
                },
                status: "Local directory ready".to_owned(),
                ..LocalDirectoryState::default()
            },
            ..Self::default()
        }
    }

    pub(super) fn snapshot(&self, available: bool) -> SftpBrowserSnapshot {
        SftpBrowserSnapshot {
            available,
            open: self.open,
            loading: self.loading,
            home: self.home.clone(),
            path: self.path.clone(),
            entries: self.entries.clone(),
            has_more: self.has_more,
            truncated: self.truncated,
            status: self.status.clone(),
            can_go_back: !self.loading && !self.back_history.is_empty(),
            can_go_forward: !self.loading && !self.forward_history.is_empty(),
            selected_count: self.selected_count(),
            all_selected: self.all_selected(),
            selected: self.selected.clone(),
            local: self.local.snapshot(),
            transfers: self
                .transfers
                .iter()
                .map(SftpTransferState::snapshot)
                .collect(),
        }
    }

    pub(in crate::app) fn queue_transfer(
        &mut self,
        id: Uuid,
        name: String,
        total_bytes: u64,
    ) -> Result<()> {
        if self.transfers.iter().any(|transfer| transfer.id == id) {
            anyhow::bail!("SFTP transfer already exists");
        }
        if self.transfers.len() >= SFTP_TRANSFER_HISTORY_LIMIT {
            let removable = self
                .transfers
                .iter()
                .position(|transfer| !transfer.phase.cancellable());
            if let Some(index) = removable {
                self.transfers.remove(index);
            } else {
                anyhow::bail!("SFTP transfer history is full");
            }
        }
        self.transfers.push_back(SftpTransferState {
            id,
            name,
            phase: SftpTransferPhase::Queued,
            downloaded_bytes: 0,
            total_bytes,
            bytes_per_second: 0,
            started_at: None,
            status: "Queued".to_owned(),
        });
        Ok(())
    }

    pub(in crate::app) fn start_transfer(&mut self, id: Uuid, name: String, total_bytes: u64) {
        let Some(transfer) = self.transfers.iter_mut().find(|transfer| transfer.id == id) else {
            return;
        };
        if transfer.phase != SftpTransferPhase::Queued {
            return;
        }
        transfer.name = name;
        transfer.phase = SftpTransferPhase::Downloading;
        transfer.total_bytes = total_bytes;
        transfer.started_at = Some(Instant::now());
        transfer.status = "Downloading".to_owned();
    }

    pub(in crate::app) fn update_transfer_progress(
        &mut self,
        id: Uuid,
        downloaded_bytes: u64,
        total_bytes: u64,
    ) {
        let Some(transfer) = self.transfers.iter_mut().find(|transfer| transfer.id == id) else {
            return;
        };
        if transfer.phase != SftpTransferPhase::Downloading {
            return;
        }
        transfer.phase = SftpTransferPhase::Downloading;
        transfer.downloaded_bytes = downloaded_bytes.min(total_bytes);
        transfer.total_bytes = total_bytes;
        if let Some(started_at) = transfer.started_at {
            let elapsed = started_at.elapsed().as_secs_f64();
            if elapsed > 0.0 {
                transfer.bytes_per_second =
                    (transfer.downloaded_bytes as f64 / elapsed).min(u64::MAX as f64) as u64;
            }
        }
        transfer.status = if total_bytes == 0 {
            "Downloading".to_owned()
        } else {
            format!(
                "{:.0}%",
                100.0 * transfer.downloaded_bytes as f64 / total_bytes as f64
            )
        };
    }

    pub(in crate::app) fn mark_transfer_opening(&mut self, id: Uuid, total_bytes: u64) -> bool {
        let Some(transfer) = self.transfers.iter_mut().find(|transfer| transfer.id == id) else {
            return false;
        };
        if transfer.phase != SftpTransferPhase::Downloading {
            return false;
        }
        transfer.phase = SftpTransferPhase::Opening;
        transfer.downloaded_bytes = total_bytes;
        transfer.total_bytes = total_bytes;
        transfer.status = "Opening".to_owned();
        true
    }

    pub(in crate::app) fn request_transfer_cancel(&mut self, id: Uuid) -> bool {
        let Some(transfer) = self.transfers.iter_mut().find(|transfer| transfer.id == id) else {
            return false;
        };
        if !transfer.phase.can_request_cancel() {
            return false;
        }
        transfer.phase = SftpTransferPhase::Cancelling;
        transfer.status = "Cancelling".to_owned();
        true
    }

    pub(in crate::app) fn transfer_is_cancellable(&self, id: Uuid) -> bool {
        self.transfers
            .iter()
            .find(|transfer| transfer.id == id)
            .is_some_and(|transfer| transfer.phase.can_request_cancel())
    }

    pub(in crate::app) fn finish_transfer(
        &mut self,
        id: Uuid,
        phase: SftpTransferPhase,
        status: String,
    ) {
        let Some(transfer) = self.transfers.iter_mut().find(|transfer| transfer.id == id) else {
            return;
        };
        let allowed = matches!(
            (transfer.phase, phase),
            (SftpTransferPhase::Queued, SftpTransferPhase::Cancelled)
                | (SftpTransferPhase::Queued, SftpTransferPhase::Failed)
                | (SftpTransferPhase::Downloading, SftpTransferPhase::Cancelled)
                | (SftpTransferPhase::Downloading, SftpTransferPhase::Failed)
                | (SftpTransferPhase::Cancelling, SftpTransferPhase::Cancelled)
                | (SftpTransferPhase::Cancelling, SftpTransferPhase::Failed)
                | (SftpTransferPhase::Opening, SftpTransferPhase::Completed)
                | (SftpTransferPhase::Opening, SftpTransferPhase::Failed)
        );
        if !allowed {
            return;
        }
        transfer.phase = phase;
        if phase == SftpTransferPhase::Completed {
            transfer.downloaded_bytes = transfer.total_bytes;
        }
        transfer.status = status;
    }

    pub(in crate::app) fn begin_navigation(
        &mut self,
        kind: SftpNavigation,
        path: Option<String>,
    ) -> Result<String> {
        if self.loading {
            anyhow::bail!("SFTP directory request already in progress");
        }
        let requested = match kind {
            SftpNavigation::Direct => path
                .context("SFTP directory path is missing")?
                .trim()
                .to_owned(),
            SftpNavigation::Back => self
                .back_history
                .back()
                .cloned()
                .context("no previous SFTP directory")?,
            SftpNavigation::Forward => self
                .forward_history
                .back()
                .cloned()
                .context("no next SFTP directory")?,
        };
        if requested.is_empty() {
            anyhow::bail!("SFTP directory path is empty");
        }
        self.pending_navigation = Some(PendingSftpNavigation {
            kind,
            from: self.path.clone(),
            requested: requested.clone(),
        });
        self.loading = true;
        self.status = "Loading directory...".to_owned();
        Ok(requested)
    }

    pub(in crate::app) fn cancel_navigation(&mut self) {
        self.pending_navigation = None;
        self.loading = false;
    }

    pub(in crate::app) fn reset_navigation(&mut self) {
        self.path.clear();
        self.entries.clear();
        self.has_more = false;
        self.truncated = false;
        self.selected.clear();
        self.back_history.clear();
        self.forward_history.clear();
        self.pending_navigation = None;
    }

    pub(in crate::app) fn complete_navigation(&mut self, path: String) {
        if let Some(pending) = self.pending_navigation.take() {
            match pending.kind {
                SftpNavigation::Direct if pending.from != path => {
                    push_sftp_history(&mut self.back_history, pending.from);
                    self.forward_history.clear();
                }
                SftpNavigation::Back => {
                    if self
                        .back_history
                        .back()
                        .is_some_and(|candidate| candidate == &pending.requested)
                    {
                        self.back_history.pop_back();
                        push_sftp_history(&mut self.forward_history, pending.from);
                    }
                }
                SftpNavigation::Forward => {
                    if self
                        .forward_history
                        .back()
                        .is_some_and(|candidate| candidate == &pending.requested)
                    {
                        self.forward_history.pop_back();
                        push_sftp_history(&mut self.back_history, pending.from);
                    }
                }
                SftpNavigation::Direct => {}
            }
        }
        self.loading = false;
        self.path = path;
    }

    pub(in crate::app) fn toggle_selection(&mut self, path: &str, selected: bool) -> bool {
        if !self.entries.iter().any(|entry| entry.path == path) {
            return false;
        }
        if selected {
            self.selected.insert(path.to_owned());
        } else {
            self.selected.remove(path);
        }
        true
    }

    pub(in crate::app) fn select_all(&mut self, selected: bool) {
        if selected {
            self.selected
                .extend(self.entries.iter().map(|entry| entry.path.clone()));
        } else {
            self.selected.clear();
        }
    }

    pub(in crate::app) fn selected_count(&self) -> usize {
        self.selected
            .iter()
            .filter(|path| self.entries.iter().any(|entry| &entry.path == *path))
            .count()
    }

    pub(in crate::app) fn all_selected(&self) -> bool {
        !self.entries.is_empty() && self.selected_count() == self.entries.len()
    }

    pub(in crate::app) fn reset(&mut self) {
        *self = Self::default();
    }
}

fn push_sftp_history(history: &mut VecDeque<String>, path: String) {
    if path.is_empty() || history.back().is_some_and(|current| current == &path) {
        return;
    }
    if history.len() >= SFTP_HISTORY_LIMIT {
        history.pop_front();
    }
    history.push_back(path);
}

impl LocalDirectoryState {
    pub(in crate::app) fn begin_load(&mut self, path: String) -> u64 {
        if self.path != path {
            self.selected.clear();
        }
        self.request_id = self.request_id.wrapping_add(1);
        self.path = path;
        self.loading = true;
        self.status = "Loading local directory...".to_owned();
        self.request_id
    }

    pub(in crate::app) fn complete(
        &mut self,
        path: String,
        entries: Vec<LocalDirectoryEntry>,
        truncated: bool,
    ) {
        if self.path != path {
            self.selected.clear();
        }
        self.loading = false;
        self.path = path;
        self.entries = entries;
        self.selected
            .retain(|selected| self.entries.iter().any(|entry| &entry.path == selected));
        self.truncated = truncated;
        self.status = if truncated {
            "Local directory limit reached".to_owned()
        } else {
            format!("{} items", self.entries.len())
        };
    }

    pub(in crate::app) fn fail(&mut self, message: String) {
        self.loading = false;
        self.status = message;
    }

    pub(in crate::app) fn toggle_selection(&mut self, path: &str, selected: bool) -> bool {
        if !self.entries.iter().any(|entry| entry.path == path) {
            return false;
        }
        if selected {
            self.selected.insert(path.to_owned());
        } else {
            self.selected.remove(path);
        }
        true
    }

    pub(in crate::app) fn select_all(&mut self, selected: bool) {
        if selected {
            self.selected
                .extend(self.entries.iter().map(|entry| entry.path.clone()));
        } else {
            self.selected.clear();
        }
    }

    fn selected_count(&self) -> usize {
        self.selected
            .iter()
            .filter(|path| self.entries.iter().any(|entry| &entry.path == *path))
            .count()
    }

    fn all_selected(&self) -> bool {
        !self.entries.is_empty() && self.selected_count() == self.entries.len()
    }

    fn snapshot(&self) -> LocalDirectorySnapshot {
        LocalDirectorySnapshot {
            loading: self.loading,
            path: self.path.clone(),
            entries: self.entries.clone(),
            truncated: self.truncated,
            status: self.status.clone(),
            selected_count: self.selected_count(),
            all_selected: self.all_selected(),
            selected: self.selected.clone(),
        }
    }
}

impl SftpTransferPhase {
    pub(in crate::app) fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Downloading => "downloading",
            Self::Cancelling => "cancelling",
            Self::Opening => "opening",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }

    pub(in crate::app) fn cancellable(self) -> bool {
        matches!(self, Self::Queued | Self::Downloading | Self::Cancelling)
    }

    fn can_request_cancel(self) -> bool {
        matches!(self, Self::Queued | Self::Downloading)
    }
}

impl SftpTransferState {
    fn snapshot(&self) -> SftpTransferSnapshot {
        SftpTransferSnapshot {
            id: self.id,
            name: self.name.clone(),
            phase: self.phase,
            downloaded_bytes: self.downloaded_bytes,
            total_bytes: self.total_bytes,
            bytes_per_second: self.bytes_per_second,
            status: self.status.clone(),
        }
    }
}

//! Transfer eligibility and event projection; text never decides control or lifetime.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SftpTransferPhase {
    Queued,
    Transferring,
    Pausing,
    Paused,
    Resuming,
    Cancelling,
    Conflict,
    Completed,
    Cancelled,
    Failed,
    Skipped,
}

impl SftpTransferPhase {
    pub fn finished(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Cancelled | Self::Failed | Self::Skipped
        )
    }
}

impl SftpTransferView {
    pub fn can_pause(&self) -> bool {
        self.pausable && self.phase == SftpTransferPhase::Transferring
    }
    pub fn can_resume(&self) -> bool {
        self.pausable && self.phase == SftpTransferPhase::Paused
    }
    pub fn can_cancel(&self) -> bool {
        !self.phase.finished() && self.phase != SftpTransferPhase::Cancelling
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SftpTransferControl {
    Pause,
    Resume,
    Cancel,
}

pub(super) use self::SftpTransferControl as Control;
pub const MAX_SFTP_TRANSFER_BATCH: usize = MAX_TRANSFERS;

impl SftpTransferControl {
    pub fn allows(self, transfer: &SftpTransferView) -> bool {
        match self {
            Self::Pause => transfer.can_pause(),
            Self::Resume => transfer.can_resume(),
            Self::Cancel => transfer.can_cancel(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SftpTransferBatchFailure {
    pub transfer_id: Uuid,
    pub message: String,
}

/// Most recent enqueue summary, not a transfer completion receipt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SftpTransferBatchResult {
    pub control: SftpTransferControl,
    pub requested: usize,
    pub queued: usize,
    pub skipped: usize,
    pub failures: Vec<SftpTransferBatchFailure>,
}

pub(super) fn validate_batch(ids: &[Uuid]) -> Result<()> {
    anyhow::ensure!(
        !ids.is_empty() && ids.len() <= MAX_SFTP_TRANSFER_BATCH,
        "SFTP batch requires 1–64 transfers"
    );
    let unique: HashSet<_> = ids.iter().collect();
    anyhow::ensure!(
        unique.len() == ids.len(),
        "SFTP batch contains duplicate transfers"
    );
    Ok(())
}

fn apply_batch(
    transfers: &mut BTreeMap<Uuid, SftpTransferView>,
    ids: &[Uuid],
    control: Control,
    mut send: impl FnMut(Uuid, Control) -> Result<()>,
) -> Result<SftpTransferBatchResult> {
    validate_batch(ids)?;
    let mut result = SftpTransferBatchResult {
        control,
        requested: ids.len(),
        queued: 0,
        skipped: 0,
        failures: vec![],
    };
    for id in ids {
        let Some(item) = transfers.get_mut(id).filter(|item| control.allows(item)) else {
            result.skipped += 1;
            continue;
        };
        match item.request_control(control, || send(*id, control)) {
            Ok(()) => result.queued += 1,
            Err(error) => result.failures.push(SftpTransferBatchFailure {
                transfer_id: *id,
                message: format!("{error:#}").chars().take(512).collect(),
            }),
        }
    }
    Ok(result)
}

impl SftpTransferView {
    fn request_control(
        &mut self,
        control: Control,
        send: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        let (allowed, phase, status) = match control {
            Control::Pause => (self.can_pause(), SftpTransferPhase::Pausing, "Pausing"),
            Control::Resume => (self.can_resume(), SftpTransferPhase::Resuming, "Resuming"),
            Control::Cancel => (
                self.can_cancel(),
                SftpTransferPhase::Cancelling,
                "Cancelling",
            ),
        };
        anyhow::ensure!(
            allowed,
            "SFTP transfer is no longer eligible for this control"
        );
        send()?;
        self.phase = phase;
        self.status = status.into();
        self.conflict = false;
        Ok(())
    }
}

impl Worker {
    pub(super) fn control_batch(&mut self, ids: Vec<Uuid>, control: Control) -> Result<()> {
        validate_batch(&ids)?;
        anyhow::ensure!(self.connected, "SFTP is not connected");
        let worker = self
            .transport
            .as_ref()
            .context("SFTP worker is unavailable")?;
        let result = apply_batch(
            &mut self.transfers,
            &ids,
            control,
            |id, control| match control {
                Control::Pause => worker.request_pause_sftp_transfer(id),
                Control::Resume => worker.request_resume_sftp_transfer(id),
                Control::Cancel => worker.request_cancel_sftp_transfer(id),
            },
        )?;
        self.status = format!(
            "SFTP batch: queued={}, skipped={}, failed={}",
            result.queued,
            result.skipped,
            result.failures.len()
        );
        self.last_batch = Some(result);
        Ok(())
    }
    pub(super) fn control_transfer(&mut self, id: Uuid, control: Control) -> Result<()> {
        anyhow::ensure!(self.connected, "SFTP is not connected");
        let item = self
            .transfers
            .get_mut(&id)
            .context("SFTP transfer no longer exists")?;
        let worker = self
            .transport
            .as_ref()
            .context("SFTP worker is unavailable")?;
        item.request_control(control, || match control {
            Control::Pause => worker.request_pause_sftp_transfer(id),
            Control::Resume => worker.request_resume_sftp_transfer(id),
            Control::Cancel => worker.request_cancel_sftp_transfer(id),
        })
    }

    /// Plan eviction first; only remove history once the new worker request was accepted.
    pub(super) fn transfer_evictions(&self, additional: usize) -> Result<Vec<Uuid>> {
        anyhow::ensure!(
            additional <= MAX_TRANSFERS,
            "SFTP batch exceeds the transfer limit"
        );
        let required = self
            .transfers
            .len()
            .saturating_add(additional)
            .saturating_sub(MAX_TRANSFERS);
        let ids: Vec<_> = self
            .transfers
            .values()
            .filter(|item| item.phase.finished())
            .take(required)
            .map(|item| item.id)
            .collect();
        anyhow::ensure!(ids.len() == required, "SFTP transfer history is full");
        Ok(ids)
    }

    pub(super) fn interrupt_transfers(&mut self) {
        self.last_batch = None;
        self.last_download_batch = None;
        for item in self
            .transfers
            .values_mut()
            .filter(|item| !item.phase.finished())
        {
            item.phase = SftpTransferPhase::Cancelled;
            item.status = "Cancelled".into();
            item.conflict = false;
        }
    }

    pub(super) fn transfer(&mut self, event: SftpTransferEvent) {
        use SftpTransferEvent as Event;
        use SftpTransferPhase as Phase;
        let id = match &event {
            Event::Queued { transfer_id, .. }
            | Event::Started { transfer_id, .. }
            | Event::Progress { transfer_id, .. }
            | Event::Paused { transfer_id, .. }
            | Event::Resumed { transfer_id, .. }
            | Event::DiscoveryFailed { transfer_id, .. }
            | Event::UploadConflict { transfer_id, .. }
            | Event::Skipped { transfer_id }
            | Event::Completed { transfer_id, .. }
            | Event::Cancelled { transfer_id }
            | Event::Failed { transfer_id, .. } => *transfer_id,
        };
        if !self.transfers.contains_key(&id) {
            // Uploads and direct downloads are registered before their requests.
            // Only recursive download children introduce new Queued identities.
            if !matches!(event, Event::Queued { .. } | Event::DiscoveryFailed { .. }) {
                return;
            }
            let Ok(evictions) = self.transfer_evictions(1) else {
                if matches!(event, Event::Queued { .. })
                    && let Some(worker) = &self.transport
                {
                    self.status = match worker.request_cancel_sftp_transfer(id) {
                        Ok(()) => {
                            "SFTP companion transfer limit reached; excess downloads cancelled"
                                .into()
                        }
                        Err(error) => format!("Cannot cancel excess transfer: {error:#}"),
                    };
                }
                return;
            };
            for id in evictions {
                self.transfers.remove(&id);
            }
        }
        let item = self
            .transfers
            .entry(id)
            .or_insert_with(|| SftpTransferView {
                id,
                name: "transfer".into(),
                status: "Queued".into(),
                downloaded_bytes: 0,
                total_bytes: 0,
                conflict: false,
                phase: Phase::Queued,
                pausable: matches!(&event, Event::Queued { remote_path, .. } if !remote_path.is_empty()),
            });
        if item.phase.finished() {
            return;
        }
        let terminal = matches!(
            event,
            Event::Completed { .. }
                | Event::Cancelled { .. }
                | Event::Failed { .. }
                | Event::DiscoveryFailed { .. }
                | Event::Skipped { .. }
        );
        if item.phase == Phase::Cancelling && !terminal {
            return;
        }
        match event {
            Event::Queued {
                name, total_bytes, ..
            } if matches!(item.phase, Phase::Queued | Phase::Conflict) => {
                item.name = name;
                item.total_bytes = total_bytes;
                item.phase = Phase::Queued;
                item.status = "Queued".into();
            }
            Event::Started {
                name, total_bytes, ..
            } if item.phase == Phase::Queued => {
                item.name = name;
                item.total_bytes = total_bytes;
                item.phase = Phase::Transferring;
                item.status = "Transferring".into();
            }
            Event::Progress {
                downloaded_bytes,
                total_bytes,
                ..
            } if matches!(
                item.phase,
                Phase::Transferring | Phase::Pausing | Phase::Resuming
            ) =>
            {
                item.downloaded_bytes = downloaded_bytes;
                item.total_bytes = total_bytes;
            }
            Event::Paused {
                downloaded_bytes,
                total_bytes,
                ..
            } if item.pausable && matches!(item.phase, Phase::Transferring | Phase::Pausing) => {
                item.downloaded_bytes = downloaded_bytes;
                item.total_bytes = total_bytes;
                item.phase = Phase::Paused;
                item.status = "Paused".into();
            }
            Event::Resumed {
                downloaded_bytes,
                total_bytes,
                ..
            } if item.pausable && matches!(item.phase, Phase::Paused | Phase::Resuming) => {
                item.downloaded_bytes = downloaded_bytes;
                item.total_bytes = total_bytes;
                item.phase = Phase::Transferring;
                item.status = "Transferring".into();
            }
            Event::Completed { total_bytes, .. } => {
                item.downloaded_bytes = total_bytes;
                item.total_bytes = total_bytes;
                item.phase = Phase::Completed;
                item.status = "Completed".into();
            }
            Event::Cancelled { .. } => {
                item.phase = Phase::Cancelled;
                item.status = "Cancelled".into();
            }
            Event::Failed { message, .. } | Event::DiscoveryFailed { message, .. } => {
                item.phase = Phase::Failed;
                item.status = format!("Failed: {message}");
            }
            Event::Skipped { .. } => {
                item.phase = Phase::Skipped;
                item.status = "Skipped".into();
            }
            Event::UploadConflict { .. } => {
                item.phase = Phase::Conflict;
                item.status = "Conflict".into();
            }
            _ => {}
        }
        item.conflict = item.phase == Phase::Conflict;
    }
}

#[cfg(test)]
pub(super) mod tests;

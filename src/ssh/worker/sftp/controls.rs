//! One bounded control registry per SFTP worker, including pre-start intents.

use super::*;
use crate::sftp::{SftpTransferAction, TransferCancellation};

const MAX_CONTROLS: usize = SFTP_TRANSFER_QUEUE_WINDOW * 4;

pub(super) struct TransferControls {
    page: TransferCancellation,
    entries: HashMap<Uuid, Entry>,
    limiter: Arc<TransferRateLimiter>,
}

struct Entry {
    control: TransferCancellation,
    parent: Option<Uuid>,
    submitted: bool,
}

impl TransferControls {
    pub(super) fn new(limiter: Arc<TransferRateLimiter>) -> Self {
        Self {
            page: TransferCancellation::with_limiter(limiter.clone()),
            entries: HashMap::new(),
            limiter,
        }
    }

    pub(super) fn register(&mut self, id: Uuid, parent: Option<Uuid>) -> TransferCancellation {
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.submitted = true;
            return entry.control.clone();
        }
        let control = parent
            .and_then(|id| self.entries.get(&id))
            .map_or(&self.page, |entry| &entry.control)
            .child();
        if self.entries.len() < MAX_CONTROLS {
            self.entries.insert(
                id,
                Entry {
                    control: control.clone(),
                    parent,
                    submitted: true,
                },
            );
        } else {
            warn!(transfer_id = %id, "SFTP control admission is full; cancelling unadmitted intent");
            control.cancel();
        }
        control
    }

    pub(super) fn get(&self, id: Uuid) -> TransferCancellation {
        self.entries.get(&id).map_or_else(
            || {
                let control = self.page.child();
                control.cancel();
                control
            },
            |entry| entry.control.clone(),
        )
    }

    pub(super) fn runnable(&self, id: Uuid) -> bool {
        let control = self.get(id);
        control.is_cancelled() || !control.is_paused()
    }

    pub(super) fn retain_live(&mut self, live: &HashSet<Uuid>) {
        self.entries
            .retain(|id, entry| !entry.submitted || live.contains(id));
    }

    pub(super) fn apply(
        &mut self,
        action: SftpTransferAction,
        all: bool,
        ids: Vec<Uuid>,
    ) -> Vec<Uuid> {
        let mut targets = ids;
        if all {
            targets.extend(self.entries.keys().copied());
        }
        targets.sort_unstable();
        targets.dedup();
        for id in &targets {
            let existed = self.entries.contains_key(id);
            let control = if existed {
                self.get(*id)
            } else {
                self.register(*id, None)
            };
            if !existed && let Some(entry) = self.entries.get_mut(id) {
                entry.submitted = false;
            }
            match action {
                SftpTransferAction::Pause if !all => control.pause(),
                SftpTransferAction::Pause => {}
                SftpTransferAction::Resume => control.resume(),
                SftpTransferAction::Cancel => control.cancel(),
            }
        }
        if all {
            match action {
                SftpTransferAction::Pause => self.page.pause(),
                SftpTransferAction::Resume => self.page.resume(),
                // Existing tokens retain their cancelled parent; new tasks
                // receive a fresh parent and remain usable after Cancel all.
                SftpTransferAction::Cancel => {
                    self.page.cancel();
                    self.page = TransferCancellation::with_limiter(self.limiter.clone());
                }
            }
        }
        let direct = targets.iter().copied().collect::<HashSet<_>>();
        targets.extend(self.entries.iter().filter_map(|(id, entry)| {
            entry
                .parent
                .filter(|parent| direct.contains(parent))
                .map(|_| *id)
        }));
        targets.sort_unstable();
        targets.dedup();
        if action == SftpTransferAction::Cancel {
            let target_set = targets.iter().copied().collect::<HashSet<_>>();
            targets.retain(|id| {
                !self
                    .entries
                    .get(id)
                    .and_then(|entry| entry.parent)
                    .is_some_and(|parent| target_set.contains(&parent))
            });
        }
        if action == SftpTransferAction::Resume {
            for id in &targets {
                self.get(*id).resume();
            }
        }
        targets
    }
}

#[cfg(test)]
mod tests;

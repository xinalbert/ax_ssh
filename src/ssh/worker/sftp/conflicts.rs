//! Download decisions belong to one live SFTP worker and one explicit batch.

use super::*;

#[derive(Default)]
pub(super) struct DownloadConflicts {
    active: HashMap<Uuid, SftpDownloadRequest>,
    pending: HashMap<Uuid, SftpDownloadRequest>,
    policies: HashMap<Uuid, SftpConflictChoice>,
    recent_batches: VecDeque<Uuid>,
}

impl DownloadConflicts {
    pub(super) fn pending_ids(&self) -> impl Iterator<Item = Uuid> + '_ {
        self.pending.keys().copied()
    }

    pub(super) fn pending_len(&self) -> usize {
        self.pending.len()
    }

    pub(super) fn contains(&self, id: Uuid) -> bool {
        self.pending.contains_key(&id)
    }

    pub(super) fn start(&mut self, request: &mut SftpDownloadRequest) {
        if let Some(batch) = request.conflict_batch() {
            if request.conflict_choice_is_unset()
                && let Some(&choice) = self.policies.get(&batch)
            {
                request.resolve_conflict(choice);
            }
            self.active.insert(request.transfer_id(), request.clone());
        }
    }

    /// Release the network task before waiting for a user decision. A policy
    /// can have arrived while this file was running, so check it again here.
    pub(super) fn conflict(
        &mut self,
        id: Uuid,
        local: crate::sftp::LocalDownloadFingerprint,
    ) -> Option<SftpDownloadRequest> {
        let mut request = self.active.remove(&id)?;
        request.set_expected_local(local);
        if let Some(&choice) = request
            .conflict_batch()
            .and_then(|batch| self.policies.get(&batch))
        {
            request.resolve_conflict(choice);
            Some(request)
        } else {
            self.pending.insert(id, request);
            None
        }
    }

    pub(super) fn finish(&mut self, id: Uuid) {
        self.active.remove(&id);
    }

    pub(super) fn cancel(&mut self, id: Uuid) -> bool {
        self.pending.remove(&id).is_some()
    }

    pub(super) fn resolve(
        &mut self,
        id: Uuid,
        choice: SftpConflictChoice,
        apply_to_batch: bool,
    ) -> Vec<SftpDownloadRequest> {
        let Some(mut request) = self.pending.remove(&id) else {
            return Vec::new();
        };
        let batch = request.conflict_batch();
        if apply_to_batch && let Some(batch) = batch {
            if !self.policies.contains_key(&batch) {
                if self.recent_batches.len() >= 32
                    && let Some(oldest) = self.recent_batches.pop_front()
                {
                    self.policies.remove(&oldest);
                }
                self.recent_batches.push_back(batch);
            }
            self.policies.insert(batch, choice);
        }
        request.resolve_conflict(choice);
        let mut resolved = vec![request];
        if apply_to_batch {
            let ids = self
                .pending
                .iter()
                .filter_map(|(id, pending)| (pending.conflict_batch() == batch).then_some(*id))
                .collect::<Vec<_>>();
            for id in ids {
                if let Some(mut pending) = self.pending.remove(&id) {
                    pending.resolve_conflict(choice);
                    resolved.push(pending);
                }
            }
        }
        resolved
    }
}

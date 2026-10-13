//! Tab refresh state without generated UI types.

use super::*;

impl AppState {
    pub(in crate::app) fn request_full_ui_refresh(&mut self) -> bool {
        self.request_ui_refresh(None, None, false)
    }

    pub(in crate::app) fn request_terminal_ui_refresh(
        &mut self,
        tab_id: Uuid,
        output_received_at: Option<Instant>,
    ) -> bool {
        self.request_ui_refresh(Some(tab_id), output_received_at, false)
    }

    pub(in crate::app) fn request_tab_ui_refresh(&mut self) -> bool {
        self.request_ui_refresh(None, None, true)
    }

    fn request_ui_refresh(
        &mut self,
        terminal_id: Option<Uuid>,
        output_received_at: Option<Instant>,
        tabs_only: bool,
    ) -> bool {
        if tabs_only {
            self.ui_refresh.tabs = true;
        } else if let Some(tab_id) = terminal_id {
            if !self.ui_refresh.full {
                self.ui_refresh.terminal_ids.insert(tab_id);
            }
        } else {
            self.ui_refresh.full = true;
            self.ui_refresh.terminal_ids.clear();
        }
        if let Some(received_at) = output_received_at {
            self.ui_refresh.earliest_output_received_at = Some(
                self.ui_refresh
                    .earliest_output_received_at
                    .map_or(received_at, |current| current.min(received_at)),
            );
        }

        if !self.ui_refresh.pending {
            self.ui_refresh.generation = self.ui_refresh.generation.saturating_add(1);
            self.ui_refresh.pending = true;
            return true;
        }

        self.ui_refresh.coalesced_requests = self.ui_refresh.coalesced_requests.saturating_add(1);
        if self.ui_refresh.in_progress {
            // The UI has already taken its batch, so this mutation cannot be
            // represented by that snapshot and requires one bounded follow-up.
            self.ui_refresh.generation = self.ui_refresh.generation.saturating_add(1);
        }
        false
    }

    pub(in crate::app) fn take_ui_refresh_batch(&mut self) -> Option<UiRefreshBatch> {
        if !self.ui_refresh.pending || self.ui_refresh.in_progress {
            return None;
        }
        self.ui_refresh.in_progress = true;
        Some(UiRefreshBatch {
            generation: self.ui_refresh.generation,
            full: std::mem::take(&mut self.ui_refresh.full),
            tabs: std::mem::take(&mut self.ui_refresh.tabs),
            terminal_ids: std::mem::take(&mut self.ui_refresh.terminal_ids),
            earliest_output_received_at: self.ui_refresh.earliest_output_received_at.take(),
            coalesced_requests: std::mem::take(&mut self.ui_refresh.coalesced_requests),
        })
    }

    pub(in crate::app) fn finish_ui_refresh(&mut self, generation: u64) -> bool {
        self.ui_refresh.in_progress = false;
        if self.ui_refresh.generation == generation {
            self.ui_refresh.pending = false;
            false
        } else {
            true
        }
    }

    pub(in crate::app) fn cancel_ui_refresh(&mut self) {
        self.ui_refresh = UiRefreshState::default();
    }
}

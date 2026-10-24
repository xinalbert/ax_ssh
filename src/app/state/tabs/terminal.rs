//! Tab terminal state without generated UI types.

use super::*;

impl AppState {
    pub(in crate::app) fn active_terminal_mut(&mut self) -> Option<&mut TerminalTabState> {
        self.active_tab_id.and_then(|id| self.terminal_mut(id))
    }

    #[cfg(test)]
    pub(in crate::app) fn resize_active_terminal(&mut self, columns: u32, rows: u32) -> Result<()> {
        let tab_id = self.active_tab_id.context("no active terminal")?;
        self.resize_terminal(tab_id, columns, rows).map(|_| ())
    }

    #[cfg(test)]
    pub(in crate::app) fn resize_terminal(
        &mut self,
        tab_id: Uuid,
        columns: u32,
        rows: u32,
    ) -> Result<bool> {
        self.resize_terminal_with_metrics(tab_id, columns, rows, 0, 0)
    }

    pub(in crate::app) fn resize_terminal_with_metrics(
        &mut self,
        tab_id: Uuid,
        columns: u32,
        rows: u32,
        cell_width: u32,
        cell_height: u32,
    ) -> Result<bool> {
        let terminal = self
            .terminal_mut(tab_id)
            .context("terminal tab not found")?;
        let current_size = terminal
            .terminal
            .as_ref()
            .context("terminal tab has no terminal model")?;
        let size = TerminalSize::model_with_pixels(
            columns as usize,
            rows as usize,
            columns.saturating_mul(cell_width),
            rows.saturating_mul(cell_height),
        );
        let characters_changed = current_size.size().columns() != size.columns()
            || current_size.size().rows() != size.rows();
        let has_window_metrics = cell_width > 0 && cell_height > 0;
        if !characters_changed && !has_window_metrics {
            return Ok(false);
        }
        if let Some(worker) = terminal.worker.as_ref() {
            worker.request_resize_with_pixels(
                size.columns(),
                size.rows(),
                size.pixel_width(),
                size.pixel_height(),
            )?;
        }
        let model = terminal
            .terminal
            .as_mut()
            .context("terminal tab has no terminal model")?;
        let responses = if has_window_metrics {
            model.set_window_size(size.columns(), size.rows(), cell_width, cell_height)
        } else {
            Vec::new()
        };
        let sent_protocol_response = !responses.is_empty();
        if characters_changed {
            terminal.viewport.resize(model, size);
        }
        if !responses.is_empty() {
            let worker = terminal
                .worker
                .as_ref()
                .context("terminal protocol response has no transport worker")?;
            for response in responses {
                worker
                    .request_send(response)
                    .context("cannot queue terminal protocol response")?;
            }
        }
        Ok(characters_changed || sent_protocol_response)
    }

    pub(in crate::app) fn scroll_terminal(&mut self, tab_id: Uuid, lines: i32) -> bool {
        let Some(terminal) = self.terminal_mut(tab_id) else {
            return false;
        };
        terminal
            .terminal
            .as_mut()
            .is_some_and(|model| terminal.viewport.scroll(model, lines))
    }

    pub(in crate::app) fn scroll_terminal_to_bottom(&mut self, tab_id: Uuid) -> bool {
        let Some(terminal) = self.terminal_mut(tab_id) else {
            return false;
        };
        terminal
            .terminal
            .as_mut()
            .is_some_and(|model| terminal.viewport.scroll_to_bottom(model))
    }

    pub(in crate::app) fn pane_session_source(&self, tab_id: Uuid) -> Option<PaneSessionSource> {
        let terminal = self.terminal(tab_id)?;
        if terminal.is_sftp() {
            return None;
        }
        if terminal.is_local() {
            return Some(PaneSessionSource::LocalShell);
        }
        terminal
            .profile_id()
            .map(PaneSessionSource::ProfileConnection)
    }

    pub(in crate::app) fn terminal_companion_id(&self, tab_id: Uuid) -> Option<Uuid> {
        let companion_id = self
            .tabs
            .iter()
            .find(|tab| tab.id == tab_id)?
            .companion_tab_id?;
        self.terminal(companion_id)
            .is_some_and(|terminal| !terminal.is_sftp())
            .then_some(companion_id)
    }

    pub(in crate::app) fn sftp_companion_id(&self, tab_id: Uuid) -> Option<Uuid> {
        let companion_id = self
            .tabs
            .iter()
            .find(|tab| tab.id == tab_id)?
            .companion_tab_id?;
        self.terminal(companion_id)
            .is_some_and(TerminalTabState::is_sftp)
            .then_some(companion_id)
    }

    pub(in crate::app) fn terminal(&self, tab_id: Uuid) -> Option<&TerminalTabState> {
        self.tabs.iter().find_map(|tab| {
            if tab.id != tab_id {
                return None;
            }
            match &tab.kind {
                WorkspaceTabKind::Terminal(terminal) => Some(terminal.as_ref()),
                WorkspaceTabKind::Settings | WorkspaceTabKind::SessionEditor(_) => None,
            }
        })
    }

    pub(in crate::app) fn terminal_mut(&mut self, tab_id: Uuid) -> Option<&mut TerminalTabState> {
        self.tabs.iter_mut().find_map(|tab| {
            if tab.id != tab_id {
                return None;
            }
            match &mut tab.kind {
                WorkspaceTabKind::Terminal(terminal) => Some(terminal.as_mut()),
                WorkspaceTabKind::Settings | WorkspaceTabKind::SessionEditor(_) => None,
            }
        })
    }

    pub(in crate::app) fn apply_terminal_title(
        &mut self,
        tab_id: Uuid,
        title: Option<String>,
    ) -> bool {
        let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == tab_id) else {
            return false;
        };
        if !matches!(tab.kind, WorkspaceTabKind::Terminal(_)) {
            return false;
        }
        let next = title.unwrap_or_else(|| tab.default_title.clone());
        if tab.title == next {
            return false;
        }
        tab.title = next;
        true
    }

    pub(in crate::app) fn apply_scrollback_setting(&mut self) {
        let scrollback_lines = self.sessions.settings.terminal.scrollback_lines as usize;
        let osc52_clipboard = self.sessions.settings.terminal.osc52_clipboard;
        for tab in &mut self.tabs {
            if let WorkspaceTabKind::Terminal(terminal) = &mut tab.kind
                && let Some(model) = terminal.terminal.as_mut()
            {
                model.set_scrollback_lines(scrollback_lines);
                model.set_osc52_clipboard_enabled(osc52_clipboard);
                if !osc52_clipboard {
                    terminal.clear_pending_clipboard_read();
                }
                terminal.discard_pending_terminal_snapshot();
            }
        }
    }

    pub(in crate::app) fn set_terminal_query_palette(&mut self, palette: TerminalQueryPalette) {
        self.terminal_query_palette = palette;
        for tab in &mut self.tabs {
            if let WorkspaceTabKind::Terminal(terminal) = &mut tab.kind
                && let Some(model) = terminal.terminal.as_mut()
            {
                model.set_query_palette(palette);
            }
        }
    }
}

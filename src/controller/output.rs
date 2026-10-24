//! Owned effects from parsing one transport output batch, without UI actions.

use std::time::Duration;

use crate::terminal::{ClipboardLoadFormatter, TerminalModel};

/// Parser effects that the session owner must route under its current identity.
///
/// Clipboard requests are still subject to the model's permission gate. This
/// value contains terminal data and deliberately does not implement `Debug`.
pub struct TerminalOutput {
    pub responses: Vec<Vec<u8>>,
    pub title_update: Option<Option<String>>,
    pub bell: bool,
    pub clipboard_store: Option<String>,
    pub clipboard_load: Option<ClipboardLoadFormatter>,
    pub clipboard_permission_generation: u64,
    pub presentation_hold: Option<Duration>,
}

impl TerminalOutput {
    /// Parses bytes in order and drains each transient effect exactly once.
    pub fn process(model: &mut TerminalModel, data: &[u8]) -> Self {
        Self {
            responses: model.process_with_responses(data),
            title_update: model.take_title_update(),
            bell: model.take_bell(),
            clipboard_store: model.take_clipboard_store(),
            clipboard_load: model.take_clipboard_load(),
            clipboard_permission_generation: model.osc52_permission_generation(),
            presentation_hold: model.synchronized_output_remaining(),
        }
    }
}

#[cfg(test)]
mod tests;

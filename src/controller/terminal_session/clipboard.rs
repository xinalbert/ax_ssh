//! Bounded OSC 52 state. Only a matching, unexpired request can send a response.
use super::*;
use crate::terminal::{ClipboardLoadFormatter, MAX_OSC52_CLIPBOARD_BYTES};

const READ_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Default)]
pub(super) struct SharedClipboard {
    generation: u64,
    enabled: bool,
    request: Option<Uuid>,
    deadline: Option<tokio::time::Instant>,
    store: Option<String>,
}

#[cfg(test)]
pub(super) mod tests;

pub(super) struct PendingRead {
    id: Uuid,
    permission: u64,
    pub(super) deadline: tokio::time::Instant,
    formatter: ClipboardLoadFormatter,
}

impl TerminalClient {
    /// Returns only the identity of an outstanding read, never clipboard contents.
    pub fn clipboard_read(&self) -> Result<Option<Uuid>> {
        let shared = self
            .shared
            .lock()
            .map_err(|_| anyhow::anyhow!("clipboard state lock poisoned"))?;
        Ok((self.accepts_clipboard(&shared)
            && shared
                .remote
                .deadline
                .is_some_and(|deadline| tokio::time::Instant::now() < deadline))
        .then_some(shared.remote.request)
        .flatten())
    }

    /// Consumes one remote write under the current connection and saved permission.
    pub fn take_remote_clipboard(&self) -> Result<Option<String>> {
        let mut shared = self
            .shared
            .lock()
            .map_err(|_| anyhow::anyhow!("clipboard state lock poisoned"))?;
        if !self.accepts_clipboard(&shared) {
            return Ok(None);
        }
        Ok(shared.remote.store.take())
    }

    fn accepts_clipboard(&self, shared: &Shared) -> bool {
        shared.remote.enabled
            && shared.view.connected
            && shared.remote.generation == shared.view.generation
            && self
                .generation
                .is_none_or(|generation| generation == shared.remote.generation)
    }

    /// Answers exactly one read. None denies without accessing the native clipboard.
    /// `revision` is the viewport captured when the user approved the read;
    /// the model owner checks it even before publishing a changed viewport.
    pub fn respond_clipboard(&self, id: Uuid, text: Option<String>, revision: i32) -> Result<()> {
        anyhow::ensure!(
            text.as_ref()
                .is_none_or(|text| text.len() <= MAX_OSC52_CLIPBOARD_BYTES),
            "clipboard exceeds the OSC 52 limit (64 KiB)"
        );
        self.send(Command::Clipboard(id, text, revision))
    }
}

impl Worker {
    pub(super) fn revoke_remote_read(&mut self) -> Result<()> {
        self.remote_read = None;
        let mut shared = self
            .shared
            .lock()
            .map_err(|_| anyhow::anyhow!("clipboard state lock poisoned"))?;
        shared.remote.request = None;
        shared.remote.deadline = None;
        drop(shared);
        (self.wake)();
        Ok(())
    }

    pub(super) fn clear_remote_clipboard(&mut self) -> Result<()> {
        self.remote_read = None;
        self.shared
            .lock()
            .map_err(|_| anyhow::anyhow!("clipboard state lock poisoned"))?
            .remote = SharedClipboard {
            generation: self.generation,
            enabled: self.connected && self.model.osc52_clipboard_enabled(),
            ..Default::default()
        };
        (self.wake)();
        Ok(())
    }

    pub(super) fn offer_clipboard(&mut self, effects: &mut TerminalOutput) -> Result<()> {
        if !self.connected
            || !self.model.osc52_clipboard_enabled()
            || effects.clipboard_permission_generation != self.model.osc52_permission_generation()
        {
            return Ok(());
        }
        let mut shared = self
            .shared
            .lock()
            .map_err(|_| anyhow::anyhow!("clipboard state lock poisoned"))?;
        shared.remote.generation = self.generation;
        shared.remote.enabled = true;
        if let Some(text) = effects.clipboard_store.take() {
            shared.remote.store = Some(text);
        }
        if let Some(formatter) = effects.clipboard_load.take() {
            let id = Uuid::new_v4();
            self.remote_read = Some(PendingRead {
                id,
                permission: effects.clipboard_permission_generation,
                deadline: tokio::time::Instant::now() + READ_TIMEOUT,
                formatter,
            });
            shared.remote.request = Some(id);
            shared.remote.deadline = self.remote_read.as_ref().map(|read| read.deadline);
        }
        drop(shared);
        (self.wake)();
        Ok(())
    }

    pub(super) fn answer_clipboard(
        &mut self,
        id: Uuid,
        text: Option<String>,
        revision: i32,
    ) -> Result<()> {
        if !self.remote_read.as_ref().is_some_and(|read| read.id == id) {
            return Ok(());
        }
        let Some(read) = self.remote_read.take() else {
            return Ok(());
        };
        self.shared
            .lock()
            .map_err(|_| anyhow::anyhow!("clipboard state lock poisoned"))?
            .remote
            .request = None;
        (self.wake)();
        if self.connected
            && revision == self.viewport.selection_revision()
            && self.model.osc52_clipboard_enabled()
            && read.permission == self.model.osc52_permission_generation()
            && tokio::time::Instant::now() < read.deadline
            && let Some(text) = text
        {
            anyhow::ensure!(
                text.len() <= MAX_OSC52_CLIPBOARD_BYTES,
                "clipboard exceeds the OSC 52 limit (64 KiB)"
            );
            let bytes = (read.formatter)(&text).into_bytes();
            anyhow::ensure!(
                bytes.len() <= MAX_OSC52_CLIPBOARD_BYTES.div_ceil(3) * 4 + 32,
                "OSC 52 response is too large"
            );
            // Atomic/chunked raw protocol response; never paste-encode or log it.
            self.transport()?.paste(bytes)?;
        }
        Ok(())
    }
}

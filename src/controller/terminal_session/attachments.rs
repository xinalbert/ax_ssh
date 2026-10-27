//! Session-owned attachment work; only an explicit current UI ticket can insert a path.
use super::*;
use crate::media::{MAX_ATTACHMENTS, MAX_IMAGE_BYTES, PrivateMediaFile};
use std::io::Write;

#[derive(Clone, Copy)]
pub(super) struct Ticket {
    id: Uuid,
    generation: u64,
    revision: i32,
    ready: bool,
    cancelled: bool,
}

pub(super) struct Prepared {
    path: String,
    file: Option<PrivateMediaFile>,
}

enum Job {
    Local(JoinHandle<Result<Prepared>>),
    Ssh(tokio::sync::oneshot::Receiver<Result<String>>),
}

#[derive(Default)]
pub(super) struct Attachments {
    job: Option<Job>,
    ticket: Option<Ticket>,
    prepared: Option<Prepared>,
    files: Vec<PrivateMediaFile>,
}

impl Attachments {
    pub(super) fn busy(&self) -> bool {
        self.job.is_some()
    }

    pub(super) async fn next(&mut self) -> Result<Prepared> {
        match &mut self.job {
            Some(Job::Local(task)) => task.await.context("attachment file task failed")?,
            Some(Job::Ssh(reply)) => reply
                .await
                .context("attachment SSH worker stopped")?
                .map(|path| Prepared { path, file: None }),
            None => std::future::pending().await,
        }
    }
}

impl TerminalClient {
    /// Admits one bounded PNG for this connection. Preparation never types into the PTY.
    pub fn prepare_image(&self, id: Uuid, png: Vec<u8>, revision: i32) -> Result<()> {
        anyhow::ensure!(
            png.len() <= MAX_IMAGE_BYTES && png.starts_with(b"\x89PNG\r\n\x1a\n"),
            "invalid or oversized PNG attachment"
        );
        {
            let mut shared = self
                .shared
                .lock()
                .map_err(|_| anyhow::anyhow!("attachment state lock poisoned"))?;
            let view = &shared.view;
            anyhow::ensure!(
                view.connected
                    && self
                        .generation
                        .is_none_or(|generation| generation == view.generation)
                    && view.selection_revision == revision,
                "attachment target changed"
            );
            anyhow::ensure!(
                matches!(
                    view.identity,
                    ConnectionIdentity::Local | ConnectionIdentity::Ssh { .. }
                ),
                "image attachments require Local Shell or SSH"
            );
            anyhow::ensure!(shared.image.is_none(), "another image attachment is busy");
            shared.image = Some(Ticket {
                id,
                generation: view.generation,
                revision,
                ready: false,
                cancelled: false,
            });
        }
        if let Err(error) = self.send(Command::PrepareImage(id, png, revision)) {
            // No worker owns this failed admission, so release its reservation.
            let mut shared = self
                .shared
                .lock()
                .map_err(|_| anyhow::anyhow!("attachment state lock poisoned"))?;
            if shared.image.is_some_and(|ticket| ticket.id == id) {
                shared.image = None;
            }
            return Err(error);
        }
        Ok(())
    }

    /// Returns None when the ticket has been revoked; no paths enter UI snapshots.
    pub fn image_ready(&self, id: Uuid) -> Result<Option<bool>> {
        let shared = self
            .shared
            .lock()
            .map_err(|_| anyhow::anyhow!("attachment state lock poisoned"))?;
        Ok(shared
            .image
            .filter(|ticket| {
                ticket.id == id
                    && !ticket.cancelled
                    && ticket.generation == shared.view.generation
                    && self
                        .generation
                        .is_none_or(|generation| generation == ticket.generation)
            })
            .map(|ticket| ticket.ready))
    }

    fn revoke_image(&self, id: Uuid) -> Result<()> {
        let mut shared = self
            .shared
            .lock()
            .map_err(|_| anyhow::anyhow!("attachment state lock poisoned"))?;
        if let Some(ticket) = shared.image.as_mut().filter(|ticket| ticket.id == id) {
            ticket.cancelled = true;
        }
        Ok(())
    }

    /// Cancelling revokes synchronously, even if a confirm intent is already queued.
    pub fn finish_image(&self, id: Uuid, accept: bool) -> Result<()> {
        if !accept {
            self.revoke_image(id)?;
        }
        self.send(Command::FinishImage(id, accept))
    }
}

impl Worker {
    fn image_current(&self, ticket: Ticket) -> Result<bool> {
        Ok(self.connected
            && ticket.generation == self.generation
            && ticket.revision == self.viewport.selection_revision()
            && self
                .shared
                .lock()
                .map_err(|_| anyhow::anyhow!("attachment state lock poisoned"))?
                .image
                .is_some_and(|current| current.id == ticket.id && !current.cancelled))
    }

    pub(super) fn prepare_image(&mut self, id: Uuid, png: Vec<u8>, revision: i32) -> Result<()> {
        let ticket = Ticket {
            id,
            generation: self.generation,
            revision,
            ready: false,
            cancelled: false,
        };
        if !self.image_current(ticket)? {
            self.clear_image_ticket(id)?;
            return Ok(());
        }
        let result = (|| {
            anyhow::ensure!(
                self.attachments.job.is_none() && self.attachments.prepared.is_none(),
                "another image attachment is busy"
            );
            anyhow::ensure!(
                self.attachments.files.len() < MAX_ATTACHMENTS,
                "session attachment limit reached (32)"
            );
            let job = match self.transport()? {
                Transport::Local(_) => Job::Local(tokio::task::spawn_blocking(move || {
                    let (owner, mut file) = PrivateMediaFile::create("png")?;
                    file.write_all(&png)
                        .context("cannot write clipboard image")?;
                    file.flush().context("cannot finish clipboard image")?;
                    let path = owner
                        .path()
                        .to_str()
                        .context("attachment path is not Unicode")?
                        .to_owned();
                    Ok(Prepared {
                        path,
                        file: Some(owner),
                    })
                })),
                Transport::Ssh(worker) => Job::Ssh(worker.request_clipboard_image(png)?),
                _ => bail!("image attachments require Local Shell or SSH"),
            };
            self.attachments.job = Some(job);
            self.attachments.ticket = Some(ticket);
            Ok(())
        })();
        if result.is_err() {
            self.clear_image_ticket(id)?;
        }
        result
    }

    pub(super) fn clear_image_ticket(&self, id: Uuid) -> Result<()> {
        let mut shared = self
            .shared
            .lock()
            .map_err(|_| anyhow::anyhow!("attachment state lock poisoned"))?;
        if shared.image.is_some_and(|ticket| ticket.id == id) {
            shared.image = None;
        }
        Ok(())
    }

    pub(super) async fn image_prepared(&mut self, result: Result<Prepared>) -> Result<()> {
        self.attachments.job = None;
        let Some(ticket) = self.attachments.ticket else {
            if let Ok(prepared) = result {
                dispose(prepared.file.into_iter().collect()).await?;
            }
            return Ok(());
        };
        match result {
            Ok(prepared) if self.image_current(ticket)? => {
                self.attachments.prepared = Some(prepared);
                if let Some(current) = self
                    .shared
                    .lock()
                    .map_err(|_| anyhow::anyhow!("attachment state lock poisoned"))?
                    .image
                    .as_mut()
                    .filter(|current| current.id == ticket.id)
                {
                    current.ready = true;
                }
                Ok(())
            }
            Ok(prepared) => {
                self.clear_image_ticket(ticket.id)?;
                dispose(prepared.file.into_iter().collect()).await
            }
            Err(error) => {
                let current = self.image_current(ticket)?;
                self.clear_image_ticket(ticket.id)?;
                if current { Err(error) } else { Ok(()) }
            }
        }
    }

    pub(super) async fn finish_image(&mut self, id: Uuid, accept: bool) -> Result<()> {
        let Some(ticket) = self.attachments.ticket.filter(|ticket| ticket.id == id) else {
            return Ok(());
        };
        // Cancellation leaves a running blocking job owned until it completes.
        if self.attachments.job.is_some() {
            return Ok(());
        }
        let prepared = self.attachments.prepared.take();
        self.attachments.ticket = None;
        // Commit or revoke under the same lock used by synchronous cancellation.
        // A cancellation ordered before this commit must defeat even a queued confirm.
        let insert = {
            let mut shared = self
                .shared
                .lock()
                .map_err(|_| anyhow::anyhow!("attachment state lock poisoned"))?;
            let current = shared.image.filter(|current| current.id == id);
            if current.is_some() {
                shared.image = None;
            }
            accept
                && self.connected
                && ticket.generation == self.generation
                && ticket.revision == self.viewport.selection_revision()
                && current.is_some_and(|current| !current.cancelled && current.ready)
        };
        if let Some(prepared) = prepared {
            let result = if insert {
                self.command(Command::Paste(attachment_text(&prepared.path)))
            } else {
                Ok(())
            };
            if insert && result.is_ok() {
                if let Some(file) = prepared.file {
                    self.attachments.files.push(file);
                }
            } else {
                dispose(prepared.file.into_iter().collect()).await?;
            }
            result?;
        }
        Ok(())
    }

    pub(super) async fn stop_images(&mut self) -> Result<()> {
        self.shared
            .lock()
            .map_err(|_| anyhow::anyhow!("attachment state lock poisoned"))?
            .image = None;
        let mut files = std::mem::take(&mut self.attachments.files);
        let mut joined = Ok(());
        // SSH upload joins belong to the existing SSH worker; dropping its reply
        // never detaches that work. Local blocking work must be joined here.
        if let Some(Job::Local(task)) = self.attachments.job.take() {
            match task.await {
                Ok(Ok(prepared)) => files.extend(prepared.file),
                Ok(Err(error)) => {
                    tracing::debug!(%error, "cancelled attachment preparation failed")
                }
                Err(error) => joined = Err(error).context("attachment file task failed"),
            }
        }
        if let Some(prepared) = self.attachments.prepared.take() {
            files.extend(prepared.file);
        }
        self.attachments.ticket = None;
        let cleanup = dispose(files).await;
        joined.and(cleanup)
    }
}

async fn dispose(files: Vec<PrivateMediaFile>) -> Result<()> {
    if !files.is_empty() {
        tokio::task::spawn_blocking(move || drop(files))
            .await
            .context("attachment cleanup task failed")?;
    }
    Ok(())
}

pub(super) fn attachment_text(path: &str) -> String {
    if path
        .chars()
        .all(|c| c.is_alphanumeric() || "/._-".contains(c))
    {
        path.to_owned()
    } else {
        format!("'{}'", path.replace('\'', "'\\''"))
    }
}

#[cfg(test)]
mod tests;

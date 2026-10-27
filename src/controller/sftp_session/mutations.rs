//! One retained remote mutation receipt; the existing write worker owns I/O.
use super::*;
use crate::sftp::{SftpWriteEvent, SftpWriteOperation};

const MAX_NAME_CHARS: usize = 512;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SftpEntryAction {
    Rename(String),
    Delete,
}

pub(super) struct Slot {
    id: Uuid,
    generation: u64,
    abandoned: bool,
    result: Option<std::result::Result<(), String>>,
}

#[derive(Clone)]
pub(super) struct Target {
    request_id: u64,
    path: String,
    directory: bool,
    size: u64,
    modified: Option<u32>,
}

pub(super) struct Pending {
    id: Uuid,
    target: Target,
    completed_path: String,
}

fn parent(path: &str) -> &str {
    match path.rsplit_once('/') {
        Some(("", _)) => "/",
        Some((parent, _)) => parent,
        None => "",
    }
}

fn directory(path: &str) -> &str {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() { "/" } else { trimmed }
}

fn operation(target: &Target, action: &SftpEntryAction) -> Result<(SftpWriteOperation, String)> {
    crate::sftp::validate_remote_path(&target.path)?;
    let leaf = target.path.rsplit('/').next().unwrap_or_default();
    anyhow::ensure!(
        !matches!(leaf, "" | "." | ".."),
        "invalid remote entry path"
    );
    match action {
        SftpEntryAction::Delete => Ok((
            SftpWriteOperation::Remove {
                path: target.path.clone(),
                directory: target.directory,
            },
            target.path.clone(),
        )),
        SftpEntryAction::Rename(name) => {
            let name = name.trim();
            anyhow::ensure!(
                !matches!(name, "" | "." | "..")
                    && name.chars().count() <= MAX_NAME_CHARS
                    && !name.contains(['/', '\\'])
                    && !name.chars().any(char::is_control),
                "remote name is invalid"
            );
            let new_path = format!("{}/{name}", parent(&target.path).trim_end_matches('/'));
            crate::sftp::validate_remote_path(&new_path)?;
            anyhow::ensure!(
                new_path != target.path,
                "old and new remote paths are identical"
            );
            Ok((
                SftpWriteOperation::Rename {
                    old_path: target.path.clone(),
                    new_path: new_path.clone(),
                },
                new_path,
            ))
        }
    }
}

impl SftpClient {
    /// Submit one frozen visible entry; its receipt occupies the slot until taken or abandoned.
    pub fn mutate_entry(
        &self,
        request_id: u64,
        entry: &SftpEntryView,
        action: SftpEntryAction,
    ) -> Result<Uuid> {
        anyhow::ensure!(
            !entry.is_symlink,
            "symbolic links cannot be modified through this action"
        );
        crate::sftp::validate_remote_path(&entry.path)?;
        let target = Target {
            request_id,
            path: entry.path.clone(),
            directory: entry.is_dir,
            size: entry.size,
            modified: entry.modified,
        };
        operation(&target, &action)?;
        let mut shared = self
            .shared
            .lock()
            .map_err(|_| anyhow::anyhow!("SFTP view lock poisoned"))?;
        anyhow::ensure!(
            shared.mutation.is_none(),
            "another remote entry operation is awaiting completion"
        );
        let id = Uuid::new_v4();
        let generation = self.generation.unwrap_or(shared.view.generation);
        self.commands
            .try_send(QueuedCommand {
                generation,
                command: Command::Mutate { id, target, action },
            })
            .map_err(|_| anyhow::anyhow!("SFTP companion queue is full or closed"))?;
        shared.mutation = Some(Slot {
            id,
            generation,
            abandoned: false,
            result: None,
        });
        Ok(id)
    }

    pub fn take_entry_result(&self, id: Uuid) -> Result<Option<std::result::Result<(), String>>> {
        let mut shared = self
            .shared
            .lock()
            .map_err(|_| anyhow::anyhow!("SFTP view lock poisoned"))?;
        let Some(slot) = shared.mutation.as_mut().filter(|slot| slot.id == id) else {
            return Ok(None);
        };
        let result = slot.result.take();
        if result.is_some() {
            shared.mutation = None;
        }
        Ok(result)
    }

    /// Does not roll back a write already admitted by the native worker.
    pub fn abandon_entry_result(&self, id: Uuid) -> Result<()> {
        let mut shared = self
            .shared
            .lock()
            .map_err(|_| anyhow::anyhow!("SFTP view lock poisoned"))?;
        if let Some(slot) = shared.mutation.as_mut().filter(|slot| slot.id == id) {
            if slot.result.is_some() {
                shared.mutation = None;
            } else {
                slot.abandoned = true;
            }
        }
        Ok(())
    }
}

impl Worker {
    pub(super) fn finish_mutation(
        &mut self,
        id: Uuid,
        result: std::result::Result<(), String>,
    ) -> Result<()> {
        let mut shared = self
            .shared
            .lock()
            .map_err(|_| anyhow::anyhow!("SFTP view lock poisoned"))?;
        if let Some(slot) = shared.mutation.as_mut().filter(|slot| slot.id == id) {
            if slot.abandoned {
                shared.mutation = None;
            } else {
                slot.result = Some(result);
            }
        }
        Ok(())
    }

    pub(super) fn reset_mutation(&mut self) -> Result<()> {
        self.mutation = None;
        self.shared
            .lock()
            .map_err(|_| anyhow::anyhow!("SFTP view lock poisoned"))?
            .mutation = None;
        Ok(())
    }

    pub(super) fn mutate_entry(
        &mut self,
        id: Uuid,
        target: Target,
        action: SftpEntryAction,
        generation: u64,
    ) -> Result<()> {
        let current = self
            .shared
            .lock()
            .map_err(|_| anyhow::anyhow!("SFTP view lock poisoned"))?
            .mutation
            .as_ref()
            .is_some_and(|slot| slot.id == id && slot.generation == generation && !slot.abandoned);
        if !current {
            return self.finish_mutation(id, Err("remote entry operation was abandoned".into()));
        }
        let result = self.admit_mutation(id, target, action, generation);
        if let Err(error) = result {
            let message: String = format!("{error:#}").chars().take(512).collect();
            self.status = message.clone();
            self.finish_mutation(id, Err(message))?;
        }
        Ok(())
    }

    fn admit_mutation(
        &mut self,
        id: Uuid,
        target: Target,
        action: SftpEntryAction,
        generation: u64,
    ) -> Result<()> {
        anyhow::ensure!(
            generation == self.generation
                && self.connected
                && !self.loading
                && self.prompt.is_none(),
            "remote directory is no longer ready"
        );
        anyhow::ensure!(
            self.mutation.is_none(),
            "another remote entry operation is running"
        );
        anyhow::ensure!(
            target.request_id == self.request_id && parent(&target.path) == directory(&self.path),
            "remote directory changed"
        );
        let entry = self
            .entries
            .iter()
            .find(|entry| entry.path == target.path)
            .context("remote entry is no longer visible")?;
        anyhow::ensure!(
            !entry.is_symlink
                && entry.is_dir == target.directory
                && entry.size == target.size
                && entry.modified == target.modified,
            "remote entry changed"
        );
        let (operation, completed_path) = operation(&target, &action)?;
        self.transport
            .as_ref()
            .context("SFTP worker is unavailable")?
            .request_sftp_write(id, operation)?;
        self.mutation = Some(Pending {
            id,
            target,
            completed_path,
        });
        self.status = "Remote entry operation queued".into();
        Ok(())
    }

    pub(super) fn mutation_event(&mut self, event: &SftpWriteEvent) -> Result<bool> {
        let Some(pending) = self.mutation.as_ref() else {
            return Ok(false);
        };
        let result = match event {
            SftpWriteEvent::Completed { operation_id, path } if *operation_id == pending.id => {
                if path == &pending.completed_path {
                    Ok(())
                } else {
                    Err("unexpected remote entry completion path".into())
                }
            }
            SftpWriteEvent::Failed {
                operation_id,
                message,
            } if *operation_id == pending.id => Err(message.chars().take(512).collect::<String>()),
            _ => return Ok(false),
        };
        let Some(pending) = self.mutation.take() else {
            return Ok(false);
        };
        let succeeded = result.is_ok();
        self.status = match &result {
            Ok(()) => "Remote entry operation completed".into(),
            Err(message) => message.clone(),
        };
        self.finish_mutation(pending.id, result)?;
        if succeeded
            && self.connected
            && !self.loading
            && self.request_id == pending.target.request_id
            && directory(&self.path) == parent(&pending.target.path)
        {
            self.next_request();
            match self
                .transport
                .as_ref()
                .context("SFTP worker is unavailable")?
                .request_list_sftp(self.request_id, self.path.clone())
            {
                Ok(()) => self.loading = true,
                Err(error) => {
                    self.status = format!("Remote entry changed; refresh failed: {error:#}")
                        .chars()
                        .take(512)
                        .collect()
                }
            }
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests;

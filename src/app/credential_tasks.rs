//! Tokio boundary for synchronous platform credential operations.

use anyhow::{Context, Result};
use tokio::time::{Duration, timeout};
use tracing::warn;
use uuid::Uuid;
use zeroize::Zeroizing;

use ax_ssh::config::CredentialStorage;
use ax_ssh::credentials::{CredentialBackup, CredentialStore};

const OPERATION_TIMEOUT: Duration = Duration::from_secs(20);

pub(in crate::app) fn credential_storage_for_save(
    requested: CredentialStorage,
    vault_password_supplied: bool,
) -> CredentialStorage {
    if requested == CredentialStorage::EncryptedVault && !vault_password_supplied {
        CredentialStorage::SystemKeyring
    } else {
        requested
    }
}

pub(super) async fn load_system_password(session_id: Uuid) -> Result<Option<Zeroizing<String>>> {
    run_read(move || CredentialStore::platform()?.load_system_password(session_id))
        .await
        .map(|password| password.map(Zeroizing::new))
}

pub(super) async fn load_vault_password(
    session_id: Uuid,
    vault_password: Zeroizing<String>,
) -> Result<Option<Zeroizing<String>>> {
    run_read(move || CredentialStore::platform()?.load_vault_password(session_id, &vault_password))
        .await
        .map(|password| password.map(Zeroizing::new))
}

pub(super) async fn save_password(
    storage: CredentialStorage,
    session_id: Uuid,
    password: Zeroizing<String>,
    vault_password: Option<Zeroizing<String>>,
    previous_storage: Option<CredentialStorage>,
) -> Result<CredentialRollback> {
    run_mutation(move || {
        let store = CredentialStore::platform()?;
        let mut backups = vec![(storage, store.backup(storage, session_id)?)];
        if let Some(previous_storage) = previous_storage.filter(|value| *value != storage) {
            backups.push((
                previous_storage,
                store.backup(previous_storage, session_id)?,
            ));
        }
        let save_result = match storage {
            CredentialStorage::SystemKeyring => {
                store.save_system_password(session_id, password.as_str())
            }
            CredentialStorage::EncryptedVault => {
                let vault_password = vault_password.context("vault password is required")?;
                store.save_vault_password(session_id, password.as_str(), vault_password.as_str())
            }
        };
        if let Err(error) = save_result {
            if let Err(restore_error) = restore_backups(&store, session_id, backups) {
                return Err(error).context(format!(
                    "failed to save remembered password and restore the previous credential: {restore_error}"
                ));
            }
            return Err(error).context("failed to save remembered password");
        }
        if let Some(previous_storage) = previous_storage.filter(|value| *value != storage)
            && let Err(error) = store.delete_password(previous_storage, session_id)
        {
            restore_backups(&store, session_id, backups)?;
            return Err(error).context("failed to remove the replaced remembered password");
        }
        Ok(CredentialRollback {
            session_id,
            backups,
        })
    })
    .await
}

pub(super) struct CredentialRollback {
    session_id: Uuid,
    backups: Vec<(CredentialStorage, CredentialBackup)>,
}

impl CredentialRollback {
    pub(super) async fn restore(self) -> Result<()> {
        run_mutation(move || {
            let store = CredentialStore::platform()?;
            restore_backups(&store, self.session_id, self.backups)
        })
        .await
    }
}

pub(super) async fn delete_password(
    session_id: Uuid,
    previous_storage: CredentialStorage,
) -> Result<CredentialRollback> {
    run_mutation(move || {
        let store = CredentialStore::platform()?;
        let backup = store.backup(previous_storage, session_id)?;
        store.delete_password(previous_storage, session_id)?;
        Ok(CredentialRollback {
            session_id,
            backups: vec![(previous_storage, backup)],
        })
    })
    .await
}

fn restore_backups(
    store: &CredentialStore,
    session_id: Uuid,
    backups: Vec<(CredentialStorage, CredentialBackup)>,
) -> Result<()> {
    for (storage, backup) in backups {
        store.restore_backup(storage, session_id, backup)?;
    }
    Ok(())
}

async fn run_read<T>(operation: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T>
where
    T: Send + 'static,
{
    timeout(OPERATION_TIMEOUT, tokio::task::spawn_blocking(operation))
        .await
        .context("system credential operation timed out")?
        .context("system credential task failed")?
}

/// Run a credential mutation to completion before returning.
///
/// `spawn_blocking` cannot cancel a closure that is already running. A timeout
/// around a mutation would therefore release the caller's persistence gate
/// while the platform keyring/vault operation could still modify storage. The
/// mutation callers hold that gate until this future completes, so waiting for
/// the join handle is required to preserve write ordering.
async fn run_mutation<T>(operation: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T>
where
    T: Send + 'static,
{
    run_mutation_with_deadline(OPERATION_TIMEOUT, operation).await
}

async fn run_mutation_with_deadline<T>(
    deadline: Duration,
    operation: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T>
where
    T: Send + 'static,
{
    let mut task = tokio::task::spawn_blocking(operation);
    match timeout(deadline, &mut task).await {
        Ok(result) => result.context("system credential task failed")?,
        Err(_) => {
            warn!(
                "system credential mutation exceeded its soft deadline; waiting for completion to preserve write ordering"
            );
            task.await.context("system credential task failed")?
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn mutation_soft_deadline_waits_for_blocking_completion() {
        let completed = Arc::new(AtomicBool::new(false));
        let completed_in_task = Arc::clone(&completed);
        let result = run_mutation_with_deadline(Duration::from_millis(1), move || {
            std::thread::sleep(std::time::Duration::from_millis(20));
            completed_in_task.store(true, Ordering::Release);
            Ok(7)
        })
        .await
        .expect("blocking mutation should complete");

        assert_eq!(result, 7);
        assert!(completed.load(Ordering::Acquire));
    }

    #[tokio::test]
    async fn mutation_completion_before_deadline_returns_its_result() {
        let result = run_mutation_with_deadline(Duration::from_secs(1), || Ok(7))
            .await
            .expect("blocking mutation should complete");

        assert_eq!(result, 7);
    }
}

//! Tokio boundary for synchronous platform credential operations.

use anyhow::{Context, Result};
use tokio::time::{Duration, timeout};
use uuid::Uuid;

use ax_ssh::credentials::CredentialStore;

const OPERATION_TIMEOUT: Duration = Duration::from_secs(20);

pub(super) async fn load_password(session_id: Uuid) -> Result<Option<String>> {
    run(move || CredentialStore.load_password(session_id)).await
}

pub(super) async fn save_password(session_id: Uuid, password: String) -> Result<()> {
    run(move || CredentialStore.save_password(session_id, &password)).await
}

pub(super) async fn delete_password(session_id: Uuid) -> Result<()> {
    run(move || CredentialStore.delete_password(session_id)).await
}

async fn run<T>(operation: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T>
where
    T: Send + 'static,
{
    timeout(OPERATION_TIMEOUT, tokio::task::spawn_blocking(operation))
        .await
        .context("system credential operation timed out")?
        .context("system credential task failed")?
}

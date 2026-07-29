//! Platform credential-store access for SSH passwords.
//!
//! Passwords are keyed by the stable session UUID and never enter the JSON
//! configuration. Callers must execute these synchronous OS operations away
//! from the Slint UI thread.

use anyhow::{Context, Result};
use keyring::{Entry, Error};
use uuid::Uuid;

const CREDENTIAL_SERVICE: &str = "com.axsoft.ax_ssh";

#[derive(Clone, Copy, Debug, Default)]
pub struct CredentialStore;

impl CredentialStore {
    pub fn load_password(self, session_id: Uuid) -> Result<Option<String>> {
        let entry = entry(session_id)?;
        match entry.get_password() {
            Ok(password) => Ok(Some(password)),
            Err(Error::NoEntry) => Ok(None),
            Err(error) => {
                Err(error).context("failed to read password from system credential store")
            }
        }
    }

    pub fn save_password(self, session_id: Uuid, password: &str) -> Result<()> {
        if password.is_empty() {
            anyhow::bail!("cannot save an empty password");
        }
        entry(session_id)?
            .set_password(password)
            .context("failed to save password in system credential store")
    }

    pub fn delete_password(self, session_id: Uuid) -> Result<()> {
        match entry(session_id)?.delete_credential() {
            Ok(()) | Err(Error::NoEntry) => Ok(()),
            Err(error) => {
                Err(error).context("failed to delete password from system credential store")
            }
        }
    }
}

fn entry(session_id: Uuid) -> Result<Entry> {
    Entry::new(CREDENTIAL_SERVICE, &credential_account(session_id))
        .context("failed to open system credential entry")
}

fn credential_account(session_id: Uuid) -> String {
    format!("session:{session_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_account_is_stable_and_session_scoped() {
        let id = Uuid::parse_str("018f7a5c-71c2-7e20-b3b0-4f85ef21d441")
            .expect("test UUID should parse");
        assert_eq!(
            credential_account(id),
            "session:018f7a5c-71c2-7e20-b3b0-4f85ef21d441"
        );
    }

    #[test]
    #[ignore = "uses the platform credential store"]
    fn platform_credential_store_round_trips_and_deletes() {
        struct Cleanup {
            store: CredentialStore,
            id: Uuid,
        }

        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = self.store.delete_password(self.id);
            }
        }

        let id = Uuid::new_v4();
        let store = CredentialStore;
        let _cleanup = Cleanup { store, id };
        let password = format!("axssh-test-{id}");

        store
            .save_password(id, &password)
            .expect("test password should be saved");
        assert_eq!(
            store
                .load_password(id)
                .expect("test password should be readable"),
            Some(password)
        );
        store
            .delete_password(id)
            .expect("test password should be deleted");
        assert_eq!(
            store
                .load_password(id)
                .expect("deleted test password lookup should succeed"),
            None
        );
    }
}

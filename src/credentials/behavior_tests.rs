use super::*;
use std::collections::HashMap;
use std::sync::{
    Mutex,
    atomic::{AtomicU8, Ordering},
};

#[derive(Default)]
struct MemoryKeyring {
    values: Mutex<HashMap<String, Zeroizing<String>>>,
    failure: AtomicU8,
}
struct MemoryEntry {
    account: String,
    keyring: Arc<MemoryKeyring>,
}

impl MemoryEntry {
    fn check(&self, operation: u8) -> keyring::Result<()> {
        if self.keyring.failure.load(Ordering::Relaxed) == operation {
            Err(Error::PlatformFailure(Box::new(std::io::Error::other(
                "injected backend failure",
            ))))
        } else {
            Ok(())
        }
    }
}
impl PasswordEntry for MemoryEntry {
    fn get_password(&self) -> Result<Option<String>> {
        self.check(1)?;
        Ok(self
            .keyring
            .values
            .lock()
            .expect("isolated keyring")
            .get(&self.account)
            .map(|value| value.to_string()))
    }
    fn set_password(&self, value: &str) -> Result<()> {
        self.check(2)?;
        self.keyring
            .values
            .lock()
            .expect("isolated keyring")
            .insert(self.account.clone(), Zeroizing::new(value.to_owned()));
        Ok(())
    }
    fn delete_credential(&self) -> Result<()> {
        self.check(3)?;
        self.keyring
            .values
            .lock()
            .expect("isolated keyring")
            .remove(&self.account);
        Ok(())
    }
}

struct Fixture {
    store: CredentialStore,
    keyring: Arc<MemoryKeyring>,
    directory: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!("axssh-credentials-{}", Uuid::new_v4()));
        fs::create_dir(&directory).expect("isolated credential directory");
        let keyring = Arc::new(MemoryKeyring::default());
        let backend = keyring.clone();
        let store = CredentialStore {
            vault_dir: directory.clone(),
            entries: Arc::new(move |account| {
                Ok(Box::new(MemoryEntry {
                    account: account.into(),
                    keyring: backend.clone(),
                }))
            }),
        };
        Self {
            store,
            keyring,
            directory,
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.directory).expect("remove isolated credentials");
    }
}

#[test]
fn system_credentials_are_scoped_idempotent_and_backed_up_without_native_keyring() {
    let fixture = Fixture::new();
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    assert!(
        fixture
            .store
            .load_system_password(first)
            .expect("missing entry")
            .is_none()
    );
    fixture
        .store
        .save_system_password(first, "ephemeral fixture secret")
        .expect("save");
    assert!(
        fixture
            .store
            .load_system_password(second)
            .expect("separate account")
            .is_none()
    );
    let backup = fixture
        .store
        .backup(CredentialStorage::SystemKeyring, first)
        .expect("backup");
    fixture
        .store
        .delete_password(CredentialStorage::SystemKeyring, first)
        .expect("delete");
    fixture
        .store
        .delete_password(CredentialStorage::SystemKeyring, first)
        .expect("idempotent delete");
    fixture
        .store
        .restore_backup(CredentialStorage::SystemKeyring, first, backup)
        .expect("restore");
    assert_eq!(
        fixture
            .store
            .load_system_password(first)
            .expect("restored entry")
            .as_deref(),
        Some("ephemeral fixture secret")
    );
    fixture
        .store
        .restore_backup(
            CredentialStorage::SystemKeyring,
            first,
            CredentialBackup::SystemKeyring(None),
        )
        .expect("restore missing entry");
    assert!(fixture.keyring.values.lock().expect("keyring").is_empty());
    assert!(
        fixture
            .store
            .restore_backup(
                CredentialStorage::EncryptedVault,
                first,
                CredentialBackup::SystemKeyring(None)
            )
            .is_err()
    );
    assert!(fixture.store.save_system_password(first, "").is_err());
    assert!(
        fixture
            .store
            .save_system_password(first, &"x".repeat(MAX_VAULT_PASSWORD_BYTES + 1))
            .is_err()
    );
    assert_eq!(
        fs::read_dir(&fixture.directory)
            .expect("isolated files")
            .count(),
        0,
        "system credentials never become plaintext files"
    );
}

#[test]
fn system_backend_errors_preserve_context_and_legacy_accounts_remain_separate() {
    let fixture = Fixture::new();
    let id = Uuid::new_v4();
    for operation in [1, 2, 3] {
        fixture.keyring.failure.store(operation, Ordering::Relaxed);
        let error = match operation {
            1 => fixture
                .store
                .load_system_password(id)
                .expect_err("read error"),
            2 => fixture
                .store
                .save_system_password(id, "ephemeral fixture secret")
                .expect_err("save error"),
            _ => fixture
                .store
                .delete_system_password(id)
                .expect_err("delete error"),
        };
        assert!(error.to_string().contains("system credential store"));
        assert!(!format!("{error:#}").contains("ephemeral fixture secret"));
    }
    fixture.keyring.failure.store(0, Ordering::Relaxed);
    assert!(
        fixture
            .store
            .load_or_migrate_vault_unlock_password(id)
            .expect("absent legacy key")
            .is_none()
    );
    fixture.keyring.values.lock().expect("keyring").insert(
        format!("{VAULT_UNLOCK_ACCOUNT_PREFIX}{id}"),
        Zeroizing::new("ephemeral legacy fixture".into()),
    );
    assert_eq!(
        fixture
            .store
            .load_vault_unlock_password(id)
            .expect("legacy account")
            .as_deref(),
        Some("ephemeral legacy fixture")
    );
    assert!(
        fixture
            .store
            .load_system_password(id)
            .expect("different namespace")
            .is_none()
    );
    fixture
        .store
        .delete_vault_unlock_password(id)
        .expect("legacy delete");
    fixture
        .store
        .delete_vault_unlock_password(id)
        .expect("legacy idempotent delete");
    assert_eq!(
        fs::read_dir(&fixture.directory)
            .expect("isolated files")
            .count(),
        0
    );
}

#[test]
fn encrypted_backup_restores_bytes_and_rejects_cross_profile_replay() {
    let fixture = Fixture::new();
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    fixture
        .store
        .save_vault_password(first, "ephemeral SSH fixture", "ephemeral vault fixture")
        .expect("encrypted save");
    let encrypted = fs::read(fixture.store.vault_path(first)).expect("encrypted bytes");
    assert!(!String::from_utf8_lossy(&encrypted).contains("ephemeral"));
    let backup = fixture
        .store
        .backup(CredentialStorage::EncryptedVault, first)
        .expect("encrypted backup");
    let replay = fixture
        .store
        .backup(CredentialStorage::EncryptedVault, first)
        .expect("replay fixture");
    fixture
        .store
        .delete_password(CredentialStorage::EncryptedVault, first)
        .expect("delete");
    fixture
        .store
        .restore_backup(CredentialStorage::EncryptedVault, first, backup)
        .expect("restore");
    assert_eq!(
        fs::read(fixture.store.vault_path(first)).expect("restored bytes"),
        encrypted
    );
    fixture
        .store
        .restore_backup(CredentialStorage::EncryptedVault, second, replay)
        .expect("replayed encrypted bytes");
    assert!(
        fixture
            .store
            .load_vault_password(second, "ephemeral vault fixture")
            .is_err(),
        "AAD binds ciphertext to the profile"
    );
    fixture
        .store
        .restore_backup(
            CredentialStorage::EncryptedVault,
            first,
            CredentialBackup::EncryptedVault(None),
        )
        .expect("restore absence");
    assert!(matches!(
        fixture
            .store
            .backup(CredentialStorage::EncryptedVault, first)
            .expect("missing backup"),
        CredentialBackup::EncryptedVault(None)
    ));
}

#[test]
fn credential_files_reject_malformed_oversized_and_unreadable_records() {
    let fixture = Fixture::new();
    let id = Uuid::new_v4();
    let path = fixture.store.vault_path(id);
    for record in [
        b"invalid JSON".to_vec(),
        serde_json::to_vec(&VaultRecord {
            version: 99,
            salt: String::new(),
            nonce: String::new(),
            ciphertext: String::new(),
        })
        .expect("version fixture"),
        serde_json::to_vec(&VaultRecord {
            version: VAULT_RECORD_VERSION,
            salt: "invalid base64!".into(),
            nonce: String::new(),
            ciphertext: String::new(),
        })
        .expect("encoding fixture"),
        vec![0; MAX_VAULT_RECORD_BYTES as usize + 1],
    ] {
        fs::write(&path, record).expect("non-secret malformed fixture");
        assert!(
            fixture
                .store
                .load_vault_password(id, "ephemeral vault fixture")
                .is_err()
        );
    }
    assert!(
        fixture
            .store
            .backup(CredentialStorage::EncryptedVault, id)
            .is_err()
    );
    fs::remove_file(&path).expect("remove invalid file");
    fs::create_dir(&path).expect("directory in place of record");
    assert!(fixture.store.delete_vault_password(id).is_err());
    assert!(
        fixture
            .store
            .load_vault_password(id, "ephemeral vault fixture")
            .is_err()
    );
    let unlock = fixture.store.vault_unlock_path(id);
    for bytes in [
        vec![0xff],
        Vec::new(),
        vec![0; VAULT_UNLOCK_FILE_BYTES as usize + 1],
    ] {
        fs::write(&unlock, bytes).expect("non-secret invalid unlock fixture");
        assert!(fixture.store.backup_vault_unlock_password(id).is_err());
    }
    fixture
        .store
        .restore_vault_unlock_password(id, None)
        .expect("remove invalid unlock record");
    assert!(
        fixture
            .store
            .load_vault_unlock_password(id)
            .expect("absent unlock")
            .is_none()
    );
}

#[test]
fn protected_unlock_keys_are_scoped_and_never_create_plaintext_files() {
    let fixture = Fixture::new();
    let id = Uuid::new_v4();
    fixture
        .store
        .save_vault_unlock_password(id, "ephemeral unlock fixture")
        .expect("save");
    assert_eq!(
        fixture
            .store
            .load_or_migrate_vault_unlock_password(id)
            .expect("load")
            .as_deref(),
        Some("ephemeral unlock fixture")
    );
    assert!(
        fixture
            .store
            .load_vault_unlock_password(Uuid::new_v4())
            .expect("other profile")
            .is_none()
    );
    assert_eq!(fs::read_dir(&fixture.directory).expect("files").count(), 0);
    let backup = fixture
        .store
        .backup_vault_unlock_password(id)
        .expect("backup");
    fixture
        .store
        .delete_vault_unlock_password(id)
        .expect("delete");
    fixture
        .store
        .restore_vault_unlock_password(id, backup)
        .expect("restore");
    assert_eq!(
        fixture
            .store
            .load_vault_unlock_password(id)
            .expect("restored key")
            .as_deref(),
        Some("ephemeral unlock fixture")
    );
    fixture
        .store
        .delete_vault_unlock_password(id)
        .expect("delete");
    fixture
        .store
        .delete_vault_unlock_password(id)
        .expect("idempotent delete");
    assert!(fixture.keyring.values.lock().expect("entries").is_empty());
}

#[test]
fn legacy_unlock_migration_keeps_source_until_protected_save_succeeds() {
    let fixture = Fixture::new();
    let id = Uuid::new_v4();
    let path = fixture.store.vault_unlock_path(id);
    fs::write(&path, b"ephemeral legacy unlock fixture").expect("legacy fixture");
    fixture.keyring.failure.store(2, Ordering::Relaxed);
    let error = fixture
        .store
        .load_or_migrate_vault_unlock_password(id)
        .expect_err("protected backend failure");
    assert!(error.to_string().contains("protected vault unlock key"));
    assert!(!format!("{error:#}").contains("ephemeral legacy unlock fixture"));
    assert_eq!(
        fs::read(&path).expect("recoverable source"),
        b"ephemeral legacy unlock fixture"
    );
    fixture.keyring.failure.store(0, Ordering::Relaxed);
    assert_eq!(
        fixture
            .store
            .load_or_migrate_vault_unlock_password(id)
            .expect("migration")
            .as_deref(),
        Some("ephemeral legacy unlock fixture")
    );
    assert!(!path.exists());
    assert_eq!(
        fixture
            .store
            .load_vault_unlock_password(id)
            .expect("protected key")
            .as_deref(),
        Some("ephemeral legacy unlock fixture")
    );
}

#[cfg(unix)]
#[test]
fn legacy_unlock_migration_rejects_symlinks_without_touching_the_target() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let id = Uuid::new_v4();
    let target = fixture.directory.join("target");
    fs::write(&target, b"ephemeral target fixture").expect("target");
    symlink(&target, fixture.store.vault_unlock_path(id)).expect("symlink");
    assert!(
        fixture
            .store
            .load_or_migrate_vault_unlock_password(id)
            .is_err()
    );
    assert_eq!(
        fs::read(&target).expect("untouched target"),
        b"ephemeral target fixture"
    );
    assert!(fixture.keyring.values.lock().expect("entries").is_empty());
}

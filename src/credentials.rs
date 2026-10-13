//! Credential backends for SSH passwords.
//!
//! Profile JSON stores only a [`CredentialStorage`] policy. Passwords are held
//! either by the platform keyring or in per-profile encrypted vault records.
//! App-generated vault unlock keys are held by the platform keyring. Legacy
//! plaintext unlock files are read only for migration and removed only after
//! their key has been saved successfully. Callers must execute these synchronous
//! operations away from the Slint UI thread and keep key material short-lived.

use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD};
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, Generate, Key, KeyInit},
};
use directories::ProjectDirs;
use keyring::{Entry, Error};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::config::{CredentialStorage, write_private_file_atomically};

const CREDENTIAL_SERVICE: &str = "com.axsoft.ax_ssh";
const VAULT_UNLOCK_ACCOUNT_PREFIX: &str = "vault-unlock:";
const VAULT_RECORD_VERSION: u32 = 1;
const VAULT_SALT_BYTES: usize = 32;
const MAX_VAULT_RECORD_BYTES: u64 = 32 * 1024;
const MAX_VAULT_PASSWORD_BYTES: usize = 1024;
const VAULT_AAD_PREFIX: &[u8] = b"axssh-credential-vault:v1:";
const VAULT_UNLOCK_FILE_BYTES: u64 = MAX_VAULT_PASSWORD_BYTES as u64;

/// A synchronous credential entry adapter for protected stores and isolated tests.
/// Implementations must keep secret values out of plaintext application files.
pub trait PasswordEntry: Send + Sync {
    fn get_password(&self) -> Result<Option<String>>;
    fn set_password(&self, password: &str) -> Result<()>;
    /// Delete an entry, succeeding when it is already absent.
    fn delete_credential(&self) -> Result<()>;
}

impl PasswordEntry for Entry {
    fn get_password(&self) -> Result<Option<String>> {
        match Entry::get_password(self) {
            Ok(password) => Ok(Some(password)),
            Err(Error::NoEntry) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
    fn set_password(&self, password: &str) -> Result<()> {
        Entry::set_password(self, password).map_err(Into::into)
    }
    fn delete_credential(&self) -> Result<()> {
        match Entry::delete_credential(self) {
            Ok(()) | Err(Error::NoEntry) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}

type EntryFactory = dyn Fn(&str) -> Result<Box<dyn PasswordEntry>> + Send + Sync;

#[derive(Clone)]
pub struct CredentialStore {
    vault_dir: PathBuf,
    entries: Arc<EntryFactory>,
}

impl std::fmt::Debug for CredentialStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CredentialStore")
            .field("vault_dir", &self.vault_dir)
            .finish_non_exhaustive()
    }
}

pub enum CredentialBackup {
    SystemKeyring(Option<Zeroizing<String>>),
    EncryptedVault(Option<Vec<u8>>),
}

impl CredentialStore {
    pub fn new(vault_dir: impl Into<PathBuf>) -> Self {
        Self {
            vault_dir: vault_dir.into(),
            entries: Arc::new(|account| Ok(Box::new(Entry::new(CREDENTIAL_SERVICE, account)?))),
        }
    }

    /// Construct a store with an explicit protected-entry provider.
    /// This crate boundary lets application tests inject isolated credentials
    /// without contacting the user's platform store or changing global state.
    pub fn with_entry_factory(
        vault_dir: impl Into<PathBuf>,
        entries: impl Fn(&str) -> Result<Box<dyn PasswordEntry>> + Send + Sync + 'static,
    ) -> Self {
        Self {
            vault_dir: vault_dir.into(),
            entries: Arc::new(entries),
        }
    }

    pub fn platform() -> Result<Self> {
        Ok(Self::new(Self::default_vault_dir()?))
    }

    fn default_vault_dir() -> Result<PathBuf> {
        let dirs = ProjectDirs::from("com", "axsoft", "ax_ssh")
            .context("cannot determine the platform credential directory")?;
        Ok(dirs.config_dir().join("credentials"))
    }

    pub fn load_system_password(&self, session_id: Uuid) -> Result<Option<String>> {
        self.system_entry(session_id)?
            .get_password()
            .context("failed to read password from system credential store")
    }

    pub fn save_system_password(&self, session_id: Uuid, password: &str) -> Result<()> {
        validate_password(password, "password")?;
        self.system_entry(session_id)?
            .set_password(password)
            .context("failed to save password in system credential store")
    }

    pub fn delete_system_password(&self, session_id: Uuid) -> Result<()> {
        self.system_entry(session_id)?
            .delete_credential()
            .context("failed to delete password from system credential store")
    }

    pub fn load_vault_password(
        &self,
        session_id: Uuid,
        vault_password: &str,
    ) -> Result<Option<String>> {
        validate_password(vault_password, "vault password")?;
        let path = self.vault_path(session_id);
        if !path.exists() {
            return Ok(None);
        }
        let metadata = fs::metadata(&path).with_context(|| {
            format!("failed to inspect encrypted credential {}", path.display())
        })?;
        if metadata.len() > MAX_VAULT_RECORD_BYTES {
            anyhow::bail!("encrypted credential record exceeds the size limit");
        }
        let bytes = fs::read(&path)
            .with_context(|| format!("failed to read encrypted credential {}", path.display()))?;
        let record: VaultRecord =
            serde_json::from_slice(&bytes).context("invalid encrypted credential record")?;
        if record.version != VAULT_RECORD_VERSION {
            anyhow::bail!("unsupported encrypted credential record version");
        }
        let salt = decode_fixed(&record.salt, VAULT_SALT_BYTES, "vault salt")?;
        let nonce = decode_fixed(&record.nonce, XNonce::default().len(), "vault nonce")?;
        let ciphertext = STANDARD_NO_PAD
            .decode(record.ciphertext)
            .context("invalid encrypted credential payload")?;
        let key = derive_vault_key(vault_password, &salt)?;
        let cipher = XChaCha20Poly1305::new_from_slice(key.as_ref())
            .map_err(|_| anyhow::anyhow!("invalid derived vault key"))?;
        let nonce = XNonce::try_from(nonce.as_slice())
            .map_err(|_| anyhow::anyhow!("invalid encrypted credential nonce"))?;
        let plaintext = cipher
            .decrypt(
                &nonce,
                chacha20poly1305::aead::Payload {
                    msg: &ciphertext,
                    aad: &vault_aad(session_id),
                },
            )
            .map_err(|_| {
                anyhow::anyhow!("vault password is incorrect or the credential was modified")
            })?;
        let plaintext = Zeroizing::new(plaintext);
        let password = std::str::from_utf8(plaintext.as_slice())
            .context("encrypted credential is not valid UTF-8")?
            .to_owned();
        Ok(Some(password))
    }

    pub fn save_vault_password(
        &self,
        session_id: Uuid,
        password: &str,
        vault_password: &str,
    ) -> Result<()> {
        validate_password(password, "password")?;
        validate_password(vault_password, "vault password")?;
        let salt = Key::<XChaCha20Poly1305>::generate();
        let nonce = XNonce::generate();
        let key = derive_vault_key(vault_password, salt.as_ref())?;
        let cipher = XChaCha20Poly1305::new_from_slice(key.as_ref())
            .map_err(|_| anyhow::anyhow!("invalid derived vault key"))?;
        let ciphertext = cipher
            .encrypt(
                &nonce,
                chacha20poly1305::aead::Payload {
                    msg: password.as_bytes(),
                    aad: &vault_aad(session_id),
                },
            )
            .map_err(|_| anyhow::anyhow!("failed to encrypt credential"))?;
        let record = VaultRecord {
            version: VAULT_RECORD_VERSION,
            salt: STANDARD_NO_PAD.encode(salt),
            nonce: STANDARD_NO_PAD.encode(nonce),
            ciphertext: STANDARD_NO_PAD.encode(ciphertext),
        };
        let bytes = serde_json::to_vec(&record).context("failed to encode encrypted credential")?;
        write_private_file_atomically(&self.vault_path(session_id), &bytes)
    }

    pub fn delete_vault_password(&self, session_id: Uuid) -> Result<()> {
        let path = self.vault_path(session_id);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error).with_context(|| {
                format!("failed to delete encrypted credential {}", path.display())
            }),
        }
    }

    pub fn delete_password(&self, storage: CredentialStorage, session_id: Uuid) -> Result<()> {
        match storage {
            CredentialStorage::SystemKeyring => self.delete_system_password(session_id),
            CredentialStorage::EncryptedVault => self.delete_vault_password(session_id),
        }
    }

    /// Load a profile's unlock key from the protected platform credential store.
    pub fn load_vault_unlock_password(&self, session_id: Uuid) -> Result<Option<String>> {
        let password = self
            .vault_unlock_entry(session_id)?
            .get_password()
            .context("failed to read protected vault unlock key")?;
        if let Some(password) = &password {
            validate_password(password, "vault unlock password")?;
        }
        Ok(password)
    }

    pub fn save_vault_unlock_password(&self, session_id: Uuid, password: &str) -> Result<()> {
        validate_password(password, "vault unlock password")?;
        self.vault_unlock_entry(session_id)?
            .set_password(password)
            .context("failed to save protected vault unlock key")
    }

    pub fn delete_vault_unlock_password(&self, session_id: Uuid) -> Result<()> {
        self.vault_unlock_entry(session_id)?
            .delete_credential()
            .context("failed to delete protected vault unlock key")?;
        self.delete_local_unlock_file(session_id)
    }

    pub fn backup_vault_unlock_password(
        &self,
        session_id: Uuid,
    ) -> Result<Option<Zeroizing<String>>> {
        self.load_or_migrate_vault_unlock_password(session_id)
            .map(|password| password.map(Zeroizing::new))
    }

    pub fn restore_vault_unlock_password(
        &self,
        session_id: Uuid,
        password: Option<Zeroizing<String>>,
    ) -> Result<()> {
        match password {
            Some(password) => self.save_vault_unlock_password(session_id, password.as_str()),
            None => self.delete_vault_unlock_password(session_id),
        }
    }

    /// Migrate a legacy plaintext file into the protected credential store.
    /// A backend or cleanup failure is reported without discarding the source.
    /// Older keyring records already use the protected account namespace.
    pub fn load_or_migrate_vault_unlock_password(
        &self,
        session_id: Uuid,
    ) -> Result<Option<String>> {
        if let Some(password) = self.read_local_unlock_file(session_id)? {
            self.save_vault_unlock_password(session_id, &password)?;
            self.delete_local_unlock_file(session_id)?;
            return Ok(Some(password.to_string()));
        }
        self.load_vault_unlock_password(session_id)
    }

    fn read_local_unlock_file(&self, session_id: Uuid) -> Result<Option<Zeroizing<String>>> {
        let path = self.vault_unlock_path(session_id);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error).context("failed to inspect legacy vault unlock file"),
        };
        if !metadata.is_file() || metadata.len() > VAULT_UNLOCK_FILE_BYTES {
            anyhow::bail!("legacy vault unlock file is not a bounded regular file");
        }
        let mut bytes = Zeroizing::new(Vec::new());
        let mut options = fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;
            options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
        }
        let file = options
            .open(&path)
            .context("failed to open legacy vault unlock file")?;
        if !file
            .metadata()
            .context("failed to inspect opened legacy unlock file")?
            .is_file()
        {
            anyhow::bail!("legacy vault unlock handle is not a regular file");
        }
        file.take(VAULT_UNLOCK_FILE_BYTES + 1)
            .read_to_end(&mut bytes)
            .context("failed to read legacy vault unlock file")?;
        if bytes.len() > MAX_VAULT_PASSWORD_BYTES {
            anyhow::bail!("legacy vault unlock file exceeds the size limit");
        }
        let password = Zeroizing::new(
            std::str::from_utf8(&bytes)
                .context("legacy vault unlock file is not valid UTF-8")?
                .to_owned(),
        );
        validate_password(&password, "vault unlock password")?;
        Ok(Some(password))
    }

    fn delete_local_unlock_file(&self, session_id: Uuid) -> Result<()> {
        match fs::remove_file(self.vault_unlock_path(session_id)) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error).context("failed to delete legacy vault unlock file"),
        }
    }

    pub fn backup(&self, storage: CredentialStorage, session_id: Uuid) -> Result<CredentialBackup> {
        match storage {
            CredentialStorage::SystemKeyring => Ok(CredentialBackup::SystemKeyring(
                self.load_system_password(session_id)?.map(Zeroizing::new),
            )),
            CredentialStorage::EncryptedVault => Ok(CredentialBackup::EncryptedVault(
                self.read_vault_record(session_id)?,
            )),
        }
    }

    pub fn restore_backup(
        &self,
        storage: CredentialStorage,
        session_id: Uuid,
        backup: CredentialBackup,
    ) -> Result<()> {
        match (storage, backup) {
            (CredentialStorage::SystemKeyring, CredentialBackup::SystemKeyring(Some(password))) => {
                self.save_system_password(session_id, password.as_str())
            }
            (CredentialStorage::SystemKeyring, CredentialBackup::SystemKeyring(None)) => {
                self.delete_system_password(session_id)
            }
            (CredentialStorage::EncryptedVault, CredentialBackup::EncryptedVault(Some(record))) => {
                write_private_file_atomically(&self.vault_path(session_id), &record)
            }
            (CredentialStorage::EncryptedVault, CredentialBackup::EncryptedVault(None)) => {
                self.delete_vault_password(session_id)
            }
            _ => anyhow::bail!("credential backup does not match its storage policy"),
        }
    }

    fn vault_path(&self, session_id: Uuid) -> PathBuf {
        self.vault_dir.join(format!("{session_id}.json"))
    }

    fn vault_unlock_path(&self, session_id: Uuid) -> PathBuf {
        self.vault_dir.join(format!("{session_id}.unlock"))
    }

    fn read_vault_record(&self, session_id: Uuid) -> Result<Option<Vec<u8>>> {
        let path = self.vault_path(session_id);
        if !path.exists() {
            return Ok(None);
        }
        let metadata = fs::metadata(&path).with_context(|| {
            format!("failed to inspect encrypted credential {}", path.display())
        })?;
        if metadata.len() > MAX_VAULT_RECORD_BYTES {
            anyhow::bail!("encrypted credential record exceeds the size limit");
        }
        fs::read(&path)
            .with_context(|| format!("failed to read encrypted credential {}", path.display()))
            .map(Some)
    }

    fn system_entry(&self, session_id: Uuid) -> Result<Box<dyn PasswordEntry>> {
        (self.entries)(&format!("session:{session_id}"))
            .context("failed to open system credential entry")
    }

    fn vault_unlock_entry(&self, session_id: Uuid) -> Result<Box<dyn PasswordEntry>> {
        (self.entries)(&format!("{VAULT_UNLOCK_ACCOUNT_PREFIX}{session_id}"))
            .context("failed to open encrypted vault unlock entry")
    }
}

#[derive(Deserialize, Serialize)]
struct VaultRecord {
    version: u32,
    salt: String,
    nonce: String,
    ciphertext: String,
}

fn validate_password(value: &str, label: &str) -> Result<()> {
    if value.is_empty() {
        anyhow::bail!("{label} cannot be empty");
    }
    if value.len() > MAX_VAULT_PASSWORD_BYTES {
        anyhow::bail!("{label} exceeds the size limit");
    }
    Ok(())
}

fn derive_vault_key(password: &str, salt: &[u8]) -> Result<Zeroizing<[u8; 32]>> {
    let params = Params::new(64 * 1024, 3, 1, Some(32))
        .map_err(|_| anyhow::anyhow!("invalid vault KDF parameters"))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = Zeroizing::new([0_u8; 32]);
    argon2
        .hash_password_into(password.as_bytes(), salt, key.as_mut())
        .map_err(|_| anyhow::anyhow!("failed to derive vault encryption key"))?;
    Ok(key)
}

fn decode_fixed(value: &str, expected_len: usize, label: &str) -> Result<Vec<u8>> {
    let bytes = STANDARD_NO_PAD
        .decode(value)
        .with_context(|| format!("invalid {label}"))?;
    if bytes.len() != expected_len {
        anyhow::bail!("invalid {label} length");
    }
    Ok(bytes)
}

fn vault_aad(session_id: Uuid) -> Vec<u8> {
    let mut aad = Vec::with_capacity(VAULT_AAD_PREFIX.len() + 36);
    aad.extend_from_slice(VAULT_AAD_PREFIX);
    aad.extend_from_slice(session_id.as_bytes());
    aad
}

#[cfg(test)]
mod behavior_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypted_vault_round_trips_and_rejects_wrong_password() {
        let root = std::env::temp_dir().join(format!("ax-ssh-vault-{}", Uuid::new_v4()));
        let store = CredentialStore::new(root.clone());
        let id = Uuid::new_v4();

        store
            .save_vault_password(id, "ssh-password", "vault-password")
            .expect("vault password should save");
        let raw_record = String::from_utf8(
            fs::read(store.vault_path(id)).expect("vault record should be readable"),
        )
        .expect("vault record should be UTF-8 JSON");
        assert!(!raw_record.contains("ssh-password"));
        assert!(!raw_record.contains("vault-password"));
        assert_eq!(
            store
                .load_vault_password(id, "vault-password")
                .expect("vault password should load"),
            Some("ssh-password".to_owned())
        );
        assert!(store.load_vault_password(id, "wrong-password").is_err());
        let path = store.vault_path(id);
        let mut record: VaultRecord =
            serde_json::from_slice(&fs::read(&path).expect("vault record should exist"))
                .expect("vault record should parse");
        let replacement = if record.ciphertext.starts_with('A') {
            "B"
        } else {
            "A"
        };
        record.ciphertext.replace_range(0..1, replacement);
        write_private_file_atomically(
            &path,
            &serde_json::to_vec(&record).expect("vault record should encode"),
        )
        .expect("tampered vault record should save");
        assert!(store.load_vault_password(id, "vault-password").is_err());
        store
            .delete_vault_password(id)
            .expect("vault password should delete");
        assert_eq!(
            store
                .load_vault_password(id, "vault-password")
                .expect("missing credential should be valid"),
            None
        );
        let _ = fs::remove_dir_all(root);
    }
}

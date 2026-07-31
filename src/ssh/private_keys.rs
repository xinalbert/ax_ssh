//! Local OpenSSH private-key discovery and loading.

use std::fs::{self, File};
use std::io::Read as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use directories::UserDirs;
use russh::keys::PrivateKey;
use zeroize::Zeroizing;

const HEADER_SCAN_BYTES: u64 = 512;

pub fn discover_private_keys() -> Result<Vec<PathBuf>> {
    let Some(user_dirs) = UserDirs::new() else {
        return Ok(Vec::new());
    };
    discover_private_keys_in(&user_dirs.home_dir().join(".ssh"))
}

fn discover_private_keys_in(directory: &Path) -> Result<Vec<PathBuf>> {
    if !directory.exists() {
        return Ok(Vec::new());
    }

    let mut paths = fs::read_dir(directory)
        .with_context(|| format!("failed to read {}", directory.display()))?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && looks_like_private_key(path))
        .collect::<Vec<_>>();
    paths.sort();
    Ok(paths)
}

fn looks_like_private_key(path: &Path) -> bool {
    if path.extension().is_some_and(|extension| extension == "pub") {
        return false;
    }
    let Ok(file) = File::open(path) else {
        return false;
    };
    let mut header = Vec::new();
    if file
        .take(HEADER_SCAN_BYTES)
        .read_to_end(&mut header)
        .is_err()
    {
        return false;
    }
    String::from_utf8_lossy(&header).contains("PRIVATE KEY-----")
}

pub(super) async fn load_private_key(
    path: PathBuf,
    passphrase: Zeroizing<String>,
) -> Result<PrivateKey> {
    tokio::task::spawn_blocking(move || {
        let passphrase = (!passphrase.is_empty()).then_some(passphrase);
        russh::keys::load_secret_key(&path, passphrase.as_deref().map(String::as_str))
            .with_context(|| format!("cannot read {}", path.display()))
    })
    .await
    .context("private-key loading task failed")?
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng as _;
    use rand::rngs::StdRng;
    use uuid::Uuid;

    #[test]
    fn discovery_only_returns_private_key_files() {
        let directory =
            std::env::temp_dir().join(format!("ax-ssh-key-discovery-{}", Uuid::new_v4()));
        fs::create_dir_all(&directory).expect("test directory should be created");
        fs::write(
            directory.join("id_ed25519"),
            "-----BEGIN OPENSSH PRIVATE KEY-----\nfixture\n",
        )
        .expect("private-key fixture should be written");
        fs::write(directory.join("id_ed25519.pub"), "ssh-ed25519 fixture")
            .expect("public-key fixture should be written");
        fs::write(directory.join("config"), "Host example")
            .expect("config fixture should be written");

        let paths = discover_private_keys_in(&directory).expect("discovery should succeed");
        assert_eq!(paths, vec![directory.join("id_ed25519")]);

        fs::remove_dir_all(directory).expect("test directory should be removed");
    }

    #[tokio::test]
    async fn encrypted_keys_require_the_matching_passphrase() {
        let mut rng = StdRng::seed_from_u64(126);
        let key = PrivateKey::random(&mut rng, russh::keys::Algorithm::Ed25519)
            .expect("test private key should be generated");
        let encrypted = key
            .encrypt(&mut rng, "correct-passphrase")
            .expect("test private key should be encrypted");
        let path = std::env::temp_dir().join(format!("ax-ssh-encrypted-key-{}", Uuid::new_v4()));
        fs::write(
            &path,
            encrypted
                .to_openssh(russh::keys::ssh_key::LineEnding::LF)
                .expect("encrypted key should encode")
                .as_bytes(),
        )
        .expect("encrypted key fixture should be written");

        assert!(
            load_private_key(path.clone(), Zeroizing::new(String::new()))
                .await
                .is_err()
        );
        assert!(
            load_private_key(path.clone(), Zeroizing::new("wrong-passphrase".into()),)
                .await
                .is_err()
        );
        let loaded = load_private_key(path.clone(), Zeroizing::new("correct-passphrase".into()))
            .await
            .expect("matching passphrase should load the key");
        assert_eq!(loaded.public_key(), key.public_key());

        fs::remove_file(path).expect("encrypted key fixture should be removed");
    }
}

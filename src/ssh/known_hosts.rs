//! Bounded OpenSSH `known_hosts` parsing and trust decisions.
//!
//! The system file is a trust *source*, never a reason to accept a key that
//! is revoked or conflicts with the profile's explicit fingerprint.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use base64::Engine as _;
use hmac::{Hmac, KeyInit, Mac};
use russh::keys::{PublicKey, parse_public_key_base64};
use sha1::Sha1;

const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_LINES: usize = 16_384;

type HmacSha1 = Hmac<Sha1>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrustDecision {
    Unknown,
    Trusted,
    Changed,
    Revoked,
}

#[derive(Clone, Debug)]
struct Entry {
    hosts: String,
    key: PublicKey,
    revoked: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Snapshot {
    entries: Vec<Entry>,
}

impl Snapshot {
    pub(crate) fn load_default() -> Result<Self> {
        Self::load_from_path(default_path()?)
    }

    pub(crate) fn load_from_path(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let metadata = match fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => {
                return Err(error).with_context(|| format!("cannot stat {}", path.display()));
            }
        };
        if metadata.len() > MAX_FILE_BYTES {
            anyhow::bail!("known_hosts file exceeds {MAX_FILE_BYTES} bytes");
        }
        let text = fs::read_to_string(path)
            .with_context(|| format!("cannot read known_hosts file {}", path.display()))?;
        let mut entries = Vec::new();
        for (index, line) in text.lines().enumerate() {
            if index >= MAX_LINES {
                anyhow::bail!("known_hosts file exceeds {MAX_LINES} lines");
            }
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let fields: Vec<&str> = line.split_whitespace().collect();
            let (marker, offset) = match fields.first().copied() {
                Some(value) if value.starts_with('@') => (Some(value), 1),
                _ => (None, 0),
            };
            if marker.is_some_and(|marker| marker != "@revoked") {
                continue;
            }
            if fields.len() < offset + 3 {
                continue;
            }
            let key = match parse_public_key_base64(fields[offset + 2]) {
                Ok(key) => key,
                Err(_) => continue,
            };
            entries.push(Entry {
                hosts: fields[offset].to_owned(),
                key,
                revoked: marker == Some("@revoked"),
            });
        }
        Ok(Self { entries })
    }

    pub(crate) fn decision(&self, host: &str, port: u16, key: &PublicKey) -> TrustDecision {
        let candidates = host_candidates(host, port);
        let mut host_seen = false;
        for entry in &self.entries {
            if !host_patterns_match(&entry.hosts, &candidates) {
                continue;
            }
            host_seen = true;
            if entry.key.algorithm() == key.algorithm() && entry.key == *key && entry.revoked {
                return TrustDecision::Revoked;
            }
        }
        if self.entries.iter().any(|entry| {
            !entry.revoked
                && host_patterns_match(&entry.hosts, &candidates)
                && entry.key.algorithm() == key.algorithm()
                && entry.key == *key
        }) {
            return TrustDecision::Trusted;
        }
        if host_seen {
            TrustDecision::Changed
        } else {
            TrustDecision::Unknown
        }
    }

    /// Append an explicitly confirmed key while preserving all existing lines.
    /// The caller must have already rejected `@revoked` and performed UI
    /// confirmation. This operation never replaces or removes an old key.
    pub(crate) fn append_confirmed(
        path: impl AsRef<Path>,
        host: &str,
        port: u16,
        key: &PublicKey,
    ) -> Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }
        let existing = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("cannot read known_hosts file {}", path.display()));
            }
        };
        let separator = if existing.is_empty() || existing.ends_with('\n') {
            ""
        } else {
            "\n"
        };
        let line = format!(
            "{separator}{} {}\n",
            format_host(host, port),
            key.to_openssh()?
        );
        atomic_replace(path, format!("{existing}{line}").as_bytes())
    }

    pub(crate) fn append_confirmed_openssh(
        path: impl AsRef<Path>,
        host: &str,
        port: u16,
        public_key: &str,
    ) -> Result<()> {
        let key = PublicKey::from_openssh(public_key).context("invalid observed host key")?;
        Self::append_confirmed(path, host, port, &key)
    }

    pub(crate) fn replace_confirmed_openssh(
        path: impl AsRef<Path>,
        host: &str,
        port: u16,
        public_key: &str,
    ) -> Result<()> {
        let path = path.as_ref();
        let key = PublicKey::from_openssh(public_key).context("invalid observed host key")?;
        let text = fs::read_to_string(path)
            .with_context(|| format!("cannot read known_hosts file {}", path.display()))?;
        let candidates = host_candidates(host, port);
        let kept = text
            .lines()
            .filter(|line| {
                let fields: Vec<&str> = line.split_whitespace().collect();
                let Some(first) = fields.first() else {
                    return true;
                };
                let marker = first.starts_with('@');
                let offset = usize::from(marker);
                marker
                    || fields.len() < offset + 3
                    || !host_patterns_match(fields[offset], &candidates)
            })
            .collect::<Vec<_>>();
        let prefix = if kept.is_empty() {
            String::new()
        } else {
            format!("{}\n", kept.join("\n"))
        };
        let line = format!("{} {}\n", format_host(host, port), key.to_openssh()?);
        atomic_replace(path, format!("{prefix}{line}").as_bytes())
    }

    pub(crate) fn remove_key(
        path: impl AsRef<Path>,
        host: &str,
        port: u16,
        fingerprint: &str,
    ) -> Result<bool> {
        let path = path.as_ref();
        let text = fs::read_to_string(path)
            .with_context(|| format!("cannot read known_hosts file {}", path.display()))?;
        let mut changed = false;
        let mut kept = Vec::new();
        for line in text.lines() {
            let fields: Vec<&str> = line.split_whitespace().collect();
            let offset = usize::from(fields.first().is_some_and(|field| field.starts_with('@')));
            let remove = fields.len() >= offset + 3
                && host_patterns_match(fields[offset], &host_candidates(host, port))
                && parse_public_key_base64(fields[offset + 2])
                    .ok()
                    .is_some_and(|key| {
                        key.fingerprint(russh::keys::HashAlg::Sha256).to_string() == fingerprint
                    });
            if remove {
                changed = true;
            } else {
                kept.push(line);
            }
        }
        if changed {
            atomic_replace(path, format!("{}\n", kept.join("\n")).as_bytes())?;
        }
        Ok(changed)
    }
}

pub(crate) fn default_path() -> Result<PathBuf> {
    #[cfg(windows)]
    let home =
        std::env::var_os("USERPROFILE").ok_or_else(|| anyhow!("USERPROFILE is unavailable"))?;
    #[cfg(not(windows))]
    let home = std::env::var_os("HOME").ok_or_else(|| anyhow!("HOME is unavailable"))?;
    Ok(PathBuf::from(home).join(".ssh").join("known_hosts"))
}

fn atomic_replace(path: &Path, bytes: &[u8]) -> Result<()> {
    let temp = path.with_extension(format!("tmp-{}", uuid::Uuid::new_v4()));
    {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temp, fs::Permissions::from_mode(0o600))?;
    }
    fs::rename(&temp, path).with_context(|| format!("cannot replace {}", path.display()))?;
    Ok(())
}

fn format_host(host: &str, port: u16) -> String {
    if port == 22 {
        host.to_owned()
    } else {
        format!("[{host}]:{port}")
    }
}

fn host_candidates(host: &str, port: u16) -> [String; 2] {
    if port == 22 {
        [host.to_owned(), format_host(host, port)]
    } else {
        [format_host(host, port), String::new()]
    }
}

fn host_patterns_match(patterns: &str, candidates: &[String; 2]) -> bool {
    let mut matched = false;
    for pattern in patterns.split(',') {
        if pattern.is_empty() {
            continue;
        }
        let negated = pattern.starts_with('!');
        let pattern = pattern.strip_prefix('!').unwrap_or(pattern);
        let this_match = candidates
            .iter()
            .filter(|candidate| !candidate.is_empty())
            .any(|candidate| {
                if let Some(rest) = pattern.strip_prefix("|1|") {
                    let Some((salt, digest)) = rest.split_once('|') else {
                        return false;
                    };
                    let (Ok(salt), Ok(expected)) = (
                        base64::engine::general_purpose::STANDARD.decode(salt),
                        base64::engine::general_purpose::STANDARD.decode(digest),
                    ) else {
                        return false;
                    };
                    let Ok(mut mac) = HmacSha1::new_from_slice(&salt) else {
                        return false;
                    };
                    mac.update(candidate.as_bytes());
                    mac.verify_slice(&expected).is_ok()
                } else {
                    glob_match(pattern, candidate)
                }
            });
        if negated && this_match {
            return false;
        }
        matched |= this_match;
    }
    matched
}

fn glob_match(pattern: &str, value: &str) -> bool {
    let (mut p, mut v, mut star, mut backtrack) = (0, 0, None, 0);
    let pattern = pattern.as_bytes();
    let value = value.as_bytes();
    while v < value.len() {
        if p < pattern.len() && (pattern[p] == value[v] || pattern[p] == b'?') {
            p += 1;
            v += 1;
        } else if p < pattern.len() && pattern[p] == b'*' {
            star = Some(p);
            p += 1;
            backtrack = v;
        } else if let Some(star_pos) = star {
            p = star_pos + 1;
            backtrack += 1;
            v = backtrack;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == b'*' {
        p += 1;
    }
    p == pattern.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    fn key(seed: u64) -> PublicKey {
        russh::keys::PrivateKey::random(
            &mut rand::rngs::StdRng::seed_from_u64(seed),
            russh::keys::Algorithm::Ed25519,
        )
        .unwrap()
        .public_key()
        .clone()
    }

    #[test]
    fn classifies_trusted_changed_and_revoked() {
        let first = key(1);
        let second = key(2);
        let path = std::env::temp_dir().join(format!("axssh-known-hosts-{}", uuid::Uuid::new_v4()));
        fs::write(
            &path,
            format!(
                "example.test {}\n@revoked revoked.test {}\n",
                first.to_openssh().unwrap(),
                second.to_openssh().unwrap()
            ),
        )
        .unwrap();
        let snapshot = Snapshot::load_from_path(&path).unwrap();
        assert_eq!(
            snapshot.decision("example.test", 22, &first),
            TrustDecision::Trusted
        );
        assert_eq!(
            snapshot.decision("example.test", 22, &second),
            TrustDecision::Changed
        );
        assert_eq!(
            snapshot.decision("revoked.test", 22, &second),
            TrustDecision::Revoked
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn matches_hashed_hosts_and_nonstandard_ports() {
        let key = key(3);
        let salt = b"test-salt";
        let mut mac = HmacSha1::new_from_slice(salt).unwrap();
        mac.update(b"example.test");
        let host = format!(
            "|1|{}|{}",
            base64::engine::general_purpose::STANDARD.encode(salt),
            base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes())
        );
        let path = std::env::temp_dir().join(format!("axssh-known-hosts-{}", uuid::Uuid::new_v4()));
        fs::write(
            &path,
            format!(
                "{host} {}\n[example.test]:2200 {}\n",
                key.to_openssh().unwrap(),
                key.to_openssh().unwrap()
            ),
        )
        .unwrap();
        let snapshot = Snapshot::load_from_path(&path).unwrap();
        assert_eq!(
            snapshot.decision("example.test", 22, &key),
            TrustDecision::Trusted
        );
        assert_eq!(
            snapshot.decision("example.test", 2200, &key),
            TrustDecision::Trusted
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn malformed_lines_do_not_broaden_trust() {
        let path = std::env::temp_dir().join(format!("axssh-known-hosts-{}", uuid::Uuid::new_v4()));
        fs::write(&path, "example.test not-a-key\n").unwrap();
        let snapshot = Snapshot::load_from_path(&path).unwrap();
        assert_eq!(snapshot.entries.len(), 0);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn non_default_port_does_not_use_plain_host_entry_and_cert_authority_is_not_trusted() {
        let key = key(4);
        let path = std::env::temp_dir().join(format!("axssh-known-hosts-{}", uuid::Uuid::new_v4()));
        fs::write(
            &path,
            format!(
                "example.test {}\n@cert-authority cert.test {}\n",
                key.to_openssh().unwrap(),
                key.to_openssh().unwrap()
            ),
        )
        .unwrap();
        let snapshot = Snapshot::load_from_path(&path).unwrap();
        assert_eq!(
            snapshot.decision("example.test", 2200, &key),
            TrustDecision::Unknown
        );
        assert_eq!(
            snapshot.decision("cert.test", 22, &key),
            TrustDecision::Unknown
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn replacement_and_removal_preserve_unrelated_and_revoked_records() {
        let old = key(5);
        let new = key(6);
        let revoked = key(7);
        let path = std::env::temp_dir().join(format!("axssh-known-hosts-{}", uuid::Uuid::new_v4()));
        fs::write(
            &path,
            format!(
                "# keep this comment\nexample.test {}\nother.test {}\n@revoked example.test {}\n",
                old.to_openssh().unwrap(),
                old.to_openssh().unwrap(),
                revoked.to_openssh().unwrap()
            ),
        )
        .unwrap();
        Snapshot::replace_confirmed_openssh(&path, "example.test", 22, &new.to_openssh().unwrap())
            .unwrap();
        let replaced = fs::read_to_string(&path).unwrap();
        assert!(replaced.contains("# keep this comment"));
        assert!(replaced.contains("other.test"));
        assert!(replaced.contains("@revoked example.test"));
        assert!(
            !replaced
                .lines()
                .any(|line| line.starts_with("example.test ")
                    && line.contains(&old.to_openssh().unwrap()))
        );
        assert!(replaced.contains(&new.to_openssh().unwrap()));
        assert!(
            Snapshot::remove_key(
                &path,
                "example.test",
                22,
                &new.fingerprint(russh::keys::HashAlg::Sha256).to_string(),
            )
            .unwrap()
        );
        let removed = fs::read_to_string(&path).unwrap();
        assert!(!removed.contains(&new.to_openssh().unwrap()));
        assert!(removed.contains("other.test"));
        assert!(removed.contains("@revoked example.test"));
        let _ = fs::remove_file(path);
    }
}

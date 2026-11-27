//! Read-only conversion of a subset of OpenSSH client configuration.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use regex::{Regex, RegexBuilder};

use super::{AuthMethod, SessionProfile, X11ForwardingMode};

const MAX_CONFIG_BYTES: usize = 256 * 1024;
const MAX_RULES: usize = 2048;
const MAX_HOSTS: usize = 128;

pub struct OpenSshImport {
    pub profiles: Vec<SessionProfile>,
    pub notes: Vec<String>,
}

struct Section {
    patterns: Vec<(bool, Regex)>,
    options: Vec<(String, Vec<String>)>,
}

impl Section {
    fn matches(&self, alias: &str) -> bool {
        let mut matched = self.patterns.is_empty();
        for (negative, pattern) in &self.patterns {
            if pattern.is_match(alias) {
                if *negative {
                    return false;
                }
                matched = true;
            }
        }
        matched
    }
}

pub fn read_openssh_import(path: &Path) -> Result<OpenSshImport> {
    let home = directories::BaseDirs::new().context("cannot locate the home directory")?;
    let path = expand_home(path, home.home_dir());
    anyhow::ensure!(
        std::fs::metadata(&path)
            .context("cannot inspect the OpenSSH configuration")?
            .is_file(),
        "select a regular configuration file"
    );
    let file = File::open(&path).context("cannot open the OpenSSH configuration")?;
    anyhow::ensure!(
        file.metadata()?.is_file(),
        "select a regular configuration file"
    );
    let mut bytes = Vec::new();
    file.take((MAX_CONFIG_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    anyhow::ensure!(
        bytes.len() <= MAX_CONFIG_BYTES,
        "OpenSSH configuration exceeds 256 KiB"
    );
    let text = std::str::from_utf8(&bytes).context("OpenSSH configuration must be UTF-8")?;
    let local_user = std::env::var(if cfg!(windows) { "USERNAME" } else { "USER" }).ok();
    parse_config(
        text.trim_start_matches('\u{feff}'),
        home.home_dir(),
        local_user.as_deref(),
    )
}

fn parse_config(text: &str, home: &Path, local_user: Option<&str>) -> Result<OpenSshImport> {
    let mut sections = vec![Section {
        patterns: vec![],
        options: vec![],
    }];
    let mut aliases = Vec::new();
    let mut seen = BTreeSet::new();
    let mut rules = 0;
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        rules += 1;
        anyhow::ensure!(
            rules <= MAX_RULES,
            "too many OpenSSH configuration directives"
        );
        let split = line
            .find(|c: char| c.is_whitespace() || c == '=')
            .unwrap_or(line.len());
        let keyword = line[..split].to_ascii_lowercase();
        let remainder = line[split..].trim_start();
        let arguments = words(remainder.strip_prefix('=').unwrap_or(remainder))
            .with_context(|| format!("invalid configuration syntax at line {}", index + 1))?;
        match keyword.as_str() {
            "include" | "match" => {
                anyhow::bail!(
                    "line {} uses Include or Match, which this importer cannot evaluate",
                    index + 1
                );
            }
            "host" => {
                anyhow::ensure!(
                    !arguments.is_empty(),
                    "missing Host patterns at line {}",
                    index + 1
                );
                let mut patterns = Vec::new();
                for pattern in &arguments {
                    anyhow::ensure!(
                        patterns.len() < MAX_HOSTS && pattern.len() <= 512,
                        "too many or oversized Host patterns"
                    );
                    let negative = pattern.starts_with('!');
                    let pattern = pattern.strip_prefix('!').unwrap_or(pattern);
                    let regex = regex::escape(pattern)
                        .replace(r"\*", ".*")
                        .replace(r"\?", ".");
                    patterns.push((
                        negative,
                        RegexBuilder::new(&format!("^{regex}$"))
                            .case_insensitive(true)
                            .build()?,
                    ));
                    if !negative
                        && !pattern.contains(['*', '?'])
                        && seen.insert(pattern.to_ascii_lowercase())
                    {
                        anyhow::ensure!(aliases.len() < MAX_HOSTS, "too many explicit SSH hosts");
                        aliases.push(pattern.to_owned());
                    }
                }
                sections.push(Section {
                    patterns,
                    options: vec![],
                });
            }
            _ => {
                let section = sections
                    .last_mut()
                    .context("missing global configuration section")?;
                section.options.push((keyword, arguments));
            }
        }
    }

    let mut result = OpenSshImport {
        profiles: Vec::new(),
        notes: Vec::new(),
    };
    let mut ignored = BTreeSet::new();
    for alias in aliases {
        match resolve_host(&alias, &sections, home, local_user, &mut ignored) {
            Ok(profile) => result.profiles.push(profile),
            Err(error) => result.notes.push(format!("{alias}: {error}")),
        }
    }
    if !ignored.is_empty() {
        result.notes.push(format!(
            "Options skipped: {}",
            ignored.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }
    if result.profiles.is_empty() {
        result
            .notes
            .push("No importable explicit Host entries".to_owned());
    }
    Ok(result)
}

fn resolve_host(
    alias: &str,
    sections: &[Section],
    home: &Path,
    local_user: Option<&str>,
    ignored: &mut BTreeSet<String>,
) -> Result<SessionProfile> {
    let mut scalar = BTreeMap::new();
    let mut identities = Vec::new();
    for (key, values) in sections
        .iter()
        .filter(|s| s.matches(alias))
        .flat_map(|s| &s.options)
    {
        match key.as_str() {
            "hostname"
            | "port"
            | "user"
            | "forwardx11"
            | "forwardx11trusted"
            | "identitiesonly"
            | "pubkeyauthentication"
            | "proxyjump"
            | "proxycommand"
            | "hostkeyalias"
            | "userknownhostsfile"
            | "globalknownhostsfile"
            | "knownhostscommand"
            | "revokedhostkeys"
            | "hostkeyalgorithms"
            | "casignaturealgorithms"
            | "canonicalizehostname"
            | "certificatefile"
            | "identityagent"
            | "pkcs11provider"
            | "securitykeyprovider" => {
                if !scalar.contains_key(key.as_str()) {
                    anyhow::ensure!(values.len() == 1, "unsupported value for {key}");
                    scalar.insert(key.as_str(), values[0].as_str());
                }
            }
            "identityfile" => {
                anyhow::ensure!(values.len() == 1, "unsupported IdentityFile value");
                identities.push(values[0].as_str());
            }
            _ => {
                // Retain directive names only and keep the preview bounded.
                if ignored.len() < 32
                    && key.len() <= 64
                    && key.chars().all(|c| c.is_ascii_alphanumeric())
                {
                    ignored.insert(key.clone());
                }
            }
        }
    }

    for (key, inactive) in [
        ("proxyjump", Some("none")),
        ("proxycommand", Some("none")),
        ("canonicalizehostname", Some("no")),
        ("hostkeyalias", None),
        ("userknownhostsfile", None),
        ("globalknownhostsfile", None),
        ("knownhostscommand", Some("none")),
        ("revokedhostkeys", Some("none")),
        ("hostkeyalgorithms", None),
        ("casignaturealgorithms", None),
        ("certificatefile", Some("none")),
        ("identityagent", None),
        ("pkcs11provider", Some("none")),
        ("securitykeyprovider", None),
    ] {
        let present = scalar.get(key);
        anyhow::ensure!(
            present.is_none_or(
                |value| inactive.is_some_and(|inactive| value.eq_ignore_ascii_case(inactive))
            ),
            "skipped because {key} requires OpenSSH semantics"
        );
    }

    let host = scalar.get("hostname").copied().unwrap_or(alias);
    let host = if host == "%h" { alias } else { host };
    let user = scalar
        .get("user")
        .copied()
        .or(local_user)
        .context("missing User")?;
    anyhow::ensure!(
        !host.contains('%')
            && !host.chars().any(char::is_whitespace)
            && !user.contains('%')
            && !user.chars().any(char::is_whitespace),
        "unsupported HostName or User"
    );
    let mut profile = SessionProfile::new(alias, host, user);
    let ssh = profile.ssh_mut().context("imported profile is not SSH")?;
    if let Some(port) = scalar.get("port") {
        ssh.port = port.parse().context("invalid Port")?;
    }
    if let Some(value) = scalar.get("pubkeyauthentication") {
        anyhow::ensure!(
            value.eq_ignore_ascii_case("yes"),
            "skipped because PubkeyAuthentication requires a different authentication policy"
        );
    }
    let identity_file = identities
        .first()
        .copied()
        .filter(|path| !path.eq_ignore_ascii_case("none"));
    if let Some(value) = scalar.get("identitiesonly") {
        anyhow::ensure!(
            value.eq_ignore_ascii_case("no")
                || (value.eq_ignore_ascii_case("yes") && identity_file.is_some()),
            "skipped because IdentitiesOnly requires an explicit identity file"
        );
    }
    if identities.len() > 1 {
        ignored.insert("additional IdentityFile entries".to_owned());
    }
    ssh.auth = match identity_file {
        Some(path) => {
            anyhow::ensure!(
                !path.contains(['%', '$']),
                "unsupported IdentityFile expansion"
            );
            let path = expand_home(Path::new(path), home);
            anyhow::ensure!(
                path.is_absolute(),
                "IdentityFile must use an absolute path or ~/"
            );
            AuthMethod::PrivateKey { path }
        }
        _ => AuthMethod::SshAgent,
    };
    ssh.x11_forwarding = if scalar
        .get("forwardx11")
        .is_some_and(|v| v.eq_ignore_ascii_case("yes"))
    {
        if scalar
            .get("forwardx11trusted")
            .is_some_and(|v| v.eq_ignore_ascii_case("yes"))
        {
            X11ForwardingMode::Trusted
        } else {
            X11ForwardingMode::Untrusted
        }
    } else {
        X11ForwardingMode::Off
    };
    profile.validate()?;
    Ok(profile)
}

fn expand_home(path: &Path, home: &Path) -> PathBuf {
    path.strip_prefix("~")
        .map_or_else(|_| path.to_owned(), |suffix| home.join(suffix))
}

fn words(line: &str) -> Result<Vec<String>> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut active = false;
    let mut quote = false;
    let mut chars = line.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '#' if !quote && !active => break,
            '"' => {
                active = true;
                quote = !quote;
            }
            '\\' if chars
                .peek()
                .is_some_and(|c| matches!(c, '"' | '\\' | ' ' | '#')) =>
            {
                active = true;
                if let Some(c) = chars.next() {
                    word.push(c);
                }
            }
            c if !quote && c.is_whitespace() => {
                if active {
                    words.push(std::mem::take(&mut word));
                    active = false;
                }
            }
            c => {
                active = true;
                word.push(c);
            }
        }
    }
    anyhow::ensure!(!quote, "unterminated quoted option");
    if active {
        words.push(word);
    }
    Ok(words)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_ordered_host_defaults_and_never_arms_agent_forwarding() {
        let home = std::env::temp_dir();
        let config = parse_config(
            r#"
Host dev !prod
    Hostname dev.example.org
    IdentityFile "~/key with spaces"
    ForwardAgent yes
Host prod
    ProxyJump bastion
Host *
    User ops
    Port=2222
"#,
            &home,
            None,
        )
        .unwrap();
        assert_eq!(config.profiles.len(), 1);
        let ssh = config.profiles[0].ssh().unwrap();
        assert_eq!(
            (ssh.host.as_str(), ssh.username.as_str(), ssh.port),
            ("dev.example.org", "ops", 2222)
        );
        assert_eq!(
            ssh.auth,
            AuthMethod::PrivateKey {
                path: home.join("key with spaces")
            }
        );
        assert!(!ssh.forward_agent);
        assert!(ssh.host_key_fingerprint.is_none());
        assert!(config.notes.iter().any(|line| line.contains("proxyjump")));
    }

    #[test]
    fn agent_fallback_does_not_broaden_identities_only() {
        let result = parse_config(
            "Host limited\n  IdentitiesOnly yes\nHost regular",
            Path::new(""),
            Some("fixture"),
        )
        .unwrap();
        assert_eq!(result.profiles.len(), 1);
        assert_eq!(result.profiles[0].name, "regular");
        assert!(
            result
                .notes
                .iter()
                .any(|line| line.contains("IdentitiesOnly"))
        );
    }

    #[test]
    fn conditional_config_is_rejected_without_evaluating_it() {
        assert!(
            parse_config(
                "Match exec \"invalid-program\"\nHost dev",
                Path::new(""),
                None
            )
            .is_err()
        );
        assert!(parse_config("Include ~/.ssh/conf.d/*", Path::new(""), None).is_err());
    }
}

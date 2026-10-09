//! Canonical system icon choices shared by persistence and application mapping.

use serde::{Deserialize, Deserializer, Serialize};

use super::SessionProfile;

mod catalog;

/// A bounded, bundled choice; IDs are independent of UI frameworks and artwork paths.
pub struct SystemIconChoice {
    pub id: &'static str,
    pub name: &'static str,
    pub family: &'static str,
    pub aliases: &'static [&'static str],
}

/// All bundled choices, ordered by family and display name.
pub fn system_icon_choices() -> &'static [SystemIconChoice] {
    catalog::SYSTEM_ICONS
}

/// Successful discovery, bound to the authenticated SSH destination.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct SystemDetection {
    host: String,
    port: u16,
    username: String,
    host_key_fingerprint: Option<String>,
    release: String,
    #[serde(default, deserialize_with = "deserialize_optional_icon")]
    icon_id: Option<String>,
}

impl SystemDetection {
    /// Failed, empty or unbounded discoveries are never cached.
    pub fn new(profile: &SessionProfile, release: &str, icon_id: &str) -> Option<Self> {
        let ssh = profile.ssh()?;
        if !valid_release(release) {
            return None;
        }
        Some(Self {
            host: ssh.host.clone(),
            port: ssh.port,
            username: ssh.username.clone(),
            host_key_fingerprint: ssh.host_key_fingerprint.clone(),
            release: release.to_owned(),
            icon_id: system_icon_id(icon_id).map(str::to_owned),
        })
    }

    pub fn matches_profile(&self, profile: &SessionProfile) -> bool {
        profile.ssh().is_some_and(|ssh| {
            self.host == ssh.host
                && self.port == ssh.port
                && self.username == ssh.username
                && self.host_key_fingerprint == ssh.host_key_fingerprint
                && valid_release(&self.release)
        })
    }

    pub fn release(&self) -> &str {
        &self.release
    }

    pub fn icon_id(&self) -> Option<&str> {
        self.icon_id.as_deref()
    }
}

fn valid_release(release: &str) -> bool {
    !release.trim().is_empty()
        && release.chars().count() <= 96
        && !release.chars().any(char::is_control)
}

pub(super) fn deserialize_detection<'de, D>(
    deserializer: D,
) -> Result<Option<Box<SystemDetection>>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value::<SystemDetection>(value)
        .ok()
        .filter(|detection| valid_release(&detection.release))
        .map(Box::new))
}

/// Resolve a canonical ID or exact normalized alias. Empty/unknown values mean automatic.
pub fn system_icon_id(value: &str) -> Option<&'static str> {
    if value.len() > 128 {
        return None;
    }
    let value = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    catalog::SYSTEM_ICONS
        .iter()
        .find(|icon| icon.aliases.contains(&value.as_str()))
        .map(|icon| icon.id)
}

pub(super) fn deserialize_optional_icon<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(value.as_str().and_then(system_icon_id).map(str::to_owned))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_catalog_id_and_alias_resolves_to_its_own_icon() {
        for icon in system_icon_choices() {
            assert_eq!(system_icon_id(icon.id), Some(icon.id));
            for alias in icon.aliases {
                assert_eq!(system_icon_id(alias), Some(icon.id));
            }
        }
        assert_eq!(system_icon_id("Red Hat Enterprise Linux"), Some("rhel"));
        assert_eq!(system_icon_id(""), None);
        assert_eq!(system_icon_id("arbitrary/path.svg"), None);
        assert_eq!(system_icon_id(&"x".repeat(129)), None);
    }

    #[test]
    fn discovery_is_bound_to_destination_but_survives_renaming() {
        let mut profile = SessionProfile::new("Server", "server.example", "alice");
        let system = SystemDetection::new(&profile, "Ubuntu 24.04 LTS", "ubuntu").unwrap();
        profile.name = "Renamed".into();
        profile.group_name = "Production".into();
        assert!(system.matches_profile(&profile));
        for field in ["host", "port", "username", "host_key"] {
            let mut changed = profile.clone();
            let ssh = changed.ssh_mut().unwrap();
            match field {
                "host" => ssh.host = "other.example".into(),
                "port" => ssh.port = 2222,
                "username" => ssh.username = "bob".into(),
                _ => ssh.host_key_fingerprint = Some("SHA256:changed".into()),
            }
            assert!(!system.matches_profile(&changed), "{field}");
        }
        assert!(SystemDetection::new(&profile, "", "ubuntu").is_none());
        assert!(SystemDetection::new(&profile, "bad\nvalue", "ubuntu").is_none());
        assert!(SystemDetection::new(&profile, &"x".repeat(97), "ubuntu").is_none());
        let unknown = SystemDetection::new(&profile, "FreeBSD 14.3", "").unwrap();
        assert!(unknown.matches_profile(&profile));
        assert_eq!(unknown.icon_id(), None);
    }

    #[test]
    fn invalid_optional_icon_metadata_does_not_break_profiles() {
        let profile = SessionProfile::new("Server", "server.example", "alice");
        for invalid in [
            serde_json::json!("not-an-icon"),
            serde_json::json!(42),
            serde_json::json!({"release":"incomplete"}),
        ] {
            let mut value = serde_json::to_value(&profile).unwrap();
            value["system_icon_override"] = invalid.clone();
            value["detected_system"] = invalid;
            let loaded: SessionProfile = serde_json::from_value(value).unwrap();
            assert!(loaded.system_icon_override.is_none());
            assert!(loaded.cached_system().is_none());
        }
        let mut value = serde_json::to_value(&profile).unwrap();
        value["system_icon_override"] = serde_json::json!("Debian");
        value["detected_system"] =
            serde_json::to_value(SystemDetection::new(&profile, "Ubuntu", "ubuntu")).unwrap();
        value["connection"]["config"]["host"] = serde_json::json!("other.example");
        let loaded: SessionProfile = serde_json::from_value(value).unwrap();
        assert_eq!(loaded.system_icon_override.as_deref(), Some("debian"));
        assert!(loaded.detected_system.is_none());
    }
}

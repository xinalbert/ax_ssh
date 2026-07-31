use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::settings::{default_toggle_sidebar_shortcut, previous_toggle_sidebar_shortcut};
use super::{
    AppSettings, AppearanceSettings, CURRENT_SCHEMA_VERSION, DEFAULT_SIDEBAR_WIDTH,
    PLATFORM_SHORTCUT_SCHEMA_VERSION, PREVIOUS_DEFAULT_SIDEBAR_WIDTH,
    THEME_SETTINGS_SCHEMA_VERSION, ThemeSettings, WORKSPACE_DENSITY_SCHEMA_VERSION,
};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum CredentialStorage {
    #[default]
    SystemKeyring,
    EncryptedVault,
}

impl CredentialStorage {
    pub fn from_setting(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "system-keyring" | "system keyring" | "keyring" | "stored" => Self::SystemKeyring,
            "encrypted-vault" | "encrypted vault" | "vault" => Self::EncryptedVault,
            _ => Self::SystemKeyring,
        }
    }

    pub const fn as_setting(self) -> &'static str {
        match self {
            Self::SystemKeyring => "system-keyring",
            Self::EncryptedVault => "encrypted-vault",
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct SessionProfile {
    pub id: Uuid,
    pub name: String,
    #[serde(default)]
    pub group_name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: AuthMethod,
    /// The backend containing an already remembered password. The credential
    /// itself is never serialized in this profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_storage: Option<CredentialStorage>,
    /// A SHA-256 SSH public-key fingerprint. The empty value means unknown;
    /// the SSH layer must refuse the connection until it is trusted.
    #[serde(default)]
    pub host_key_fingerprint: Option<String>,
}

#[derive(Deserialize)]
struct SessionProfileWire {
    id: Uuid,
    name: String,
    #[serde(default)]
    group_name: String,
    host: String,
    port: u16,
    username: String,
    auth: AuthMethod,
    #[serde(default)]
    credential_storage: Option<CredentialStorage>,
    #[serde(default)]
    credential_stored: Option<bool>,
    #[serde(default)]
    host_key_fingerprint: Option<String>,
}

impl<'de> Deserialize<'de> for SessionProfile {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = SessionProfileWire::deserialize(deserializer)?;
        let credential_storage = wire.credential_storage.or_else(|| {
            wire.credential_stored
                .unwrap_or(false)
                .then_some(CredentialStorage::SystemKeyring)
        });
        let auth = wire.auth;
        if credential_storage.is_some() && matches!(auth, AuthMethod::PrivateKey { .. }) {
            return Err(serde::de::Error::custom(
                "private-key profiles cannot store password credentials",
            ));
        }
        Ok(Self {
            id: wire.id,
            name: wire.name,
            group_name: wire.group_name,
            host: wire.host,
            port: wire.port,
            username: wire.username,
            auth,
            credential_storage,
            host_key_fingerprint: wire.host_key_fingerprint,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum AuthMethod {
    Password,
    PrivateKey { path: PathBuf },
}

impl Default for AuthMethod {
    fn default() -> Self {
        Self::Password
    }
}
impl SessionProfile {
    pub fn new(
        name: impl Into<String>,
        host: impl Into<String>,
        username: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            group_name: String::new(),
            host: host.into(),
            port: 22,
            username: username.into(),
            auth: AuthMethod::default(),
            credential_storage: None,
            host_key_fingerprint: None,
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            anyhow::bail!("session name cannot be empty");
        }
        if self.host.trim().is_empty() {
            anyhow::bail!("host cannot be empty");
        }
        if self.username.trim().is_empty() {
            anyhow::bail!("username cannot be empty");
        }
        if self.port == 0 {
            anyhow::bail!("port must be between 1 and 65535");
        }
        validate_group_name(&self.group_name, true)?;
        if let AuthMethod::PrivateKey { path } = &self.auth {
            if path.as_os_str().is_empty() {
                anyhow::bail!("private key path cannot be empty");
            }
            if self.credential_storage.is_some() {
                anyhow::bail!("private-key profiles cannot store password credentials");
            }
        }
        Ok(())
    }
}

pub fn normalize_group_name(value: &str) -> String {
    let value = value.trim();
    if value.eq_ignore_ascii_case("Ungrouped") {
        String::new()
    } else {
        value.to_owned()
    }
}

fn validate_group_name(value: &str, allow_empty: bool) -> Result<()> {
    let value = normalize_group_name(value);
    if !allow_empty && value.is_empty() {
        anyhow::bail!("group name cannot be empty");
    }
    if value.chars().count() > 64 {
        anyhow::bail!("group name cannot exceed 64 characters");
    }
    if value.chars().any(char::is_control) {
        anyhow::bail!("group name cannot contain control characters");
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct SessionStore {
    pub version: u32,
    #[serde(default)]
    pub groups: Vec<String>,
    #[serde(default)]
    pub sessions: Vec<SessionProfile>,
    pub settings: AppSettings,
}

impl Default for SessionStore {
    fn default() -> Self {
        Self {
            version: CURRENT_SCHEMA_VERSION,
            groups: Vec::new(),
            sessions: Vec::new(),
            settings: AppSettings::default(),
        }
    }
}

#[derive(Deserialize)]
struct SessionStoreWire {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    groups: Vec<String>,
    #[serde(default)]
    sessions: Vec<SessionProfile>,
    #[serde(default)]
    settings: Option<AppSettings>,
    #[serde(default)]
    appearance: Option<AppearanceSettings>,
}

impl<'de> Deserialize<'de> for SessionStore {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = SessionStoreWire::deserialize(deserializer)?;
        let mut settings = wire.settings.unwrap_or_default();
        if let Some(appearance) = wire.appearance
            && settings.appearance == AppearanceSettings::default()
        {
            settings.appearance = appearance;
        }
        if wire.version < PLATFORM_SHORTCUT_SCHEMA_VERSION {
            if settings
                .shortcuts
                .toggle_sidebar
                .eq_ignore_ascii_case(&previous_toggle_sidebar_shortcut())
            {
                settings.shortcuts.toggle_sidebar = default_toggle_sidebar_shortcut();
            }
        }
        if wire.version < WORKSPACE_DENSITY_SCHEMA_VERSION
            && settings.workspace.sidebar_width == PREVIOUS_DEFAULT_SIDEBAR_WIDTH
        {
            settings.workspace.sidebar_width = DEFAULT_SIDEBAR_WIDTH;
        }
        if wire.version < THEME_SETTINGS_SCHEMA_VERSION {
            settings.appearance.theme = ThemeSettings::from_terminal_color_scheme(
                settings.appearance.terminal_color_scheme,
            );
        }
        settings.normalize_in_place();
        let mut store = Self {
            version: wire.version.max(CURRENT_SCHEMA_VERSION),
            groups: wire.groups,
            sessions: wire.sessions,
            settings,
        };
        store.normalize_groups();
        Ok(store)
    }
}

impl SessionStore {
    pub fn upsert(&mut self, mut profile: SessionProfile) {
        profile.group_name = normalize_group_name(&profile.group_name);
        if !profile.group_name.is_empty() && !self.groups.contains(&profile.group_name) {
            self.groups.push(profile.group_name.clone());
        }
        if let Some(existing) = self.sessions.iter_mut().find(|item| item.id == profile.id) {
            *existing = profile;
        } else {
            self.sessions.push(profile);
        }
    }

    pub fn remove(&mut self, id: Uuid) -> bool {
        let before = self.sessions.len();
        self.sessions.retain(|item| item.id != id);
        before != self.sessions.len()
    }

    pub fn add_group(&mut self, name: &str) -> Result<bool> {
        validate_group_name(name, false)?;
        let name = normalize_group_name(name);
        if self.groups.contains(&name) {
            return Ok(false);
        }
        self.groups.push(name);
        Ok(true)
    }

    pub fn rename_group(&mut self, old_name: &str, new_name: &str) -> Result<bool> {
        let old_name = normalize_group_name(old_name);
        validate_group_name(&old_name, false)?;
        validate_group_name(new_name, false)?;
        let new_name = normalize_group_name(new_name);
        if old_name == new_name {
            return Ok(false);
        }
        if self.groups.contains(&new_name) {
            anyhow::bail!("group already exists");
        }
        let Some(group) = self.groups.iter_mut().find(|group| **group == old_name) else {
            return Ok(false);
        };
        *group = new_name.clone();
        for profile in &mut self.sessions {
            if normalize_group_name(&profile.group_name) == old_name {
                profile.group_name = new_name.clone();
            }
        }
        Ok(true)
    }

    pub fn remove_group(&mut self, name: &str) -> bool {
        let name = normalize_group_name(name);
        let before = self.groups.len();
        self.groups.retain(|group| group != &name);
        if self.groups.len() == before {
            return false;
        }
        for profile in &mut self.sessions {
            if normalize_group_name(&profile.group_name) == name {
                profile.group_name.clear();
            }
        }
        true
    }

    fn normalize_groups(&mut self) {
        let mut normalized = Vec::new();
        for name in self
            .groups
            .iter()
            .chain(self.sessions.iter().map(|profile| &profile.group_name))
        {
            let name = normalize_group_name(name);
            if !name.is_empty() && !normalized.contains(&name) {
                normalized.push(name);
            }
        }
        self.groups = normalized;
        for profile in &mut self.sessions {
            profile.group_name = normalize_group_name(&profile.group_name);
        }
    }
}

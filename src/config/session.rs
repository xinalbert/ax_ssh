use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::settings::{default_toggle_sidebar_shortcut, previous_toggle_sidebar_shortcut};
use super::{
    AppSettings, AppearanceSettings, CURRENT_SCHEMA_VERSION, DEFAULT_SIDEBAR_WIDTH,
    DEFAULT_TERMINAL_CONTRAST_RATIO_TENTHS, PLATFORM_SHORTCUT_SCHEMA_VERSION,
    PREVIOUS_DEFAULT_SIDEBAR_WIDTH, TERMINAL_CONTRAST_SCHEMA_VERSION,
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

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SessionProtocol {
    #[default]
    Ssh,
    Telnet,
    Serial,
}

impl SessionProtocol {
    pub const fn as_setting(self) -> &'static str {
        match self {
            Self::Ssh => "ssh",
            Self::Telnet => "telnet",
            Self::Serial => "serial",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct SshConfig {
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_key_fingerprint: Option<String>,
    /// Whether this profile may request X11 forwarding for terminal sessions.
    /// Authentication cookies remain worker-local and are never persisted.
    #[serde(default = "default_true")]
    pub x11_forwarding: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct TelnetConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SerialDataBits {
    Five,
    Six,
    Seven,
    #[default]
    Eight,
}

impl SerialDataBits {
    pub const fn as_setting(self) -> &'static str {
        match self {
            Self::Five => "5",
            Self::Six => "6",
            Self::Seven => "7",
            Self::Eight => "8",
        }
    }

    pub fn from_setting(value: &str) -> Self {
        match value.trim() {
            "5" => Self::Five,
            "6" => Self::Six,
            "7" => Self::Seven,
            _ => Self::Eight,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SerialStopBits {
    #[default]
    One,
    Two,
}

impl SerialStopBits {
    pub const fn as_setting(self) -> &'static str {
        match self {
            Self::One => "1",
            Self::Two => "2",
        }
    }

    pub fn from_setting(value: &str) -> Self {
        if value.trim() == "2" {
            Self::Two
        } else {
            Self::One
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SerialParity {
    #[default]
    None,
    Odd,
    Even,
}

impl SerialParity {
    pub const fn as_setting(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Odd => "odd",
            Self::Even => "even",
        }
    }

    pub fn from_setting(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "odd" => Self::Odd,
            "even" => Self::Even,
            _ => Self::None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SerialFlowControl {
    #[default]
    None,
    Software,
    Hardware,
}

impl SerialFlowControl {
    pub const fn as_setting(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Software => "software",
            Self::Hardware => "hardware",
        }
    }

    pub fn from_setting(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "software" => Self::Software,
            "hardware" => Self::Hardware,
            _ => Self::None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct SerialConfig {
    pub port_name: String,
    pub baud_rate: u32,
    #[serde(default)]
    pub data_bits: SerialDataBits,
    #[serde(default)]
    pub stop_bits: SerialStopBits,
    #[serde(default)]
    pub parity: SerialParity,
    #[serde(default)]
    pub flow_control: SerialFlowControl,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usb_vendor_id: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usb_product_id: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usb_serial_number: Option<String>,
}

impl Default for SerialConfig {
    fn default() -> Self {
        Self {
            port_name: String::new(),
            baud_rate: 115_200,
            data_bits: SerialDataBits::default(),
            stop_bits: SerialStopBits::default(),
            parity: SerialParity::default(),
            flow_control: SerialFlowControl::default(),
            usb_vendor_id: None,
            usb_product_id: None,
            usb_serial_number: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "protocol", content = "config", rename_all = "kebab-case")]
pub enum ConnectionProfile {
    Ssh(SshConfig),
    Telnet(TelnetConfig),
    Serial(SerialConfig),
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct SessionProfile {
    pub id: Uuid,
    pub name: String,
    #[serde(default)]
    pub group_name: String,
    pub connection: ConnectionProfile,
}

#[derive(Deserialize)]
struct SessionProfileWire {
    id: Uuid,
    name: String,
    #[serde(default)]
    group_name: String,
    #[serde(default)]
    connection: Option<ConnectionProfile>,
    #[serde(default)]
    host: Option<String>,
    #[serde(default)]
    port: Option<u16>,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    auth: Option<AuthMethod>,
    #[serde(default)]
    credential_storage: Option<CredentialStorage>,
    #[serde(default)]
    credential_stored: Option<bool>,
    #[serde(default)]
    host_key_fingerprint: Option<String>,
    #[serde(default)]
    x11_forwarding: Option<bool>,
}

impl<'de> Deserialize<'de> for SessionProfile {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = SessionProfileWire::deserialize(deserializer)?;
        let connection = if let Some(connection) = wire.connection {
            if wire.host.is_some()
                || wire.port.is_some()
                || wire.username.is_some()
                || wire.auth.is_some()
                || wire.credential_storage.is_some()
                || wire.credential_stored.is_some()
                || wire.host_key_fingerprint.is_some()
                || wire.x11_forwarding.is_some()
            {
                return Err(serde::de::Error::custom(
                    "profile cannot mix legacy SSH fields with a protocol config",
                ));
            }
            connection
        } else {
            let credential_storage = wire.credential_storage.or_else(|| {
                wire.credential_stored
                    .unwrap_or(false)
                    .then_some(CredentialStorage::SystemKeyring)
            });
            ConnectionProfile::Ssh(SshConfig {
                host: wire
                    .host
                    .ok_or_else(|| serde::de::Error::missing_field("host"))?,
                port: wire
                    .port
                    .ok_or_else(|| serde::de::Error::missing_field("port"))?,
                username: wire
                    .username
                    .ok_or_else(|| serde::de::Error::missing_field("username"))?,
                auth: wire
                    .auth
                    .ok_or_else(|| serde::de::Error::missing_field("auth"))?,
                credential_storage,
                host_key_fingerprint: wire.host_key_fingerprint,
                x11_forwarding: wire.x11_forwarding.unwrap_or(true),
            })
        };
        validate_connection_consistency(&connection).map_err(serde::de::Error::custom)?;
        Ok(Self {
            id: wire.id,
            name: wire.name,
            group_name: wire.group_name,
            connection,
        })
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
            connection: ConnectionProfile::Ssh(SshConfig {
                host: host.into(),
                port: 22,
                username: username.into(),
                auth: AuthMethod::default(),
                credential_storage: None,
                host_key_fingerprint: None,
                x11_forwarding: true,
            }),
        }
    }

    pub fn new_telnet(name: impl Into<String>, host: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            group_name: String::new(),
            connection: ConnectionProfile::Telnet(TelnetConfig {
                host: host.into(),
                port: 23,
            }),
        }
    }

    pub fn new_serial(name: impl Into<String>, port_name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            group_name: String::new(),
            connection: ConnectionProfile::Serial(SerialConfig {
                port_name: port_name.into(),
                ..SerialConfig::default()
            }),
        }
    }

    pub const fn protocol(&self) -> SessionProtocol {
        match self.connection {
            ConnectionProfile::Ssh(_) => SessionProtocol::Ssh,
            ConnectionProfile::Telnet(_) => SessionProtocol::Telnet,
            ConnectionProfile::Serial(_) => SessionProtocol::Serial,
        }
    }

    pub fn ssh(&self) -> Option<&SshConfig> {
        match &self.connection {
            ConnectionProfile::Ssh(config) => Some(config),
            ConnectionProfile::Telnet(_) | ConnectionProfile::Serial(_) => None,
        }
    }

    pub fn ssh_mut(&mut self) -> Option<&mut SshConfig> {
        match &mut self.connection {
            ConnectionProfile::Ssh(config) => Some(config),
            ConnectionProfile::Telnet(_) | ConnectionProfile::Serial(_) => None,
        }
    }

    pub fn telnet(&self) -> Option<&TelnetConfig> {
        match &self.connection {
            ConnectionProfile::Telnet(config) => Some(config),
            ConnectionProfile::Ssh(_) | ConnectionProfile::Serial(_) => None,
        }
    }

    pub fn serial(&self) -> Option<&SerialConfig> {
        match &self.connection {
            ConnectionProfile::Serial(config) => Some(config),
            ConnectionProfile::Ssh(_) | ConnectionProfile::Telnet(_) => None,
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            anyhow::bail!("session name cannot be empty");
        }
        validate_group_name(&self.group_name, true)?;
        validate_connection_consistency(&self.connection)?;
        Ok(())
    }
}

const fn default_true() -> bool {
    true
}

fn validate_connection_consistency(connection: &ConnectionProfile) -> Result<()> {
    match connection {
        ConnectionProfile::Ssh(config) => {
            validate_host_and_port(&config.host, config.port)?;
            if config.username.trim().is_empty() {
                anyhow::bail!("username cannot be empty");
            }
            if let AuthMethod::PrivateKey { path } = &config.auth {
                if path.as_os_str().is_empty() {
                    anyhow::bail!("private key path cannot be empty");
                }
                if config.credential_storage.is_some() {
                    anyhow::bail!("private-key profiles cannot store password credentials");
                }
            }
        }
        ConnectionProfile::Telnet(config) => validate_host_and_port(&config.host, config.port)?,
        ConnectionProfile::Serial(config) => {
            let port_name = config.port_name.trim();
            if port_name.is_empty() {
                anyhow::bail!("serial port cannot be empty");
            }
            if port_name.chars().count() > 512 || port_name.chars().any(char::is_control) {
                anyhow::bail!("serial port name is invalid");
            }
            if config.baud_rate == 0 || config.baud_rate > 12_000_000 {
                anyhow::bail!("serial baud rate must be between 1 and 12000000");
            }
            if let Some(serial_number) = &config.usb_serial_number
                && (serial_number.chars().count() > 256
                    || serial_number.chars().any(char::is_control))
            {
                anyhow::bail!("USB serial number is invalid");
            }
        }
    }
    Ok(())
}

fn validate_host_and_port(host: &str, port: u16) -> Result<()> {
    if host.trim().is_empty() {
        anyhow::bail!("host cannot be empty");
    }
    if port == 0 {
        anyhow::bail!("port must be between 1 and 65535");
    }
    Ok(())
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
        if wire.version < TERMINAL_CONTRAST_SCHEMA_VERSION {
            settings.appearance.terminal_minimum_contrast_ratio_tenths =
                DEFAULT_TERMINAL_CONTRAST_RATIO_TENTHS;
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

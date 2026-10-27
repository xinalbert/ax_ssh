//! Bounded session/group JSON exchange shared by both frontends.
//! No I/O, credential backend or transport is accessed by this module.
use crate::config::{
    AuthMethod, ConnectionProfile, MAX_HOST_CHARS, MAX_PRIVATE_KEY_PATH_CHARS,
    MAX_SESSION_NAME_CHARS, MAX_USERNAME_CHARS, SessionProfile, SessionStore, normalize_group_name,
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// One explicit saved catalog target; never a live connection or credential.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionExportTarget {
    Server(Uuid),
    Group(String),
}

pub(crate) const SESSION_TRANSFER_FORMAT: &str = "axssh-session-export";
pub(crate) const SESSION_TRANSFER_VERSION: u32 = 1;
pub const MAX_SESSION_TRANSFER_BYTES: usize = 256 * 1024;
pub const MAX_SESSION_TRANSFER_PROFILES: usize = 128;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SessionTransferKind {
    Server,
    Group,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct SessionTransferEnvelope {
    pub(crate) format: String,
    pub(crate) version: u32,
    pub(crate) kind: SessionTransferKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) group_name: Option<String>,
    pub(crate) profiles: Vec<SessionProfile>,
}

#[derive(Clone, Copy)]
pub enum SessionImportMode {
    SingleServer,
    IntoGroup,
    Automatic,
}

pub fn export_session_profile(sessions: &SessionStore, session_id: &str) -> Result<String> {
    let session_id = Uuid::parse_str(session_id).context("invalid session id")?;
    let mut profile = sessions
        .sessions
        .iter()
        .find(|profile| profile.id == session_id)
        .cloned()
        .context("session not found")?;
    sanitize_transferred_profile(&mut profile);
    serialize_session_transfer(SessionTransferEnvelope {
        format: SESSION_TRANSFER_FORMAT.to_owned(),
        version: SESSION_TRANSFER_VERSION,
        kind: SessionTransferKind::Server,
        group_name: None,
        profiles: vec![profile],
    })
}

pub fn export_session_group(sessions: &SessionStore, group_name: &str) -> Result<String> {
    let group_name = normalized_transfer_group_name(group_name)?;
    if !sessions.groups.iter().any(|group| group == &group_name) {
        anyhow::bail!("group not found");
    }
    let mut profiles = sessions
        .sessions
        .iter()
        .filter(|profile| normalize_group_name(&profile.group_name) == group_name)
        .cloned()
        .collect::<Vec<_>>();
    if profiles.len() > MAX_SESSION_TRANSFER_PROFILES {
        anyhow::bail!("group exceeds the {MAX_SESSION_TRANSFER_PROFILES} server export limit");
    }
    for profile in &mut profiles {
        sanitize_transferred_profile(profile);
    }
    serialize_session_transfer(SessionTransferEnvelope {
        format: SESSION_TRANSFER_FORMAT.to_owned(),
        version: SESSION_TRANSFER_VERSION,
        kind: SessionTransferKind::Group,
        group_name: Some(group_name),
        profiles,
    })
}

fn serialize_session_transfer(envelope: SessionTransferEnvelope) -> Result<String> {
    validate_session_transfer(&envelope)?;
    let text =
        serde_json::to_string_pretty(&envelope).context("cannot serialize session export")?;
    if text.len() > MAX_SESSION_TRANSFER_BYTES {
        anyhow::bail!("session export exceeds the 256 KiB clipboard limit");
    }
    Ok(text)
}

pub(crate) fn parse_session_transfer(text: &str) -> Result<SessionTransferEnvelope> {
    if text.trim().is_empty() {
        anyhow::bail!("clipboard is empty");
    }
    if text.len() > MAX_SESSION_TRANSFER_BYTES {
        anyhow::bail!("clipboard data exceeds the 256 KiB import limit");
    }
    let mut envelope: SessionTransferEnvelope =
        serde_json::from_str(text).context("clipboard does not contain AxSSH session JSON")?;
    for profile in &mut envelope.profiles {
        sanitize_transferred_profile(profile);
    }
    validate_session_transfer(&envelope)?;
    Ok(envelope)
}

fn validate_session_transfer(envelope: &SessionTransferEnvelope) -> Result<()> {
    if envelope.format != SESSION_TRANSFER_FORMAT {
        anyhow::bail!("clipboard data is not an AxSSH session export");
    }
    if envelope.version != SESSION_TRANSFER_VERSION {
        anyhow::bail!("unsupported AxSSH session export version");
    }
    if envelope.profiles.len() > MAX_SESSION_TRANSFER_PROFILES {
        anyhow::bail!("session export exceeds the {MAX_SESSION_TRANSFER_PROFILES} server limit");
    }
    match envelope.kind {
        SessionTransferKind::Server => {
            if envelope.group_name.is_some() || envelope.profiles.len() != 1 {
                anyhow::bail!("server export must contain exactly one server");
            }
        }
        SessionTransferKind::Group => {
            let group_name = envelope
                .group_name
                .as_deref()
                .context("group export is missing its group name")?;
            normalized_transfer_group_name(group_name)?;
        }
    }
    for profile in &envelope.profiles {
        validate_transferred_profile(profile)?;
    }
    Ok(())
}

fn validate_transferred_profile(profile: &SessionProfile) -> Result<()> {
    profile.validate()?;
    validate_transfer_text("session name", &profile.name, MAX_SESSION_NAME_CHARS)?;
    match &profile.connection {
        ConnectionProfile::Ssh(ssh) => {
            validate_transfer_text("SSH host", &ssh.host, MAX_HOST_CHARS)?;
            validate_transfer_text("SSH username", &ssh.username, MAX_USERNAME_CHARS)?;
            if let AuthMethod::PrivateKey { path } = &ssh.auth {
                let path = path
                    .to_str()
                    .context("private key path must use valid Unicode")?;
                validate_transfer_text("private key path", path, MAX_PRIVATE_KEY_PATH_CHARS)?;
            }
        }
        ConnectionProfile::Telnet(telnet) => {
            validate_transfer_text("Telnet host", &telnet.host, MAX_HOST_CHARS)?;
        }
        ConnectionProfile::Serial(serial) => {
            validate_transfer_text("serial port", &serial.port_name, MAX_HOST_CHARS)?;
            if let Some(serial_number) = &serial.usb_serial_number {
                validate_transfer_text("USB serial number", serial_number, 256)?;
            }
        }
    }
    Ok(())
}

fn validate_transfer_text(label: &str, value: &str, max_chars: usize) -> Result<()> {
    let value = value.trim();
    if value.is_empty() {
        anyhow::bail!("{label} cannot be empty");
    }
    if value.chars().count() > max_chars || value.chars().any(char::is_control) {
        anyhow::bail!("{label} is invalid");
    }
    Ok(())
}

fn sanitize_transferred_profile(profile: &mut SessionProfile) {
    profile.id = Uuid::nil();
    profile.detected_system = None;
    profile.group_name = normalize_group_name(&profile.group_name);
    if let Some(ssh) = profile.ssh_mut() {
        ssh.credential_storage = None;
        ssh.credential_vault_key_saved = false;
        ssh.host_key_fingerprint = None;
    }
}

pub fn normalized_transfer_group_name(group_name: &str) -> Result<String> {
    let group_name = normalize_group_name(group_name);
    let mut validation_store = SessionStore::default();
    validation_store.add_group(&group_name)?;
    Ok(group_name)
}

pub fn import_session_transfer_into_store(
    sessions: &mut SessionStore,
    text: &str,
    mode: SessionImportMode,
    target_group: &str,
) -> Result<(usize, Option<String>)> {
    let envelope = parse_session_transfer(text)?;
    let target_group = normalize_group_name(target_group);
    let imported_group = match mode {
        SessionImportMode::SingleServer => {
            if envelope.kind != SessionTransferKind::Server {
                anyhow::bail!("Import Server requires a single-server export");
            }
            if !target_group.is_empty() {
                ensure_session_group(sessions, &target_group)?;
            }
            Some(target_group)
        }
        SessionImportMode::IntoGroup => {
            let target_group = normalized_transfer_group_name(&target_group)?;
            if envelope.profiles.is_empty() {
                anyhow::bail!("clipboard export contains no servers");
            }
            ensure_session_group(sessions, &target_group)?;
            Some(target_group)
        }
        SessionImportMode::Automatic => match envelope.kind {
            SessionTransferKind::Group => {
                let source_group = envelope
                    .group_name
                    .as_deref()
                    .context("group export is missing its group name")?;
                let group = unique_imported_group_name(sessions, source_group)?;
                sessions.add_group(&group)?;
                Some(group)
            }
            SessionTransferKind::Server => None,
        },
    };

    let profile_count = envelope.profiles.len();
    for mut profile in envelope.profiles {
        let destination_group = imported_group
            .clone()
            .unwrap_or_else(|| normalize_group_name(&profile.group_name));
        if !destination_group.is_empty() {
            ensure_session_group(sessions, &destination_group)?;
        }
        profile.id = Uuid::new_v4();
        profile.group_name = destination_group;
        profile.name = unique_imported_session_name(sessions, &profile.name)?;
        sanitize_imported_profile(&mut profile);
        validate_transferred_profile(&profile)?;
        sessions.upsert(profile);
    }
    Ok((
        profile_count,
        imported_group.filter(|group| !group.is_empty()),
    ))
}

fn sanitize_imported_profile(profile: &mut SessionProfile) {
    let id = profile.id;
    let group_name = profile.group_name.clone();
    sanitize_transferred_profile(profile);
    profile.id = id;
    profile.group_name = group_name;
}

fn ensure_session_group(sessions: &mut SessionStore, group_name: &str) -> Result<()> {
    if group_name.is_empty() || sessions.groups.iter().any(|group| group == group_name) {
        return Ok(());
    }
    sessions.add_group(group_name)?;
    Ok(())
}

pub fn duplicate_group_name(sessions: &SessionStore, name: &str) -> Result<String> {
    let name = normalized_transfer_group_name(name)?;
    let mut copy_number = 1u32;
    loop {
        let suffix = if copy_number == 1 {
            " Copy".to_owned()
        } else {
            format!(" Copy {copy_number}")
        };
        let stem_limit = 64usize.saturating_sub(suffix.chars().count());
        let stem = name.chars().take(stem_limit).collect::<String>();
        let candidate = format!("{}{suffix}", stem.trim_end());
        normalized_transfer_group_name(&candidate)?;
        if !sessions.groups.iter().any(|group| group == &candidate) {
            return Ok(candidate);
        }
        copy_number = copy_number
            .checked_add(1)
            .context("too many duplicate group names")?;
    }
}

fn unique_imported_group_name(sessions: &SessionStore, name: &str) -> Result<String> {
    let name = normalized_transfer_group_name(name)?;
    if !sessions.groups.iter().any(|group| group == &name) {
        return Ok(name);
    }
    duplicate_group_name(sessions, &name)
}

fn unique_imported_session_name(sessions: &SessionStore, name: &str) -> Result<String> {
    if !sessions.sessions.iter().any(|profile| profile.name == name) {
        return Ok(name.to_owned());
    }
    let mut copy_number = 1u32;
    loop {
        let suffix = if copy_number == 1 {
            " Copy".to_owned()
        } else {
            format!(" Copy {copy_number}")
        };
        let stem_limit = MAX_SESSION_NAME_CHARS.saturating_sub(suffix.chars().count());
        let stem = name.chars().take(stem_limit).collect::<String>();
        let candidate = format!("{}{suffix}", stem.trim_end());
        if !sessions
            .sessions
            .iter()
            .any(|profile| profile.name == candidate)
        {
            return Ok(candidate);
        }
        copy_number = copy_number
            .checked_add(1)
            .context("too many imported session name conflicts")?;
    }
}

#[cfg(test)]
mod tests;

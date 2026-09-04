use super::*;
use crate::app::state::PersistenceCoordinator;
use serde::{Deserialize, Serialize};

pub(super) const SESSION_TRANSFER_FORMAT: &str = "axssh-session-export";
pub(super) const SESSION_TRANSFER_VERSION: u32 = 1;
pub(super) const MAX_SESSION_TRANSFER_BYTES: usize = 256 * 1024;
const MAX_SESSION_TRANSFER_PROFILES: usize = 128;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(super) enum SessionTransferKind {
    Server,
    Group,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct SessionTransferEnvelope {
    pub(super) format: String,
    pub(super) version: u32,
    pub(super) kind: SessionTransferKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) group_name: Option<String>,
    pub(super) profiles: Vec<SessionProfile>,
}

#[derive(Clone, Copy)]
pub(super) enum SessionImportMode {
    SingleServer,
    IntoGroup,
    Automatic,
}

pub(in crate::app) fn wire_session_management(
    ui: &AppWindow,
    state: Arc<Mutex<AppState>>,
    runtime: Handle,
    persistence: Arc<PersistenceCoordinator>,
) {
    let ui_for_duplicate = ui.as_weak();
    let state_for_duplicate = state.clone();
    let runtime_for_duplicate = runtime.clone();
    let persistence_for_duplicate = persistence.clone();
    ui.on_duplicate_session(move |id| {
        log_ui_action("session.duplicate");
        let id = match parse_uuid(id.as_str(), "session", &ui_for_duplicate) {
            Some(id) => id,
            None => return,
        };
        let ui = ui_for_duplicate.clone();
        let state = state_for_duplicate.clone();
        let persistence = persistence_for_duplicate.clone();
        runtime_for_duplicate.spawn(async move {
            let _persistence_guard = persistence.gate.lock().await;
            match duplicate_session_profile(&state, id) {
                Ok(message) => {
                    refresh_session_models(&ui, &state);
                    refresh_workspace(&ui, &state);
                    set_status(&ui, &message);
                }
                Err(error) => {
                    set_status(&ui, &format!("Cannot duplicate session: {error}"));
                }
            }
        });
    });

    let ui_for_transfer = ui.as_weak();
    let state_for_transfer = state.clone();
    let runtime_for_transfer = runtime.clone();
    let persistence_for_transfer = persistence.clone();
    ui.on_session_transfer_action(move |action, target| {
        let action = action.as_str();
        let target = target.as_str().to_owned();
        match action {
            "copy-server" | "copy-group" => {
                log_ui_action(if action == "copy-server" {
                    "session.copy-config"
                } else {
                    "group.copy-config"
                });
                let export = match state_for_transfer.lock() {
                    Ok(app) if action == "copy-server" => {
                        export_session_profile(&app.sessions, &target)
                    }
                    Ok(app) => export_session_group(&app.sessions, &target),
                    Err(_) => Err(anyhow::anyhow!("state lock poisoned")),
                };
                match export {
                    Ok(text) => {
                        if let Some(ui) = ui_for_transfer.upgrade() {
                            set_platform_clipboard_text(&ui, &text);
                            set_status(
                                &ui_for_transfer,
                                if action == "copy-server" {
                                    "Server copied as AxSSH JSON"
                                } else {
                                    "Group copied as AxSSH JSON"
                                },
                            );
                        }
                    }
                    Err(error) => set_status(
                        &ui_for_transfer,
                        &format!("Cannot copy session configuration: {error}"),
                    ),
                }
            }
            "duplicate-group" => {
                log_ui_action("group.duplicate");
                let ui = ui_for_transfer.clone();
                let state = state_for_transfer.clone();
                let persistence = persistence_for_transfer.clone();
                runtime_for_transfer.spawn(async move {
                    let _persistence_guard = persistence.gate.lock().await;
                    match duplicate_session_group(&state, &target) {
                        Ok(message) => {
                            refresh_session_models(&ui, &state);
                            refresh_workspace(&ui, &state);
                            set_status(&ui, &message);
                        }
                        Err(error) => {
                            set_status(&ui, &format!("Cannot duplicate group: {error}"));
                        }
                    }
                });
            }
            "import-server" | "import-group" | "import-any" => {
                log_ui_action(match action {
                    "import-server" => "session.import-config",
                    "import-group" => "group.import-config",
                    _ => "session-list.import-config",
                });
                let Some(ui) = ui_for_transfer.upgrade() else {
                    return;
                };
                let clipboard = platform_clipboard_text(&ui);
                let mode = match action {
                    "import-server" => SessionImportMode::SingleServer,
                    "import-group" => SessionImportMode::IntoGroup,
                    _ => SessionImportMode::Automatic,
                };
                let ui = ui_for_transfer.clone();
                let state = state_for_transfer.clone();
                let persistence = persistence_for_transfer.clone();
                runtime_for_transfer.spawn(async move {
                    let _persistence_guard = persistence.gate.lock().await;
                    match import_session_transfer(&state, &clipboard, mode, &target) {
                        Ok(message) => {
                            refresh_session_models(&ui, &state);
                            refresh_workspace(&ui, &state);
                            set_status(&ui, &message);
                        }
                        Err(error) => {
                            set_status(&ui, &format!("Cannot import sessions: {error}"));
                        }
                    }
                });
            }
            "export-none" => {
                log_ui_action("session.export-without-selection");
                set_status(
                    &ui_for_transfer,
                    "Select a group or server before exporting",
                );
            }
            _ => set_status(&ui_for_transfer, "Unknown session transfer action"),
        }
    });

    let ui_for_action = ui.as_weak();
    let persistence_for_action = persistence;
    ui.on_manage_session_action(move |action, target, value| {
        log_ui_action("session-management.execute");
        let action = action.as_str().to_owned();
        let target = target.as_str().to_owned();
        let value = value.as_str().to_owned();
        let ui = ui_for_action.clone();
        let state = state.clone();
        let persistence = persistence_for_action.clone();
        runtime.spawn(async move {
            let result = if action == "delete-session" {
                delete_session_profile(&state, &persistence, &target).await
            } else {
                let _persistence_guard = persistence.gate.lock().await;
                update_session_group(&state, &action, &target, &value)
            };
            match result {
                Ok(message) => {
                    if action == "delete-session"
                        && !state.lock().is_ok_and(|app| app.has_session_editor_tab())
                    {
                        clear_session_editor_resources(&ui);
                    }
                    refresh_session_models(&ui, &state);
                    refresh_workspace(&ui, &state);
                    set_status(&ui, &message);
                }
                Err(error) => {
                    set_status(&ui, &format!("Cannot update sessions: {error}"));
                }
            }
        });
    });
}

pub(super) fn duplicate_session_profile(
    state: &Arc<Mutex<AppState>>,
    session_id: Uuid,
) -> Result<String> {
    let mut app = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    let mut candidate = app.sessions.clone();
    let source = candidate
        .sessions
        .iter()
        .find(|profile| profile.id == session_id)
        .cloned()
        .context("session not found")?;
    let mut duplicate = source.clone();
    duplicate.id = Uuid::new_v4();
    duplicate.name = duplicate_session_name(&candidate, &source.name)?;
    if let Some(ssh) = duplicate.ssh_mut() {
        ssh.credential_storage = None;
        ssh.credential_vault_key_saved = false;
    }
    duplicate.validate()?;
    candidate.upsert(duplicate);
    app.config.save(&candidate)?;
    app.sessions = candidate;
    info!(source_session_id = %session_id, "session profile duplicated");
    Ok(format!("Session {} duplicated", source.name))
}

pub(super) fn export_session_profile(sessions: &SessionStore, session_id: &str) -> Result<String> {
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

pub(super) fn export_session_group(sessions: &SessionStore, group_name: &str) -> Result<String> {
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

pub(super) fn parse_session_transfer(text: &str) -> Result<SessionTransferEnvelope> {
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
    profile.group_name = normalize_group_name(&profile.group_name);
    if let Some(ssh) = profile.ssh_mut() {
        ssh.credential_storage = None;
        ssh.credential_vault_key_saved = false;
        ssh.host_key_fingerprint = None;
    }
}

fn normalized_transfer_group_name(group_name: &str) -> Result<String> {
    let group_name = normalize_group_name(group_name);
    let mut validation_store = SessionStore::default();
    validation_store.add_group(&group_name)?;
    Ok(group_name)
}

pub(super) fn duplicate_session_group(
    state: &Arc<Mutex<AppState>>,
    group_name: &str,
) -> Result<String> {
    let mut app = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    let mut candidate = app.sessions.clone();
    let source_group = normalized_transfer_group_name(group_name)?;
    if !candidate.groups.iter().any(|group| group == &source_group) {
        anyhow::bail!("group not found");
    }
    let source_profiles = candidate
        .sessions
        .iter()
        .filter(|profile| normalize_group_name(&profile.group_name) == source_group)
        .cloned()
        .collect::<Vec<_>>();
    if source_profiles.len() > MAX_SESSION_TRANSFER_PROFILES {
        anyhow::bail!("group exceeds the {MAX_SESSION_TRANSFER_PROFILES} server duplicate limit");
    }
    let duplicate_group = duplicate_group_name(&candidate, &source_group)?;
    candidate.add_group(&duplicate_group)?;
    for source in source_profiles {
        let mut duplicate = source.clone();
        duplicate.id = Uuid::new_v4();
        duplicate.name = duplicate_session_name(&candidate, &source.name)?;
        duplicate.group_name = duplicate_group.clone();
        if let Some(ssh) = duplicate.ssh_mut() {
            ssh.credential_storage = None;
            ssh.credential_vault_key_saved = false;
        }
        duplicate.validate()?;
        candidate.upsert(duplicate);
    }
    app.config.save(&candidate)?;
    app.sessions = candidate;
    info!("session group duplicated");
    Ok(format!(
        "Group {source_group} duplicated as {duplicate_group}"
    ))
}

fn import_session_transfer(
    state: &Arc<Mutex<AppState>>,
    text: &str,
    mode: SessionImportMode,
    target_group: &str,
) -> Result<String> {
    let mut app = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    let mut candidate = app.sessions.clone();
    let (profile_count, imported_group) =
        import_session_transfer_into_store(&mut candidate, text, mode, target_group)?;
    app.config.save(&candidate)?;
    app.sessions = candidate;
    info!(profile_count, "session configuration imported");
    if profile_count == 0 {
        Ok(format!(
            "Group {} imported",
            imported_group.context("import did not create a group")?
        ))
    } else if profile_count == 1 {
        Ok("Imported 1 server".to_owned())
    } else {
        Ok(format!("Imported {profile_count} servers"))
    }
}

pub(super) fn import_session_transfer_into_store(
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

fn duplicate_group_name(sessions: &SessionStore, name: &str) -> Result<String> {
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

pub(super) fn duplicate_session_name(sessions: &SessionStore, name: &str) -> Result<String> {
    let base = format!("{name} Copy");
    if !sessions.sessions.iter().any(|profile| profile.name == base) {
        return Ok(base);
    }
    let mut suffix = 2u32;
    loop {
        let candidate = format!("{base} {suffix}");
        if !sessions
            .sessions
            .iter()
            .any(|profile| profile.name == candidate)
        {
            return Ok(candidate);
        }
        suffix = suffix
            .checked_add(1)
            .context("too many duplicate session names")?;
    }
}

pub(super) fn update_session_group(
    state: &Arc<Mutex<AppState>>,
    action: &str,
    target: &str,
    value: &str,
) -> Result<String> {
    let mut app = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    let mut candidate = app.sessions.clone();
    let (changed, message) = match action {
        "new-group" => {
            let group_name = normalize_group_name(value);
            let changed = candidate.add_group(&group_name)?;
            (changed, format!("Group {group_name} created"))
        }
        "rename-group" => {
            let old_name = normalize_group_name(target);
            let new_name = normalize_group_name(value);
            let changed = candidate.rename_group(&old_name, &new_name)?;
            (changed, format!("Group renamed to {new_name}"))
        }
        "delete-group" => {
            let group_name = normalize_group_name(target);
            let changed = candidate.remove_group(&group_name);
            (changed, format!("Group {group_name} removed"))
        }
        _ => anyhow::bail!("unknown session action"),
    };
    if !changed {
        anyhow::bail!("group was not changed");
    }
    app.config.save(&candidate)?;
    app.sessions = candidate;
    Ok(message)
}

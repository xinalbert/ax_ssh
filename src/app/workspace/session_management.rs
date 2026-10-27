use super::*;
use crate::app::state::PersistenceCoordinator;
use ax_ssh::session_transfer::{
    MAX_SESSION_TRANSFER_BYTES, MAX_SESSION_TRANSFER_PROFILES, SessionImportMode,
    duplicate_group_name, export_session_group, export_session_profile,
    import_session_transfer_into_store, normalized_transfer_group_name,
};

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
            match duplicate_session_profile(&state, &persistence, id).await {
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
                            if !set_platform_clipboard_text(&ui, &text) {
                                return;
                            }
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
                    match duplicate_session_group(&state, &persistence, &target).await {
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
                let clipboard = match platform_clipboard_text(&ui) {
                    Ok(text) => text,
                    Err(error) => {
                        ui.set_status(format!("Cannot read clipboard: {error}").into());
                        return;
                    }
                };
                let mode = match action {
                    "import-server" => SessionImportMode::SingleServer,
                    "import-group" => SessionImportMode::IntoGroup,
                    _ => SessionImportMode::Automatic,
                };
                let ui = ui_for_transfer.clone();
                let state = state_for_transfer.clone();
                let persistence = persistence_for_transfer.clone();
                runtime_for_transfer.spawn(async move {
                    match import_session_transfer(&state, &persistence, &clipboard, mode, &target)
                        .await
                    {
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
        if action == "clear-saved-password" {
            set_status(&ui, "Clearing saved password...");
        }
        runtime.spawn(async move {
            let result = if action == "delete-session" {
                delete_session_profile(&state, &persistence, &target).await
            } else if action == "clear-saved-password" {
                clear_saved_password(&state, &persistence, &target).await
            } else {
                update_session_group(&state, &persistence, &action, &target, &value).await
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
                    if action == "clear-saved-password" {
                        set_status(&ui, &format!("Cannot clear saved password: {error}"));
                    } else {
                        set_status(&ui, &format!("Cannot update sessions: {error}"));
                    }
                }
            }
        });
    });
}

pub(super) async fn duplicate_session_profile(
    state: &Arc<Mutex<AppState>>,
    persistence: &PersistenceCoordinator,
    session_id: Uuid,
) -> Result<String> {
    let message = update_session_store(state, persistence, move |candidate| {
        let source = candidate
            .sessions
            .iter()
            .find(|profile| profile.id == session_id)
            .cloned()
            .context("session not found")?;
        let mut duplicate = source.clone();
        duplicate.id = Uuid::new_v4();
        duplicate.name = duplicate_session_name(candidate, &source.name)?;
        if let Some(ssh) = duplicate.ssh_mut() {
            ssh.credential_storage = None;
            ssh.credential_vault_key_saved = false;
        }
        duplicate.validate()?;
        candidate.upsert(duplicate);
        Ok(format!("Session {} duplicated", source.name))
    })
    .await?;
    info!(source_session_id = %session_id, "session profile duplicated");
    Ok(message)
}

pub(super) async fn duplicate_session_group(
    state: &Arc<Mutex<AppState>>,
    persistence: &PersistenceCoordinator,
    group_name: &str,
) -> Result<String> {
    let source_group = normalized_transfer_group_name(group_name)?;
    let message = update_session_store(state, persistence, move |candidate| {
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
            anyhow::bail!(
                "group exceeds the {MAX_SESSION_TRANSFER_PROFILES} server duplicate limit"
            );
        }
        let duplicate_group = duplicate_group_name(candidate, &source_group)?;
        candidate.add_group(&duplicate_group)?;
        for source in source_profiles {
            let mut duplicate = source.clone();
            duplicate.id = Uuid::new_v4();
            duplicate.name = duplicate_session_name(candidate, &source.name)?;
            duplicate.group_name = duplicate_group.clone();
            if let Some(ssh) = duplicate.ssh_mut() {
                ssh.credential_storage = None;
                ssh.credential_vault_key_saved = false;
            }
            duplicate.validate()?;
            candidate.upsert(duplicate);
        }
        Ok(format!(
            "Group {source_group} duplicated as {duplicate_group}"
        ))
    })
    .await?;
    info!("session group duplicated");
    Ok(message)
}

async fn import_session_transfer(
    state: &Arc<Mutex<AppState>>,
    persistence: &PersistenceCoordinator,
    text: &str,
    mode: SessionImportMode,
    target_group: &str,
) -> Result<String> {
    if text.len() > MAX_SESSION_TRANSFER_BYTES {
        anyhow::bail!("session import exceeds the {MAX_SESSION_TRANSFER_BYTES} byte limit");
    }
    let text = text.to_owned();
    let target_group = target_group.to_owned();
    let (message, profile_count) = update_session_store(state, persistence, move |candidate| {
        let (profile_count, imported_group) =
            import_session_transfer_into_store(candidate, &text, mode, &target_group)?;
        let message = if profile_count == 0 {
            format!(
                "Group {} imported",
                imported_group.context("import did not create a group")?
            )
        } else if profile_count == 1 {
            "Imported 1 server".to_owned()
        } else {
            format!("Imported {profile_count} servers")
        };
        Ok((message, profile_count))
    })
    .await?;
    info!(profile_count, "session configuration imported");
    Ok(message)
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

pub(super) async fn update_session_group(
    state: &Arc<Mutex<AppState>>,
    persistence: &PersistenceCoordinator,
    action: &str,
    target: &str,
    value: &str,
) -> Result<String> {
    let action = action.to_owned();
    let target = target.to_owned();
    let value = value.to_owned();
    update_session_store(state, persistence, move |candidate| {
        let (changed, message) = match action.as_str() {
            "new-group" => {
                let group_name = normalize_group_name(&value);
                let changed = candidate.add_group(&group_name)?;
                (changed, format!("Group {group_name} created"))
            }
            "rename-group" => {
                let old_name = normalize_group_name(&target);
                let new_name = normalize_group_name(&value);
                let changed = candidate.rename_group(&old_name, &new_name)?;
                (changed, format!("Group renamed to {new_name}"))
            }
            "delete-group" => {
                let group_name = normalize_group_name(&target);
                let changed = candidate.remove_group(&group_name);
                (changed, format!("Group {group_name} removed"))
            }
            _ => anyhow::bail!("unknown session action"),
        };
        if !changed {
            anyhow::bail!("group was not changed");
        }
        Ok(message)
    })
    .await
}

async fn update_session_store<T: Send + 'static>(
    state: &Arc<Mutex<AppState>>,
    persistence: &PersistenceCoordinator,
    update: impl FnOnce(&mut SessionStore) -> Result<T> + Send + 'static,
) -> Result<T> {
    update_session_store_with(state, persistence, update, ConfigStore::save).await
}

async fn update_session_store_with<T: Send + 'static>(
    state: &Arc<Mutex<AppState>>,
    persistence: &PersistenceCoordinator,
    update: impl FnOnce(&mut SessionStore) -> Result<T> + Send + 'static,
    save: impl FnOnce(&ConfigStore, &SessionStore) -> Result<()> + Send + 'static,
) -> Result<T> {
    let guard = persistence.gate.clone().lock_owned().await;
    let state = state.clone();
    tokio::task::spawn_blocking(move || {
        // Keep serialization even if the async caller is cancelled during fsync.
        let _guard = guard;
        let (config, groups, profiles) = {
            let app = state
                .lock()
                .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
            (
                app.config.clone(),
                app.sessions.groups.clone(),
                app.sessions.sessions.clone(),
            )
        };
        // Do not persist transient Settings previews as a side effect of CRUD.
        let mut candidate = config
            .load()
            .context("failed to reload committed session store")?;
        candidate.groups = groups;
        candidate.sessions = profiles;
        let result = update(&mut candidate)?;
        save(&config, &candidate)?;
        let mut app = state
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
        // Only these fields belong to this transaction. UI previews may change
        // while disk I/O is in progress; keep them and recent-workspace state.
        app.sessions.groups = candidate.groups;
        app.sessions.sessions = candidate.sessions;
        Ok(result)
    })
    .await
    .context("session management persistence task failed")?
}

#[cfg(test)]
mod tests;

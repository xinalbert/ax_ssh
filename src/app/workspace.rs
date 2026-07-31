use super::*;

pub(super) fn wire_workspace_tabs(ui: &AppWindow, state: Arc<Mutex<AppState>>, runtime: Handle) {
    let ui_for_settings = ui.as_weak();
    let state_for_settings = state.clone();
    ui.on_open_settings(move || {
        match state_for_settings.lock() {
            Ok(mut app) => {
                app.open_settings_tab();
            }
            Err(_) => {
                set_status(&ui_for_settings, "Cannot update workspace tabs");
                return;
            }
        }
        refresh_workspace(&ui_for_settings, &state_for_settings);
    });

    let ui_for_new = ui.as_weak();
    let state_for_new = state.clone();
    ui.on_new_session(move || {
        match state_for_new.lock() {
            Ok(mut app) => {
                app.open_session_editor_tab();
            }
            Err(_) => {
                set_status(&ui_for_new, "Cannot update workspace tabs");
                return;
            }
        }
        refresh_workspace(&ui_for_new, &state_for_new);
    });

    let ui_for_new_in_group = ui.as_weak();
    let state_for_new_in_group = state.clone();
    ui.on_new_session_in_group(move |group_name| {
        match state_for_new_in_group.lock() {
            Ok(mut app) => {
                app.open_session_editor_for_group(group_name.as_str());
            }
            Err(_) => {
                set_status(&ui_for_new_in_group, "Cannot update workspace tabs");
                return;
            }
        }
        refresh_workspace(&ui_for_new_in_group, &state_for_new_in_group);
    });

    let ui_for_edit = ui.as_weak();
    let state_for_edit = state.clone();
    ui.on_edit_session(move |id| {
        let id = match parse_uuid(id.as_str(), "session", &ui_for_edit) {
            Some(id) => id,
            None => return,
        };
        let opened = state_for_edit
            .lock()
            .is_ok_and(|mut app| app.open_session_editor_for_profile(id));
        if !opened {
            set_status(&ui_for_edit, "Session not found");
            return;
        }
        refresh_workspace(&ui_for_edit, &state_for_edit);
    });

    let ui_for_local = ui.as_weak();
    let state_for_local = state.clone();
    let runtime_for_local = runtime.clone();
    ui.on_open_local_shell(move || {
        if let Err(error) = start_local_shell(
            &runtime_for_local,
            state_for_local.clone(),
            ui_for_local.clone(),
        ) {
            set_status(&ui_for_local, &format!("Cannot open local shell: {error}"));
        }
    });

    let ui_for_activate = ui.as_weak();
    let state_for_activate = state.clone();
    ui.on_activate_tab(move |id| {
        let id = match parse_uuid(id.as_str(), "tab", &ui_for_activate) {
            Some(id) => id,
            None => return,
        };
        let activated = state_for_activate
            .lock()
            .is_ok_and(|mut app| app.activate_tab(id));
        if !activated {
            set_status(&ui_for_activate, "Tab not found");
            return;
        }
        refresh_workspace(&ui_for_activate, &state_for_activate);
    });

    let ui_for_move = ui.as_weak();
    let state_for_move = state.clone();
    ui.on_move_tab(move |id, target_index| {
        let id = match parse_uuid(id.as_str(), "tab", &ui_for_move) {
            Some(id) => id,
            None => return,
        };
        let moved = state_for_move
            .lock()
            .is_ok_and(|mut app| app.move_tab(id, target_index.max(0) as usize));
        if !moved {
            set_status(&ui_for_move, "Tab not found");
            return;
        }
        refresh_workspace(&ui_for_move, &state_for_move);
    });

    let ui_for_close = ui.as_weak();
    let state_for_close = state.clone();
    let runtime_for_close = runtime.clone();
    ui.on_close_tab(move |id| {
        let id = match parse_uuid(id.as_str(), "tab", &ui_for_close) {
            Some(id) => id,
            None => return,
        };
        close_workspace_tab(id, &state_for_close, &ui_for_close, &runtime_for_close);
    });

    let ui_for_cancel_editor = ui.as_weak();
    let state_for_cancel_editor = state;
    ui.on_cancel_session_dialog(move || {
        let active_id = state_for_cancel_editor
            .lock()
            .ok()
            .and_then(|app| app.active_tab_id());
        if let Some(active_id) = active_id {
            close_workspace_tab(
                active_id,
                &state_for_cancel_editor,
                &ui_for_cancel_editor,
                &runtime,
            );
        }
    });
}

pub(super) fn close_workspace_tab(
    tab_id: Uuid,
    state: &Arc<Mutex<AppState>>,
    ui: &slint::Weak<AppWindow>,
    runtime: &Handle,
) {
    let closed = match state.lock() {
        Ok(mut app) => app.close_tab(tab_id),
        Err(_) => {
            set_status(ui, "Cannot update workspace tabs");
            return;
        }
    };
    let Some(closed) = closed else {
        set_status(ui, "Tab not found");
        return;
    };
    if let Some(probe) = closed.pending_probe
        && probe.cancel.send(()).is_err()
    {
        debug!(tab_id = %tab_id, "host-key probe already stopped while closing tab");
    }
    if closed.dismissed_prompt {
        set_dialog_open(ui, Dialog::HostKey, false);
        set_dialog_open(ui, Dialog::Password, false);
    }
    if let Some(worker) = closed.worker {
        let ui = ui.clone();
        runtime.spawn(async move {
            if let Err(error) = worker.shutdown().await {
                warn!(tab_id = %tab_id, %error, "failed to shut down closed tab worker");
                set_status(
                    &ui,
                    &format!("Cannot close terminal worker cleanly: {error}"),
                );
            }
        });
    }
    refresh_workspace(ui, state);
}

pub(super) fn wire_session_editor(ui: &AppWindow, state: Arc<Mutex<AppState>>, runtime: Handle) {
    let ui_for_save = ui.as_weak();
    let state_for_save = state.clone();
    ui.on_save_session(
        move |name,
              group_name,
              host,
              port,
              username,
              auth_method,
              private_key_path,
              password,
              remember_password| {
            let parsed_port = match port.trim().parse::<u16>() {
                Ok(port) if port > 0 => port,
                _ => {
                    set_status(&ui_for_save, "Port must be a number between 1 and 65535");
                    return;
                }
            };
            let (editor_tab_id, existing_profile) = match state_for_save.lock() {
                Ok(app) => {
                    let Some(profile_id) = app.active_editor_profile_id() else {
                        set_status(&ui_for_save, "Session editor is not active");
                        return;
                    };
                    let existing_profile = profile_id.and_then(|profile_id| {
                        app.sessions
                            .sessions
                            .iter()
                            .find(|profile| profile.id == profile_id)
                            .cloned()
                    });
                    if profile_id.is_some() && existing_profile.is_none() {
                        set_status(&ui_for_save, "Session not found");
                        return;
                    }
                    (app.active_tab_id(), existing_profile)
                }
                Err(_) => {
                    set_status(&ui_for_save, "Cannot read session state");
                    return;
                }
            };
            let (profile, credential_change) = match profile_from_editor(
                existing_profile.as_ref(),
                name.as_str(),
                group_name.as_str(),
                host.as_str(),
                parsed_port,
                username.as_str(),
                auth_method.as_str(),
                private_key_path.as_str(),
                password.as_str(),
                remember_password,
            ) {
                Ok(result) => result,
                Err(error) => {
                    set_status(&ui_for_save, &format!("Cannot save session: {error}"));
                    return;
                }
            };
            let profile_id = profile.id;
            if let Err(error) = profile.validate() {
                set_status(&ui_for_save, &format!("Cannot save session: {error}"));
                return;
            }
            let state = state_for_save.clone();
            let ui = ui_for_save.clone();
            set_status(&ui_for_save, "Saving session...");
            runtime.spawn(async move {
                let credential_rollback = match apply_credential_change(
                    profile_id,
                    credential_change,
                )
                .await
                {
                    Ok(rollback) => rollback,
                    Err(error) => {
                        warn!(session_id = %profile_id, %error, "failed to update session credential");
                        set_status(&ui, &format!("Cannot update password: {error}"));
                        return;
                    }
                };

                let save_result = (|| -> Result<()> {
                    let mut app = state
                        .lock()
                        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
                    let mut candidate = app.sessions.clone();
                    candidate.upsert(profile.clone());
                    app.config.save(&candidate)?;
                    app.sessions = candidate;
                    app.expanded_groups.insert(profile.group_name.clone());
                    Ok(())
                })();

                if let Err(error) = save_result {
                    if let Some(rollback) = credential_rollback
                        && let Err(rollback_error) = rollback.restore().await
                    {
                        warn!(session_id = %profile_id, %rollback_error, "failed to restore credential after profile save failure");
                    }
                    set_status(&ui, &format!("Cannot save session: {error}"));
                    return;
                }

                info!(
                    session_id = %profile_id,
                    credential_stored = profile.credential_stored,
                    private_key = matches!(profile.auth, AuthMethod::PrivateKey { .. }),
                    "session profile saved"
                );
                refresh_session_models(&ui, &state);
                if let Some(editor_tab_id) = editor_tab_id {
                    let _ = state.lock().map(|mut app| app.close_tab(editor_tab_id));
                }
                refresh_workspace(&ui, &state);
                set_status(&ui, "Session saved");
            });
        },
    );

    let ui_for_group = ui.as_weak();
    ui.on_toggle_group(move |group_name| {
        let group_name = normalize_group_name(group_name.as_str());
        match state.lock() {
            Ok(mut app) => {
                if !app.expanded_groups.insert(group_name.clone()) {
                    app.expanded_groups.remove(&group_name);
                }
            }
            Err(_) => {
                set_status(&ui_for_group, "Cannot update group state");
                return;
            }
        }
        refresh_session_models(&ui_for_group, &state);
    });
}

pub(super) fn wire_session_management(
    ui: &AppWindow,
    state: Arc<Mutex<AppState>>,
    runtime: Handle,
) {
    let ui_for_action = ui.as_weak();
    ui.on_manage_session_action(move |action, target, value| {
        let action = action.as_str().to_owned();
        let target = target.as_str().to_owned();
        let value = value.as_str().to_owned();
        let ui = ui_for_action.clone();
        let state = state.clone();
        runtime.spawn(async move {
            let result = if action == "delete-session" {
                delete_session_profile(&state, &target).await
            } else {
                update_session_group(&state, &action, &target, &value)
            };
            match result {
                Ok(message) => {
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

fn update_session_group(
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
    match action {
        "new-group" => {
            app.expanded_groups.insert(normalize_group_name(value));
        }
        "rename-group" => {
            let was_expanded = app.expanded_groups.remove(&normalize_group_name(target));
            if was_expanded {
                app.expanded_groups.insert(normalize_group_name(value));
            }
        }
        "delete-group" => {
            app.expanded_groups.remove(&normalize_group_name(target));
            app.expanded_groups.insert(String::new());
        }
        _ => {}
    }
    Ok(message)
}

async fn delete_session_profile(state: &Arc<Mutex<AppState>>, session_id: &str) -> Result<String> {
    let session_id = Uuid::parse_str(session_id).context("invalid session id")?;
    let profile = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?
        .sessions
        .sessions
        .iter()
        .find(|profile| profile.id == session_id)
        .cloned()
        .context("session not found")?;
    let credential_rollback = apply_credential_change(
        session_id,
        if profile.credential_stored {
            CredentialChange::Delete
        } else {
            CredentialChange::None
        },
    )
    .await?;
    let save_result = (|| -> Result<()> {
        let mut app = state
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
        let mut candidate = app.sessions.clone();
        if !candidate.remove(session_id) {
            anyhow::bail!("session not found");
        }
        app.config.save(&candidate)?;
        app.sessions = candidate;
        if app.active_editor_profile_id() == Some(Some(session_id))
            && let Some(tab_id) = app.active_tab_id()
        {
            let _ = app.close_tab(tab_id);
        }
        Ok(())
    })();
    if let Err(error) = save_result {
        if let Some(rollback) = credential_rollback
            && let Err(rollback_error) = rollback.restore().await
        {
            warn!(session_id = %session_id, %rollback_error, "failed to restore credential after session delete failure");
        }
        return Err(error);
    }
    info!(session_id = %session_id, "session profile deleted");
    Ok(format!("Session {} deleted", profile.name))
}

enum CredentialChange {
    None,
    Store(String),
    Delete,
}

struct CredentialRollback {
    session_id: Uuid,
    previous_password: Option<String>,
}

impl CredentialRollback {
    async fn restore(self) -> Result<()> {
        if let Some(password) = self.previous_password {
            save_stored_password(self.session_id, password).await
        } else {
            delete_stored_password(self.session_id).await
        }
    }
}

async fn apply_credential_change(
    session_id: Uuid,
    change: CredentialChange,
) -> Result<Option<CredentialRollback>> {
    match change {
        CredentialChange::None => Ok(None),
        CredentialChange::Store(password) => {
            let previous_password = load_stored_password(session_id).await?;
            save_stored_password(session_id, password).await?;
            Ok(Some(CredentialRollback {
                session_id,
                previous_password,
            }))
        }
        CredentialChange::Delete => {
            let previous_password = load_stored_password(session_id).await?;
            delete_stored_password(session_id).await?;
            Ok(Some(CredentialRollback {
                session_id,
                previous_password,
            }))
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn profile_from_editor(
    existing: Option<&SessionProfile>,
    name: &str,
    group_name: &str,
    host: &str,
    port: u16,
    username: &str,
    auth_method: &str,
    private_key_path: &str,
    password: &str,
    remember_password: bool,
) -> Result<(SessionProfile, CredentialChange)> {
    let private_key = auth_method == "Private key";
    let previous_credential_stored = existing.is_some_and(|profile| profile.credential_stored);
    let credential_stored = if private_key {
        false
    } else if remember_password {
        if password.is_empty() && !previous_credential_stored {
            anyhow::bail!("enter a password before enabling password storage");
        }
        true
    } else {
        false
    };
    let credential_change = if private_key || !remember_password {
        if previous_credential_stored {
            CredentialChange::Delete
        } else {
            CredentialChange::None
        }
    } else if password.is_empty() {
        CredentialChange::None
    } else {
        CredentialChange::Store(password.to_owned())
    };

    let normalized_host = host.trim();
    let mut profile = SessionProfile::new(name.trim(), normalized_host, username.trim());
    if let Some(existing) = existing {
        profile.id = existing.id;
        profile.host_key_fingerprint = (existing.host.trim() == normalized_host
            && existing.port == port)
            .then(|| existing.host_key_fingerprint.clone())
            .flatten();
    }
    profile.group_name = normalize_group_name(group_name);
    profile.port = port;
    profile.auth = if private_key {
        AuthMethod::PrivateKey {
            path: PathBuf::from(private_key_path.trim()),
        }
    } else {
        AuthMethod::Password
    };
    profile.credential_stored = credential_stored;
    Ok((profile, credential_change))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editing_without_a_password_preserves_an_existing_credential() {
        let mut existing = SessionProfile::new("old", "old.example", "alice");
        existing.credential_stored = true;
        existing.host_key_fingerprint = Some("SHA256:trusted".into());

        let (profile, change) = profile_from_editor(
            Some(&existing),
            "new",
            "Production",
            "old.example",
            22,
            "alice",
            "Password",
            "",
            "",
            true,
        )
        .expect("profile should update");

        assert_eq!(profile.id, existing.id);
        assert!(profile.credential_stored);
        assert_eq!(profile.host_key_fingerprint, existing.host_key_fingerprint);
        assert!(matches!(change, CredentialChange::None));
    }

    #[test]
    fn endpoint_changes_clear_trust_and_disabling_storage_deletes_the_credential() {
        let mut existing = SessionProfile::new("old", "old.example", "alice");
        existing.credential_stored = true;
        existing.host_key_fingerprint = Some("SHA256:trusted".into());

        let (profile, change) = profile_from_editor(
            Some(&existing),
            "new",
            "",
            "new.example",
            2222,
            "alice",
            "Password",
            "",
            "",
            false,
        )
        .expect("profile should update");

        assert_eq!(profile.id, existing.id);
        assert!(!profile.credential_stored);
        assert_eq!(profile.host_key_fingerprint, None);
        assert!(matches!(change, CredentialChange::Delete));
    }

    #[test]
    fn group_management_persists_and_moves_profiles_to_ungrouped_on_delete() {
        let path = std::env::temp_dir().join(format!("ax-ssh-groups-{}.json", Uuid::new_v4()));
        let mut sessions = SessionStore::default();
        let mut profile = SessionProfile::new("server", "server.example", "alice");
        profile.group_name = "Production".into();
        sessions.upsert(profile.clone());
        let state = Arc::new(Mutex::new(AppState::new(ConfigStore::new(&path), sessions)));

        update_session_group(&state, "new-group", "", "Staging").expect("group should be added");
        update_session_group(&state, "rename-group", "Staging", "QA")
            .expect("group should be renamed");
        update_session_group(&state, "delete-group", "Production", "")
            .expect("group should be removed");

        let app = state.lock().expect("state should remain readable");
        assert_eq!(app.sessions.groups, ["QA"]);
        assert_eq!(app.sessions.sessions[0].group_name, "");
        assert_eq!(
            app.config.load().expect("saved state should load"),
            app.sessions
        );
        drop(app);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn deleting_a_profile_keeps_open_terminal_tabs() {
        let path = std::env::temp_dir().join(format!("ax-ssh-delete-{}.json", Uuid::new_v4()));
        let mut sessions = SessionStore::default();
        let profile = SessionProfile::new("server", "server.example", "alice");
        sessions.upsert(profile.clone());
        let mut app = AppState::new(ConfigStore::new(&path), sessions);
        let terminal_id = app.open_terminal_tab(&profile);
        let state = Arc::new(Mutex::new(app));

        delete_session_profile(&state, &profile.id.to_string())
            .await
            .expect("profile should be deleted");

        let app = state.lock().expect("state should remain readable");
        assert!(app.sessions.sessions.is_empty());
        assert!(app.terminal(terminal_id).is_some());
        drop(app);
        let _ = std::fs::remove_file(path);
    }
}

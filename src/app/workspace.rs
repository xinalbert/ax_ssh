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

    let ui_for_group = ui.as_weak();
    let state_for_group = state.clone();
    ui.on_activate_group(move |group_name| {
        let group_name = normalize_group_name(group_name.as_str());
        match state_for_group.lock() {
            Ok(mut app) => {
                app.expanded_groups.insert(group_name);
            }
            Err(_) => {
                set_status(&ui_for_group, "Cannot update group state");
                return;
            }
        }
        refresh_session_models(&ui_for_group, &state_for_group);
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
            let private_key = auth_method.as_str() == "Private key";
            if !private_key && remember_password && password.is_empty() {
                set_status(
                    &ui_for_save,
                    "Enter a password before enabling password storage",
                );
                return;
            }

            let mut profile = SessionProfile::new(name.as_str(), host.as_str(), username.as_str());
            profile = SessionProfile {
                group_name: normalize_group_name(group_name.as_str()),
                port: parsed_port,
                auth: if private_key {
                    AuthMethod::PrivateKey {
                        path: PathBuf::from(private_key_path.trim()),
                    }
                } else {
                    AuthMethod::Password
                },
                credential_stored: !private_key && remember_password,
                ..profile
            };
            let profile_id = profile.id;
            if let Err(error) = profile.validate() {
                set_status(&ui_for_save, &format!("Cannot save session: {error}"));
                return;
            }
            let editor_tab_id = state_for_save
                .lock()
                .ok()
                .and_then(|app| app.active_tab_id());
            let secret = password.as_str().to_owned();
            let state = state_for_save.clone();
            let ui = ui_for_save.clone();
            set_status(&ui_for_save, "Saving session...");
            runtime.spawn(async move {
                if !private_key
                    && remember_password
                    && let Err(error) = save_stored_password(profile_id, secret).await
                {
                    warn!(session_id = %profile_id, %error, "failed to save session credential");
                    set_status(&ui, &format!("Cannot save password: {error}"));
                    return;
                }

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
                    if !private_key
                        && remember_password
                        && let Err(cleanup_error) = delete_stored_password(profile_id).await
                    {
                        warn!(
                            session_id = %profile_id,
                            %cleanup_error,
                            "failed to roll back credential after profile save failure"
                        );
                    }
                    set_status(&ui, &format!("Cannot save session: {error}"));
                    return;
                }

                info!(
                    session_id = %profile_id,
                    credential_stored = !private_key && remember_password,
                    private_key,
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

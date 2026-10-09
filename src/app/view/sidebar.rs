use super::*;

pub(super) fn terminal_select_all_shortcut_for_platform(apple_platform: bool) -> &'static str {
    if apple_platform {
        "Cmd+A"
    } else {
        "Ctrl+Shift+A"
    }
}

pub(in crate::app) fn session_group_rows(
    sessions: &SessionStore,
    filter: &str,
    profile_releases: &HashMap<Uuid, String>,
) -> Vec<SessionGroupRow> {
    let filter = filter.trim().to_lowercase();
    session_groups(sessions)
        .into_iter()
        .filter_map(|group| {
            let group_name = group.name;
            let display_name = if group_name.is_empty() {
                "Ungrouped".to_owned()
            } else {
                group_name.clone()
            };
            let group_matches = !filter.is_empty() && display_name.to_lowercase().contains(&filter);
            let profiles = group
                .profiles
                .into_iter()
                .filter(|profile| {
                    filter.is_empty() || group_matches || {
                        profile.name.to_lowercase().contains(&filter)
                            || profile_sidebar_details(profile)
                                .to_lowercase()
                                .contains(&filter)
                            || profile_sidebar_endpoint(
                                profile,
                                &sessions.settings.workspace.session_mask_character,
                            )
                            .to_lowercase()
                            .contains(&filter)
                    }
                })
                .map(|profile| SessionProfileRow {
                    id: profile.id.to_string().into(),
                    name: profile.name.clone().into(),
                    details: profile_sidebar_details(profile).into(),
                    endpoint: profile_sidebar_endpoint(
                        profile,
                        &sessions.settings.workspace.session_mask_character,
                    )
                    .into(),
                    system_platform: profile
                        .system_icon_override
                        .as_deref()
                        .and_then(ax_ssh::config::system_icon_id)
                        .or_else(|| profile.cached_system().and_then(|system| system.icon_id()))
                        .or_else(|| {
                            profile_releases
                                .get(&profile.id)
                                .map(|release| system_platform(release))
                        })
                        .unwrap_or("")
                        .into(),
                    sftp_enabled: profile.ssh().is_some(),
                })
                .collect::<Vec<_>>();
            if !filter.is_empty() && !group_matches && profiles.is_empty() {
                return None;
            }
            Some(SessionGroupRow {
                group_name: group_name.into(),
                name: display_name.clone().into(),
                profiles: ModelRc::new(VecModel::from(profiles)),
            })
        })
        .collect()
}

pub(in crate::app) fn session_filter_changed(filter: &str, state: &Arc<Mutex<AppState>>) {
    let Ok(mut app) = state.lock() else {
        return;
    };
    app.session_filter = filter
        .chars()
        .filter(|character| !character.is_control())
        .take(128)
        .collect();
}

pub(in crate::app) fn connection_option_rows(
    sessions: &SessionStore,
) -> Vec<ConnectableSessionRow> {
    sessions
        .sessions
        .iter()
        .map(|profile| ConnectableSessionRow {
            id: profile.id.to_string().into(),
            name: profile.name.clone().into(),
            endpoint: profile_sidebar_endpoint(
                profile,
                &sessions.settings.workspace.session_mask_character,
            )
            .into(),
        })
        .collect()
}

pub(in crate::app) fn group_option_rows(sessions: &SessionStore) -> Vec<SharedString> {
    group_options(sessions)
        .into_iter()
        .map(SharedString::from)
        .collect()
}

pub(in crate::app) fn shell_option_rows(settings: &AppSettings) -> Vec<SharedString> {
    settings
        .terminal
        .known_shells
        .iter()
        .cloned()
        .map(SharedString::from)
        .collect()
}

pub(in crate::app) fn font_option_rows(
    selected: &str,
    system_families: &[String],
) -> Vec<SharedString> {
    font_options(selected, system_families)
        .into_iter()
        .map(SharedString::from)
        .collect()
}

pub(in crate::app) fn refresh_session_models(
    ui: &slint::Weak<AppWindow>,
    state: &Arc<Mutex<AppState>>,
) {
    // A connection may be monitored from a detached window, but navigation
    // belongs only to the main window.
    let ui = global_window_router()
        .and_then(|router| router.main_ui())
        .unwrap_or_else(|| ui.clone());
    let state = Arc::clone(state);
    dispatch_ui(&ui, move |ui| {
        let (rows, groups, options, filter) = match state.lock() {
            Ok(app) => (
                session_group_rows(
                    &app.sessions,
                    &app.session_filter,
                    &app.profile_system_releases(),
                ),
                group_option_rows(&app.sessions),
                connection_option_rows(&app.sessions),
                app.session_filter.clone(),
            ),
            Err(_) => {
                ui.set_status("State lock poisoned".into());
                return;
            }
        };
        let current = ui.get_sessions();
        if !update_session_group_model(&current, &rows) {
            ui.set_sessions(ModelRc::new(VecModel::from(rows)));
        }
        ui.set_session_filter(filter.into());
        ui.set_group_options(ModelRc::new(VecModel::from(groups)));
        ui.set_connection_options(ModelRc::new(VecModel::from(options)));
    });
}

pub(super) fn update_session_group_model(
    current: &ModelRc<SessionGroupRow>,
    rows: &[SessionGroupRow],
) -> bool {
    let Some(model) = current.as_any().downcast_ref::<VecModel<SessionGroupRow>>() else {
        return false;
    };
    if current.row_count() != rows.len()
        || current
            .iter()
            .zip(rows)
            .any(|(previous, next)| previous.group_name != next.group_name)
    {
        return false;
    }
    // Row notifications preserve the group's component and its local
    // disclosure state when only profile data (such as a system icon) changes.
    for (index, row) in rows.iter().enumerate() {
        if current.row_data(index).is_none_or(|previous| {
            previous.name != row.name || !previous.profiles.iter().eq(row.profiles.iter())
        }) {
            model.set_row_data(index, row.clone());
        }
    }
    true
}

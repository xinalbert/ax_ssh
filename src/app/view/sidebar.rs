use super::*;

pub(super) fn terminal_select_all_shortcut_for_platform(apple_platform: bool) -> &'static str {
    if apple_platform {
        "Cmd+A"
    } else {
        "Ctrl+Shift+A"
    }
}

pub(in crate::app) fn session_group_rows(sessions: &SessionStore) -> Vec<SessionGroupRow> {
    session_groups(sessions)
        .into_iter()
        .map(|group| {
            let group_name = group.name;
            let display_name = if group_name.is_empty() {
                "Ungrouped".to_owned()
            } else {
                group_name.clone()
            };
            let profiles = ModelRc::new(VecModel::from(
                group
                    .profiles
                    .into_iter()
                    .map(|profile| SessionProfileRow {
                        id: profile.id.to_string().into(),
                        name: profile.name.clone().into(),
                        details: profile_sidebar_details(profile).into(),
                        endpoint: profile_sidebar_endpoint(
                            profile,
                            &sessions.settings.workspace.session_mask_character,
                        )
                        .into(),
                        icon: compact_label(&profile.name, "--", 2).into(),
                        sftp_enabled: profile.ssh().is_some(),
                    })
                    .collect::<Vec<_>>(),
            ));
            SessionGroupRow {
                group_name: group_name.into(),
                name: display_name.clone().into(),
                icon: compact_label(
                    &display_name,
                    "Un",
                    usize::from(sessions.settings.workspace.collapsed_group_label_chars),
                )
                .into(),
                profiles,
            }
        })
        .collect()
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
    let state = Arc::clone(state);
    dispatch_ui(ui, move |ui| {
        let (rows, groups, options) = match state.lock() {
            Ok(app) => (
                session_group_rows(&app.sessions),
                group_option_rows(&app.sessions),
                connection_option_rows(&app.sessions),
            ),
            Err(_) => {
                ui.set_status("State lock poisoned".into());
                return;
            }
        };
        ui.set_sessions(ModelRc::new(VecModel::from(rows)));
        ui.set_group_options(ModelRc::new(VecModel::from(groups)));
        ui.set_connection_options(ModelRc::new(VecModel::from(options)));
    });
}

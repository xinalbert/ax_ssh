//! One action table for native menus, Linux menus and keyboard routing.
use super::i18n::{self, mark, tr};
use ax_ssh::config::AppSettings;
use ax_ssh::controller::WorkspaceView;
use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Command {
    NewProfile,
    ImportSessions,
    ExportSelected,
    NewConnection,
    NewLocal,
    WorkspaceFile,
    SaveWorkspace,
    Copy,
    Paste,
    Settings,
    ToggleSidebar,
    OpenSftp,
    SplitRight,
    SplitDown,
    PreviousTab,
    NextTab,
    CloseTab,
    About,
    Quit,
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(super) mod native;

pub(super) const GROUPS: &[(&str, &[(Command, &str)])] = &[
    (
        mark!("File"),
        &[
            (Command::NewProfile, mark!("New Server…")),
            (Command::ImportSessions, mark!("Import from Clipboard")),
            (
                Command::ExportSelected,
                mark!("Export Selected to Clipboard"),
            ),
            (Command::NewConnection, mark!("New Connection")),
            (Command::NewLocal, mark!("New Local Shell")),
            (Command::WorkspaceFile, mark!("Open / Save Workspace…")),
            (Command::SaveWorkspace, mark!("Save Workspace")),
        ],
    ),
    (
        mark!("Edit"),
        &[
            (Command::Copy, mark!("Copy")),
            (Command::Paste, mark!("Paste")),
            (Command::Settings, mark!("Settings…")),
        ],
    ),
    (
        mark!("View"),
        &[
            (Command::ToggleSidebar, mark!("Toggle Session Sidebar")),
            (Command::OpenSftp, mark!("SFTP")),
        ],
    ),
    (
        mark!("Pane"),
        &[
            (Command::SplitRight, mark!("Split Columns")),
            (Command::SplitDown, mark!("Split Rows")),
        ],
    ),
    (
        mark!("Window"),
        &[
            (Command::PreviousTab, mark!("Previous Tab")),
            (Command::NextTab, mark!("Next Tab")),
            (Command::CloseTab, mark!("Close Current Tab")),
        ],
    ),
    (mark!("Help"), &[(Command::About, mark!("About AxSSH"))]),
];

impl Command {
    pub(super) fn shortcut(self, settings: &AppSettings) -> Option<String> {
        let primary = if cfg!(target_os = "macos") {
            "Cmd"
        } else {
            "Ctrl"
        };
        Some(match self {
            Self::NewProfile => settings.shortcuts.new_session.clone(),
            Self::ImportSessions => settings.shortcuts.import_sessions.clone(),
            Self::ExportSelected => settings.shortcuts.export_selected.clone(),
            Self::Settings => settings.shortcuts.open_settings.clone(),
            Self::ToggleSidebar => settings.shortcuts.toggle_sidebar.clone(),
            Self::Copy => settings.shortcuts.copy_selection.clone(),
            Self::Paste => settings.shortcuts.paste.clone(),
            Self::OpenSftp => settings.shortcuts.open_sftp.clone(),
            Self::SplitRight => format!("{primary}+D"),
            Self::SplitDown => format!("{primary}+Shift+D"),
            Self::PreviousTab => "Ctrl+Shift+Tab".into(),
            Self::NextTab => "Ctrl+Tab".into(),
            Self::CloseTab => format!("{primary}+W"),
            Self::Quit => format!("{primary}+Q"),
            _ => return None,
        })
    }
    pub(super) fn enabled_for_view(self, view: &WorkspaceView, blocked: bool) -> bool {
        let terminal = view
            .tabs
            .iter()
            .any(|tab| Some(tab.id) == view.active_tab_id && !tab.settings);
        self.enabled(view.ready, blocked, terminal, view.tabs.len())
            && (self != Self::OpenSftp || sftp_target(view).is_some())
            && (self != Self::ExportSelected
                || !view.profiles.is_empty()
                || !view.groups.is_empty())
    }

    fn enabled(self, ready: bool, blocked: bool, terminal: bool, tabs: usize) -> bool {
        if self == Self::Quit {
            return !blocked;
        }
        ready
            && !blocked
            && match self {
                Self::SplitRight | Self::SplitDown => terminal,
                Self::CloseTab => tabs > 0,
                Self::PreviousTab | Self::NextTab => tabs > 1,
                _ => true,
            }
    }
}

/// Only saved SSH profiles can connect the existing SFTP companion.
pub(super) fn sftp_target(view: &WorkspaceView) -> Option<uuid::Uuid> {
    let tab = view
        .tabs
        .iter()
        .find(|tab| Some(tab.id) == view.active_tab_id && !tab.settings)?;
    let focused = tab.layout.panes.iter().find(|pane| pane.focused)?.tab_id;
    tab.panes
        .iter()
        .find(|pane| {
            pane.id == focused
                && pane.sftp
                && !pane.local
                && pane.profile.as_ref().is_some_and(|p| p.ssh().is_some())
        })
        .map(|pane| pane.id)
}

pub(super) fn parse_shortcut(text: &str) -> anyhow::Result<KeyboardShortcut> {
    let (mods, key) = text
        .rsplit_once('+')
        .ok_or_else(|| anyhow::anyhow!("shortcut needs a modifier"))?;
    let mut modifiers = Modifiers::NONE;
    for part in mods.split('+') {
        match part {
            "Ctrl" => modifiers.ctrl = true,
            "Shift" => modifiers.shift = true,
            "Alt" => modifiers.alt = true,
            "Cmd" | "Meta" => modifiers.command = true,
            _ => anyhow::bail!("unknown shortcut modifier"),
        }
    }
    let key =
        Key::from_name(key).ok_or_else(|| anyhow::anyhow!("unsupported shortcut key: {key}"))?;
    Ok(KeyboardShortcut::new(modifiers, key))
}

pub(super) fn keyboard(
    ui: &mut egui::Ui,
    settings: &AppSettings,
    native: bool,
    enabled: impl Fn(Command) -> bool,
) -> Option<Command> {
    // AppKit owns accelerators while eframe owns the Windows message loop.
    // egui-winit already converts the OS clipboard chords to Copy/Paste events;
    // Ctrl+Shift+C/V still arrive as keys and use RequestCopy/RequestPaste.
    let mut commands: Vec<_> = GROUPS
        .iter()
        .flat_map(|(_, items)| items.iter().map(|(cmd, _)| *cmd))
        .chain([Command::Quit])
        .collect();
    // Consume more-specific combinations before their base key.
    commands.sort_by_key(|cmd| {
        std::cmp::Reverse(
            cmd.shortcut(settings)
                .map_or(0, |text| text.matches('+').count()),
        )
    });
    commands.into_iter().find(|cmd| {
        !(cfg!(target_os = "macos") && native)
            && enabled(*cmd)
            && cmd
                .shortcut(settings)
                .and_then(|text| parse_shortcut(&text).ok())
                .is_some_and(|shortcut| ui.input_mut(|input| {
                    let mut consumed = false;
                    input.events.retain(|event| {
                        let matched = matches!(event, egui::Event::Key { key, pressed: true, repeat: false, modifiers, .. } if *key == shortcut.logical_key && modifiers.matches_exact(shortcut.modifiers));
                        consumed |= matched;
                        !matched
                    });
                    consumed
                }))
    })
}

pub(super) fn bar(
    ui: &mut egui::Ui,
    settings: &AppSettings,
    enabled: impl Fn(Command) -> bool,
) -> Option<Command> {
    let mut result = None;
    egui::MenuBar::new().ui(ui, |ui| {
        for (title, items) in GROUPS {
            ui.menu_button(i18n::current(ui.ctx()).text("", title), |ui| {
                for (cmd, label) in *items {
                    let button = egui::Button::new(i18n::current(ui.ctx()).text("", label))
                        .shortcut_text(cmd.shortcut(settings).unwrap_or_default());
                    if ui.add_enabled(enabled(*cmd), button).clicked() {
                        result = Some(*cmd);
                        ui.close();
                    }
                }
                if *title == "File"
                    && ui
                        .add_enabled(
                            enabled(Command::Quit),
                            egui::Button::new(tr!("Quit", ui.ctx())),
                        )
                        .clicked()
                {
                    result = Some(Command::Quit);
                    ui.close();
                }
            });
        }
    });
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saved_transfer_chords_share_export_availability_and_modal_gates() {
        let mut settings = AppSettings::default();
        settings.shortcuts.import_sessions = "Ctrl+Alt+I".into();
        settings.shortcuts.export_selected = "Ctrl+Alt+E".into();
        let mut view = WorkspaceView {
            ready: true,
            busy: false,
            status: String::new(),
            error: None,
            catalog_revision: 0,
            settings: std::sync::Arc::new(settings),
            profiles: std::sync::Arc::new(vec![]),
            groups: std::sync::Arc::new(vec![]),
            tabs: vec![],
            active_tab_id: None,
        };
        assert!(!Command::ExportSelected.enabled_for_view(&view, false));
        assert!(Command::ImportSessions.enabled_for_view(&view, false));
        view.groups = std::sync::Arc::new(vec!["Empty group".into()]);
        for command in [Command::ImportSessions, Command::ExportSelected] {
            for blocked in [true, false] {
                let chord = parse_shortcut(&command.shortcut(&view.settings).unwrap()).unwrap();
                egui::Context::default()
                    .run_ui(
                        egui::RawInput {
                            events: vec![egui::Event::Key {
                                key: chord.logical_key,
                                physical_key: None,
                                pressed: true,
                                repeat: false,
                                modifiers: chord.modifiers,
                            }],
                            ..Default::default()
                        },
                        |ui| {
                            assert_eq!(
                                keyboard(ui, &view.settings, false, |cmd| cmd
                                    .enabled_for_view(&view, blocked)),
                                (!blocked).then_some(command)
                            );
                        },
                    )
                    .drop_without_applying_deltas();
            }
        }
    }
    #[test]
    fn shortcuts_parse_and_modal_gates_cover_every_menu_action() {
        let settings = AppSettings::default();
        for cmd in GROUPS
            .iter()
            .flat_map(|(_, items)| items.iter().map(|(cmd, _)| *cmd))
            .chain([Command::Quit])
        {
            if let Some(text) = cmd.shortcut(&settings) {
                assert!(parse_shortcut(&text).is_ok(), "{text}");
            }
            assert!(!cmd.enabled(true, true, true, 2));
        }
        assert!(!Command::SplitRight.enabled(true, false, false, 2));
        assert!(!Command::NextTab.enabled(true, false, true, 1));
    }
}

#[cfg(test)]
mod routing_tests {
    use super::*;
    #[test]
    fn sftp_chord_is_consumed_once_only_when_enabled_and_never_on_repeat() {
        let settings = AppSettings::default();
        let chord = parse_shortcut(&settings.shortcuts.open_sftp).unwrap();
        for (enabled, repeat, expected) in [
            (true, false, Some(Command::OpenSftp)),
            (false, false, None),
            (true, true, None),
        ] {
            let ctx = egui::Context::default();
            if repeat {
                ctx.run_ui(
                    egui::RawInput {
                        events: vec![egui::Event::Key {
                            key: chord.logical_key,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: chord.modifiers,
                        }],
                        ..Default::default()
                    },
                    |_| {},
                )
                .drop_without_applying_deltas();
            }
            ctx.run_ui(
                egui::RawInput {
                    events: vec![egui::Event::Key {
                        key: chord.logical_key,
                        physical_key: None,
                        pressed: true,
                        repeat,
                        modifiers: chord.modifiers,
                    }],
                    ..Default::default()
                },
                |ui| {
                    assert_eq!(
                        keyboard(ui, &settings, false, |cmd| cmd != Command::OpenSftp
                            || enabled),
                        expected
                    );
                    assert_eq!(
                        ui.input(|input| input
                            .events
                            .iter()
                            .any(|event| matches!(event, egui::Event::Key { key: Key::M, .. }))),
                        expected.is_none()
                    );
                    assert_eq!(
                        keyboard(ui, &settings, false, |_| true),
                        if enabled {
                            None
                        } else {
                            Some(Command::OpenSftp)
                        }
                    );
                },
            )
            .drop_without_applying_deltas();
        }
    }

    #[test]
    fn exact_chords_ignore_repeats_and_do_not_steal_alt_combinations() {
        let settings = AppSettings::default();
        let ctx = egui::Context::default();
        let mut modifiers = parse_shortcut(&Command::SplitDown.shortcut(&settings).unwrap())
            .unwrap()
            .modifiers;
        let event = |modifiers, repeat| egui::Event::Key {
            key: Key::D,
            physical_key: None,
            pressed: true,
            repeat,
            modifiers,
        };
        for (events, expected) in [
            (vec![event(modifiers, false)], Some(Command::SplitDown)),
            (vec![event(modifiers, true)], None),
            (
                {
                    modifiers.alt = true;
                    vec![event(modifiers, false)]
                },
                None,
            ),
        ] {
            let output = ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    assert_eq!(keyboard(ui, &settings, false, |_| true), expected);
                    assert_eq!(keyboard(ui, &settings, false, |_| true), None);
                },
            );
            output.drop_without_applying_deltas();
        }
    }
}

//! One action table for native menus, Linux menus and keyboard routing.
use ax_ssh::config::AppSettings;
use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Command {
    NewProfile,
    NewConnection,
    NewLocal,
    WorkspaceFile,
    SaveWorkspace,
    Copy,
    Paste,
    Settings,
    ToggleSidebar,
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
        "File",
        &[
            (Command::NewProfile, "New Server…"),
            (Command::NewConnection, "New Connection"),
            (Command::NewLocal, "New Local Shell"),
            (Command::WorkspaceFile, "Open / Save Workspace…"),
            (Command::SaveWorkspace, "Save Workspace"),
        ],
    ),
    (
        "Edit",
        &[
            (Command::Copy, "Copy"),
            (Command::Paste, "Paste"),
            (Command::Settings, "Settings…"),
        ],
    ),
    (
        "View",
        &[(Command::ToggleSidebar, "Toggle Session Sidebar")],
    ),
    (
        "Pane",
        &[
            (Command::SplitRight, "Split Columns"),
            (Command::SplitDown, "Split Rows"),
        ],
    ),
    (
        "Window",
        &[
            (Command::PreviousTab, "Previous Tab"),
            (Command::NextTab, "Next Tab"),
            (Command::CloseTab, "Close Current Tab"),
        ],
    ),
    ("Help", &[(Command::About, "About AxSSH")]),
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
            Self::Settings => settings.shortcuts.open_settings.clone(),
            Self::ToggleSidebar => settings.shortcuts.toggle_sidebar.clone(),
            Self::Copy => settings.shortcuts.copy_selection.clone(),
            Self::Paste => settings.shortcuts.paste.clone(),
            Self::SplitRight => format!("{primary}+D"),
            Self::SplitDown => format!("{primary}+Shift+D"),
            Self::PreviousTab => "Ctrl+Shift+Tab".into(),
            Self::NextTab => "Ctrl+Tab".into(),
            Self::CloseTab => format!("{primary}+W"),
            Self::Quit => format!("{primary}+Q"),
            _ => return None,
        })
    }
    pub(super) fn enabled(self, ready: bool, blocked: bool, terminal: bool, tabs: usize) -> bool {
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
            ui.menu_button(*title, |ui| {
                for (cmd, label) in *items {
                    let button = egui::Button::new(*label)
                        .shortcut_text(cmd.shortcut(settings).unwrap_or_default());
                    if ui.add_enabled(enabled(*cmd), button).clicked() {
                        result = Some(*cmd);
                        ui.close();
                    }
                }
                if *title == "File"
                    && ui
                        .add_enabled(enabled(Command::Quit), egui::Button::new("Quit"))
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

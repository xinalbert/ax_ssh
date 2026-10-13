//! Persistent shortcuts settings and normalization.

use super::*;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ShortcutSettings {
    #[serde(default = "default_open_settings_shortcut")]
    pub open_settings: String,
    #[serde(default = "default_new_session_shortcut")]
    pub new_session: String,
    #[serde(default = "default_import_sessions_shortcut")]
    pub import_sessions: String,
    #[serde(default = "default_export_selected_shortcut")]
    pub export_selected: String,
    #[serde(default = "default_toggle_sidebar_shortcut")]
    pub toggle_sidebar: String,
    #[serde(default = "default_copy_selection_shortcut")]
    pub copy_selection: String,
    #[serde(default = "default_paste_shortcut")]
    pub paste: String,
    #[serde(default = "default_open_sftp_shortcut")]
    pub open_sftp: String,
}

impl ShortcutSettings {
    pub fn normalized(input: Self) -> Self {
        let candidate = Self {
            open_settings: input.open_settings.trim().to_owned(),
            new_session: input.new_session.trim().to_owned(),
            import_sessions: input.import_sessions.trim().to_owned(),
            export_selected: input.export_selected.trim().to_owned(),
            toggle_sidebar: input.toggle_sidebar.trim().to_owned(),
            copy_selection: input.copy_selection.trim().to_owned(),
            paste: input.paste.trim().to_owned(),
            open_sftp: input.open_sftp.trim().to_owned(),
        };
        if candidate.validate().is_ok() {
            candidate
        } else {
            Self::default()
        }
    }

    pub fn validate(&self) -> Result<()> {
        let shortcuts = [
            ("open settings", self.open_settings.as_str()),
            ("new server", self.new_session.as_str()),
            ("import sessions", self.import_sessions.as_str()),
            ("export selected", self.export_selected.as_str()),
            ("toggle sidebar", self.toggle_sidebar.as_str()),
            ("copy selection", self.copy_selection.as_str()),
            ("paste", self.paste.as_str()),
            ("open SFTP", self.open_sftp.as_str()),
        ];
        for (label, shortcut) in shortcuts {
            validate_shortcut(label, shortcut)?;
        }
        for (index, (label, shortcut)) in shortcuts.iter().enumerate() {
            if let Some((other_label, _)) = shortcuts[index + 1..]
                .iter()
                .find(|(_, other)| other.eq_ignore_ascii_case(shortcut))
            {
                anyhow::bail!("{label} and {other_label} cannot use the same shortcut");
            }
        }
        Ok(())
    }
}

impl Default for ShortcutSettings {
    fn default() -> Self {
        Self {
            open_settings: default_open_settings_shortcut(),
            new_session: default_new_session_shortcut(),
            import_sessions: default_import_sessions_shortcut(),
            export_selected: default_export_selected_shortcut(),
            toggle_sidebar: default_toggle_sidebar_shortcut(),
            copy_selection: default_copy_selection_shortcut(),
            paste: default_paste_shortcut(),
            open_sftp: default_open_sftp_shortcut(),
        }
    }
}

fn validate_shortcut(label: &str, shortcut: &str) -> Result<()> {
    let shortcut = shortcut.trim();
    if shortcut.is_empty() {
        anyhow::bail!("{label} shortcut cannot be empty");
    }
    if shortcut.chars().count() > MAX_SHORTCUT_CHARS || shortcut.chars().any(char::is_control) {
        anyhow::bail!("{label} shortcut is invalid");
    }
    let Some((modifiers, key)) = shortcut.rsplit_once('+') else {
        anyhow::bail!("{label} shortcut must include a modifier");
    };
    if key.is_empty() || key.chars().any(char::is_whitespace) {
        anyhow::bail!("{label} shortcut key is invalid");
    }
    let mut seen = Vec::new();
    for modifier in modifiers.split('+') {
        if !matches!(modifier, "Cmd" | "Meta" | "Ctrl" | "Alt" | "Shift") {
            anyhow::bail!("{label} shortcut contains an unknown modifier");
        }
        if seen.contains(&modifier) {
            anyhow::bail!("{label} shortcut contains a repeated modifier");
        }
        seen.push(modifier);
    }
    Ok(())
}

fn default_open_settings_shortcut() -> String {
    if cfg!(target_os = "macos") {
        "Cmd+,".to_owned()
    } else {
        "Ctrl+,".to_owned()
    }
}

fn default_new_session_shortcut() -> String {
    if cfg!(target_os = "macos") {
        "Cmd+N".to_owned()
    } else {
        "Ctrl+N".to_owned()
    }
}

fn default_import_sessions_shortcut() -> String {
    if cfg!(target_os = "macos") {
        "Cmd+Shift+I".to_owned()
    } else {
        "Ctrl+Shift+I".to_owned()
    }
}

fn default_export_selected_shortcut() -> String {
    if cfg!(target_os = "macos") {
        "Cmd+Shift+E".to_owned()
    } else {
        "Ctrl+Shift+E".to_owned()
    }
}

pub(in crate::config) fn default_toggle_sidebar_shortcut() -> String {
    if cfg!(target_os = "macos") {
        "Cmd+S".to_owned()
    } else {
        "Ctrl+S".to_owned()
    }
}

pub(in crate::config) fn previous_toggle_sidebar_shortcut() -> String {
    if cfg!(target_os = "macos") {
        "Cmd+B".to_owned()
    } else {
        "Ctrl+Shift+B".to_owned()
    }
}

pub(in crate::config) fn default_copy_selection_shortcut() -> String {
    if cfg!(target_os = "macos") {
        "Cmd+C".to_owned()
    } else {
        "Ctrl+Shift+C".to_owned()
    }
}

pub(in crate::config) fn default_paste_shortcut() -> String {
    if cfg!(target_os = "macos") {
        "Cmd+V".to_owned()
    } else {
        "Ctrl+Shift+V".to_owned()
    }
}

fn default_open_sftp_shortcut() -> String {
    "Ctrl+M".to_owned()
}

//! Persistent terminal settings and normalization.

use super::*;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct TerminalSettings {
    #[serde(default = "default_scrollback_lines")]
    pub scrollback_lines: u32,
    #[serde(default = "default_terminal_columns")]
    pub default_columns: u16,
    #[serde(default = "default_terminal_rows")]
    pub default_rows: u16,
    #[serde(default = "default_local_shell")]
    pub local_shell: String,
    #[serde(default = "default_known_shells")]
    pub known_shells: Vec<String>,
    /// Whether macOS Option-modified keys should be encoded as terminal Meta.
    #[serde(default)]
    pub option_as_meta: bool,
    /// Request line start by sending Home after each alternate-scroll Up/Down key.
    #[serde(default)]
    pub alternate_scroll_to_home: bool,
    /// Whether remote OSC 52 access to the default clipboard is enabled.
    #[serde(default)]
    pub osc52_clipboard: bool,
}

/// Raw terminal values supplied by an application settings surface.
#[derive(Clone, Copy, Debug)]
pub struct TerminalSettingsInput<'a> {
    pub scrollback_lines: i32,
    pub default_columns: i32,
    pub default_rows: i32,
    pub local_shell: &'a str,
    pub known_shells: &'a [String],
    pub option_as_meta: bool,
    pub alternate_scroll_to_home: bool,
    pub osc52_clipboard: bool,
}

impl TerminalSettings {
    pub fn normalized(input: TerminalSettingsInput<'_>) -> Self {
        let (local_shell, known_shells) =
            normalize_shell_settings(input.local_shell, input.known_shells);
        Self {
            scrollback_lines: input
                .scrollback_lines
                .clamp(MIN_SCROLLBACK_LINES as i32, MAX_SCROLLBACK_LINES as i32)
                as u32,
            default_columns: input
                .default_columns
                .clamp(MIN_TERMINAL_COLUMNS.into(), MAX_TERMINAL_COLUMNS.into())
                as u16,
            default_rows: input
                .default_rows
                .clamp(MIN_TERMINAL_ROWS.into(), MAX_TERMINAL_ROWS.into())
                as u16,
            local_shell,
            known_shells,
            option_as_meta: input.option_as_meta,
            alternate_scroll_to_home: input.alternate_scroll_to_home,
            osc52_clipboard: input.osc52_clipboard,
        }
    }

    pub(super) fn normalize_in_place(&mut self) {
        *self = Self::normalized(TerminalSettingsInput {
            scrollback_lines: i32::try_from(self.scrollback_lines).unwrap_or(i32::MAX),
            default_columns: i32::from(self.default_columns),
            default_rows: i32::from(self.default_rows),
            local_shell: &self.local_shell,
            known_shells: &self.known_shells,
            option_as_meta: self.option_as_meta,
            alternate_scroll_to_home: self.alternate_scroll_to_home,
            osc52_clipboard: self.osc52_clipboard,
        });
    }

    pub fn merge_known_shells(&mut self, discovered: impl IntoIterator<Item = String>) -> bool {
        let previous = self.known_shells.clone();
        let mut merged = previous.clone();
        merged.extend(discovered);
        let (local_shell, known_shells) = normalize_shell_settings(&self.local_shell, &merged);
        self.local_shell = local_shell;
        self.known_shells = known_shells;
        self.known_shells != previous
    }
}

impl Default for TerminalSettings {
    fn default() -> Self {
        Self {
            scrollback_lines: default_scrollback_lines(),
            default_columns: default_terminal_columns(),
            default_rows: default_terminal_rows(),
            local_shell: default_local_shell(),
            known_shells: default_known_shells(),
            option_as_meta: false,
            alternate_scroll_to_home: false,
            osc52_clipboard: false,
        }
    }
}

fn normalize_shell_settings(local_shell: &str, known_shells: &[String]) -> (String, Vec<String>) {
    let local_shell =
        normalize_shell_name(local_shell).unwrap_or_else(|| SYSTEM_DEFAULT_SHELL.to_owned());
    let mut normalized = Vec::with_capacity(known_shells.len().min(MAX_KNOWN_SHELLS) + 2);
    normalized.push(SYSTEM_DEFAULT_SHELL.to_owned());
    for shell in known_shells {
        let Some(shell) = normalize_shell_name(shell) else {
            continue;
        };
        if normalized
            .iter()
            .any(|known| known.eq_ignore_ascii_case(&shell))
        {
            continue;
        }
        normalized.push(shell);
        if normalized.len() >= MAX_KNOWN_SHELLS {
            break;
        }
    }
    if !normalized
        .iter()
        .any(|known| known.eq_ignore_ascii_case(&local_shell))
        && normalized.len() < MAX_KNOWN_SHELLS
    {
        normalized.push(local_shell.clone());
    }
    (local_shell, normalized)
}

fn normalize_shell_name(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()
        && value.chars().count() <= MAX_SHELL_NAME_CHARS
        && !value.chars().any(char::is_control))
    .then(|| value.to_owned())
}

const fn default_scrollback_lines() -> u32 {
    DEFAULT_SCROLLBACK_LINES
}

const fn default_terminal_columns() -> u16 {
    DEFAULT_TERMINAL_COLUMNS
}

const fn default_terminal_rows() -> u16 {
    DEFAULT_TERMINAL_ROWS
}

fn default_local_shell() -> String {
    SYSTEM_DEFAULT_SHELL.to_owned()
}

fn default_known_shells() -> Vec<String> {
    vec![default_local_shell()]
}

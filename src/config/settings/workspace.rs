//! Persistent workspace settings and normalization.

use super::*;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct WorkspaceSettings {
    #[serde(default = "default_sidebar_width")]
    pub sidebar_width: u16,
    #[serde(default = "default_tab_width")]
    pub tab_width: u16,
    #[serde(default = "default_session_mask_character")]
    pub session_mask_character: String,
    /// `0` keeps the entire group name in the widened collapsed sidebar.
    #[serde(default = "default_collapsed_group_label_chars")]
    pub collapsed_group_label_chars: u8,
}

/// Raw workspace values supplied by an application settings surface.
#[derive(Clone, Copy, Debug)]
pub struct WorkspaceSettingsInput<'a> {
    pub sidebar_width: i32,
    pub tab_width: i32,
    pub session_mask_character: &'a str,
    pub collapsed_group_label_chars: i32,
}

impl WorkspaceSettings {
    pub fn normalized(input: WorkspaceSettingsInput<'_>) -> Self {
        Self {
            sidebar_width: input
                .sidebar_width
                .clamp(MIN_SIDEBAR_WIDTH.into(), MAX_SIDEBAR_WIDTH.into())
                as u16,
            tab_width: input
                .tab_width
                .clamp(MIN_TAB_WIDTH.into(), MAX_TAB_WIDTH.into()) as u16,
            session_mask_character: normalize_session_mask_character(input.session_mask_character),
            collapsed_group_label_chars: input.collapsed_group_label_chars.clamp(
                i32::from(MIN_COLLAPSED_GROUP_LABEL_CHARS),
                i32::from(MAX_COLLAPSED_GROUP_LABEL_CHARS),
            ) as u8,
        }
    }

    pub(super) fn normalize_in_place(&mut self) {
        *self = Self::normalized(WorkspaceSettingsInput {
            sidebar_width: i32::from(self.sidebar_width),
            tab_width: i32::from(self.tab_width),
            session_mask_character: &self.session_mask_character,
            collapsed_group_label_chars: i32::from(self.collapsed_group_label_chars),
        });
    }
}

impl Default for WorkspaceSettings {
    fn default() -> Self {
        Self {
            sidebar_width: default_sidebar_width(),
            tab_width: default_tab_width(),
            session_mask_character: default_session_mask_character(),
            collapsed_group_label_chars: default_collapsed_group_label_chars(),
        }
    }
}

const fn default_sidebar_width() -> u16 {
    DEFAULT_SIDEBAR_WIDTH
}

const fn default_tab_width() -> u16 {
    DEFAULT_TAB_WIDTH
}

fn default_session_mask_character() -> String {
    DEFAULT_SESSION_MASK_CHARACTER.to_owned()
}

const fn default_collapsed_group_label_chars() -> u8 {
    DEFAULT_COLLAPSED_GROUP_LABEL_CHARS
}

fn normalize_session_mask_character(value: &str) -> String {
    let value = value.trim();
    if value.chars().count() == 1
        && !value
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        value.to_owned()
    } else {
        default_session_mask_character()
    }
}

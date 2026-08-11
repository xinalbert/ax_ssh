use std::collections::{HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use slint::Model;

use super::file_icons::{FileIconKey, clear_global_cache, global_provider, prewarm_async};
use super::local_files::LocalDirectoryEntry;
use super::state::{SftpTransferPhase, SftpTransferSnapshot};
use super::*;

const ICON_PREWARM_PENDING_KEY_LIMIT: usize = 256;
const ICON_PREWARM_BATCH_KEY_LIMIT: usize = 64;
static PRIVATE_KEY_OPTION_GENERATION: AtomicU64 = AtomicU64::new(0);
static COALESCED_WORKSPACE_REFRESHES: AtomicU64 = AtomicU64::new(0);

mod options;
mod settings;
mod sftp;
mod sidebar;
mod terminal;
mod workspace;

use self::options::*;
use self::settings::*;
use self::sftp::*;
use self::sidebar::*;
use self::terminal::*;
use self::workspace::*;

pub(super) use self::options::{
    clear_private_key_option_model, clear_session_editor_option_models,
    clear_settings_option_models, dispatch_ui, dispatch_ui_result, load_font_options,
    load_local_shell_options, load_private_key_options, load_x11_server_installations, parse_uuid,
    set_status,
};
pub(super) use self::settings::{
    apply_rendered_terminal, apply_settings_to_component, empty_terminal_snapshot,
    terminal_render_line, to_slint_color,
};
pub(super) use self::sftp::{
    clear_file_icon_cache, local_icon_keys, prewarm_file_icons, sftp_icon_keys,
};
pub(super) use self::sidebar::{
    connection_option_rows, font_option_rows, group_option_rows, refresh_session_models,
    session_group_rows, shell_option_rows,
};
pub(super) use self::terminal::{
    apply_active_snapshot, apply_terminal_pane_layout, dispatch_active_snapshot,
    dispatch_terminal_output_snapshot, set_tab_status,
};
pub(super) use self::workspace::refresh_workspace;

#[cfg(test)]
use self::workspace::visible_workspace_tab_rows;

#[cfg(test)]
mod tests;

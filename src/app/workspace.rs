use super::*;

mod session_editor;
mod session_management;
mod tabs;

pub(super) use self::session_editor::{
    ProfileMutationCoordinator, SessionEditorContext, wire_session_editor,
};
pub(super) use self::session_management::wire_session_management;
pub(super) use self::tabs::{close_terminal_child_pane, close_workspace_tab, wire_workspace_tabs};

use self::session_editor::delete_session_profile;
use self::tabs::clear_session_editor_resources;

#[cfg(test)]
use self::session_editor::{
    CredentialChange, begin_profile_mutation, commit_profile_delete, commit_profile_save,
    profile_from_editor, profile_from_editor_with_password, should_connect_after_save,
};
#[cfg(test)]
use self::session_management::{
    MAX_SESSION_TRANSFER_BYTES, SESSION_TRANSFER_FORMAT, SESSION_TRANSFER_VERSION,
    SessionImportMode, SessionTransferEnvelope, SessionTransferKind, duplicate_session_group,
    duplicate_session_name, duplicate_session_profile, export_session_group,
    export_session_profile, import_session_transfer_into_store, parse_session_transfer,
    update_session_group,
};

#[cfg(test)]
mod tests;

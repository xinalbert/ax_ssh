use super::*;
use ax_ssh::config::{ConfigStore, SessionProfile, SessionStore};
#[test]
fn external_edit_disconnect_reconnect_and_reset_invalidate_old_copy_context() {
    let root = std::env::temp_dir().join(format!("axssh-edit-context-{}", Uuid::new_v4()));
    let mut app = AppState::new(
        ConfigStore::new(root.join("sessions.json")),
        SessionStore::default(),
    );
    let profile = SessionProfile::new("Remote", "remote.example", "alice");
    let tab = app.open_sftp_tab(&profile);
    let terminal = app.terminal_mut(tab).unwrap();
    let attempt = Uuid::new_v4();
    assert!(terminal.set_ssh_attempt(Some(attempt)));
    terminal.connected = true;
    let (id, _, _) = terminal
        .sftp
        .external_edits
        .open("/srv/file.txt".into())
        .unwrap();
    assert!(connected_edit(terminal, attempt, id).is_some());
    assert!(connected_edit(terminal, Uuid::new_v4(), id).is_none());
    terminal.connected = false;
    assert!(connected_edit(terminal, attempt, id).is_none());
    terminal.connected = true;
    terminal.sftp.reset_navigation();
    assert!(connected_edit(terminal, attempt, id).is_none());
}

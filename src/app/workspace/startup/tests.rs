use super::*;
use uuid::Uuid;

#[test]
fn startup_uses_latest_checkpoint_instead_of_stale_recent_template() {
    let root = std::env::temp_dir().join(format!("axssh-startup-{}", Uuid::new_v4()));
    let config = ConfigStore::new(root.join("sessions.json"));
    let template = root.join("template.json");
    let old = ax_ssh::config::WorkspaceSnapshot {
        version: ax_ssh::config::WORKSPACE_SNAPSHOT_VERSION,
        ..Default::default()
    };
    ConfigStore::save_workspace_file(&template, &old).expect("template");
    let mut latest = old.clone();
    latest
        .windows
        .push(ax_ssh::config::WorkspaceWindowSnapshot {
            placement: Some(ax_ssh::config::WindowPlacement {
                width: 900,
                height: 600,
                position: None,
                maximized: true,
            }),
            ..Default::default()
        });
    config.save_workspace(&latest).expect("checkpoint");
    let mut sessions = SessionStore::default();
    sessions
        .record_workspace_path(&template)
        .expect("recent template");
    assert_eq!(load_startup_workspace(&config, &mut sessions), Some(latest));
    assert_eq!(
        ConfigStore::load_workspace_file(&template).expect("unchanged template"),
        old
    );
    std::fs::write(config.workspace_path(), b"invalid JSON").expect("corrupt checkpoint");
    assert_eq!(load_startup_workspace(&config, &mut sessions), Some(old));
    std::fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn startup_workspace_skips_missing_recent_path_and_restores_next_available() {
    let root = std::env::temp_dir().join(format!("ax-ssh-startup-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("create startup workspace test directory");
    let config = ConfigStore::new(root.join("sessions.json"));
    let valid_path = root.join("valid-workspace.json");
    let missing_path = root.join("missing-workspace.json");
    let expected = ax_ssh::config::WorkspaceSnapshot {
        version: ax_ssh::config::WORKSPACE_SNAPSHOT_VERSION,
        ..Default::default()
    };

    ConfigStore::save_workspace_file(&valid_path, &expected)
        .expect("save startup workspace test snapshot");
    let mut sessions = SessionStore::default();
    sessions
        .record_workspace_path(&valid_path)
        .expect("record valid workspace path");
    sessions
        .record_workspace_path(&missing_path)
        .expect("record missing workspace path");

    let restored = load_startup_workspace(&config, &mut sessions)
        .expect("restore the next available recent workspace");

    assert_eq!(restored, expected);
    assert_eq!(
        sessions.recent_workspace_paths(),
        &[valid_path.to_string_lossy().into_owned()]
    );
    assert_eq!(
        config
            .load()
            .expect("load cleaned session store")
            .recent_workspaces,
        sessions.recent_workspaces
    );
    std::fs::remove_dir_all(root).expect("remove startup workspace test directory");
}

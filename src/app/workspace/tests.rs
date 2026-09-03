use super::*;

#[test]
fn save_and_connect_is_limited_to_ssh_profiles() {
    let ssh = SessionProfile::new("server", "server.example", "alice");
    let telnet = SessionProfile::new_telnet("console", "console.example");

    assert!(should_connect_after_save(true, &ssh));
    assert!(!should_connect_after_save(false, &ssh));
    assert!(!should_connect_after_save(true, &telnet));
}

#[test]
fn editing_without_a_password_preserves_an_existing_credential() {
    let mut existing = SessionProfile::new("old", "old.example", "alice");
    let ssh = existing.ssh_mut().expect("profile should be SSH");
    ssh.credential_storage = Some(CredentialStorage::EncryptedVault);
    ssh.host_key_fingerprint = Some("SHA256:trusted".into());
    ssh.x11_forwarding = true;

    let (profile, change) = profile_from_editor(
        Some(&existing),
        "new",
        "Production",
        "SSH",
        "old.example",
        "22",
        "alice",
        "Password",
        "",
        true,
        "",
        "115200",
        "8",
        "1",
        "none",
        "none",
        None,
    )
    .expect("profile should update");

    assert_eq!(profile.id, existing.id);
    assert_eq!(
        profile
            .ssh()
            .expect("updated profile should be SSH")
            .credential_storage,
        Some(CredentialStorage::EncryptedVault)
    );
    assert_eq!(
        profile
            .ssh()
            .expect("updated profile should be SSH")
            .host_key_fingerprint,
        existing
            .ssh()
            .expect("existing profile should be SSH")
            .host_key_fingerprint
    );
    assert!(
        profile
            .ssh()
            .expect("updated profile should be SSH")
            .x11_forwarding
    );
    assert!(matches!(change, CredentialChange::None));
}

#[test]
fn remembering_a_password_updates_the_existing_credential_backend() {
    let mut existing = SessionProfile::new("old", "old.example", "alice");
    let ssh = existing.ssh_mut().expect("profile should be SSH");
    ssh.credential_storage = Some(CredentialStorage::SystemKeyring);

    let (profile, change, connection_password) = profile_from_editor_with_password(
        Some(&existing),
        "updated",
        "",
        "SSH",
        "old.example",
        "22",
        "alice",
        "Password",
        "",
        "~",
        "",
        "new-password",
        true,
        "system-keyring",
        "",
        true,
        "",
        "115200",
        "8",
        "1",
        "none",
        "none",
        None,
    )
    .expect("profile should update");

    assert_eq!(
        profile.ssh().and_then(|ssh| ssh.credential_storage),
        Some(CredentialStorage::SystemKeyring)
    );
    assert_eq!(
        connection_password
            .as_ref()
            .expect("entered password should be available for immediate connection")
            .as_str(),
        "new-password"
    );
    match change {
        CredentialChange::Store {
            storage,
            previous_storage,
            password,
            vault_password,
        } => {
            assert_eq!(storage, CredentialStorage::SystemKeyring);
            assert_eq!(previous_storage, Some(CredentialStorage::SystemKeyring));
            assert_eq!(password.as_str(), "new-password");
            assert!(vault_password.is_none());
        }
        CredentialChange::None | CredentialChange::Delete(_) => {
            panic!("entered password should be stored")
        }
    }
}

#[test]
fn new_password_is_one_time_by_default() {
    let (profile, change, connection_password) = profile_from_editor_with_password(
        None,
        "new",
        "",
        "SSH",
        "new.example",
        "22",
        "alice",
        "Password",
        "",
        "~",
        "",
        "new-password",
        false,
        "encrypted-vault",
        "",
        true,
        "",
        "115200",
        "8",
        "1",
        "none",
        "none",
        None,
    )
    .expect("one-time password should not require a vault password");

    assert_eq!(profile.ssh().and_then(|ssh| ssh.credential_storage), None);
    assert!(matches!(change, CredentialChange::None));
    assert_eq!(
        connection_password
            .expect("one-time password should be returned for immediate connection")
            .as_str(),
        "new-password"
    );
}

#[test]
fn session_editor_saves_trimmed_sftp_default_paths() {
    let (profile, _, _) = profile_from_editor_with_password(
        None,
        "files",
        "",
        "SSH",
        "files.example",
        "22",
        "alice",
        "Password",
        "",
        "  /srv/releases  ",
        "  /Users/alice/Downloads  ",
        "",
        false,
        "system-keyring",
        "",
        true,
        "",
        "115200",
        "8",
        "1",
        "none",
        "none",
        None,
    )
    .expect("profile should save SFTP default paths");

    let ssh = profile.ssh().expect("profile should use SSH");
    assert_eq!(ssh.sftp_remote_path, "/srv/releases");
    assert_eq!(ssh.sftp_local_path, "/Users/alice/Downloads");
}

#[test]
fn remembering_new_password_without_vault_password_is_rejected() {
    let result = profile_from_editor_with_password(
        None,
        "new",
        "",
        "SSH",
        "new.example",
        "22",
        "alice",
        "Password",
        "",
        "~",
        "",
        "new-password",
        true,
        "encrypted-vault",
        "",
        true,
        "",
        "115200",
        "8",
        "1",
        "none",
        "none",
        None,
    );
    let error = match result {
        Err(error) => error,
        Ok(_) => panic!("encrypted vault saves require a vault password"),
    };
    assert_eq!(
        error.to_string(),
        "vault password is required for encrypted application vault"
    );

    let (profile, change, connection_password) = profile_from_editor_with_password(
        None,
        "new",
        "",
        "SSH",
        "new.example",
        "22",
        "alice",
        "Password",
        "",
        "~",
        "",
        "new-password",
        true,
        "encrypted-vault",
        "vault-password",
        true,
        "",
        "115200",
        "8",
        "1",
        "none",
        "none",
        None,
    )
    .expect("profile should save with a vault password");
    assert_eq!(
        profile.ssh().and_then(|ssh| ssh.credential_storage),
        Some(CredentialStorage::EncryptedVault)
    );
    match change {
        CredentialChange::Store {
            storage,
            vault_password,
            ..
        } => {
            assert_eq!(storage, CredentialStorage::EncryptedVault);
            assert!(vault_password.is_some());
        }
        CredentialChange::None | CredentialChange::Delete(_) => {
            panic!("vault password should retain encrypted-vault storage")
        }
    }
    assert_eq!(
        connection_password
            .expect("remembered password should also be used for immediate connection")
            .as_str(),
        "new-password"
    );
}

#[test]
fn one_time_password_preserves_an_existing_remembered_credential() {
    let mut existing = SessionProfile::new("old", "old.example", "alice");
    existing
        .ssh_mut()
        .expect("profile should be SSH")
        .credential_storage = Some(CredentialStorage::SystemKeyring);

    let (profile, change, connection_password) = profile_from_editor_with_password(
        Some(&existing),
        "updated",
        "",
        "SSH",
        "old.example",
        "22",
        "alice",
        "Password",
        "",
        "~",
        "",
        "temporary-password",
        false,
        "encrypted-vault",
        "",
        true,
        "",
        "115200",
        "8",
        "1",
        "none",
        "none",
        None,
    )
    .expect("one-time password should update the connection draft");

    assert_eq!(
        profile.ssh().and_then(|ssh| ssh.credential_storage),
        Some(CredentialStorage::SystemKeyring)
    );
    assert!(matches!(change, CredentialChange::None));
    assert_eq!(
        connection_password
            .expect("one-time password should be available for connection")
            .as_str(),
        "temporary-password"
    );
}

#[test]
fn non_ssh_editor_protocols_cannot_retain_x11_state() {
    let (telnet, _) = profile_from_editor(
        None,
        "console",
        "",
        "Telnet",
        "router.example",
        "23",
        "",
        "Password",
        "",
        true,
        "",
        "115200",
        "8",
        "1",
        "none",
        "none",
        None,
    )
    .expect("Telnet profile should be created");
    let encoded = serde_json::to_string(&telnet).expect("Telnet profile should serialize");
    assert!(!encoded.contains("x11_forwarding"));
}

#[test]
fn switching_to_private_key_preserves_trust_and_deletes_the_credential() {
    let mut existing = SessionProfile::new("old", "old.example", "alice");
    let ssh = existing.ssh_mut().expect("profile should be SSH");
    ssh.credential_storage = Some(CredentialStorage::SystemKeyring);
    ssh.host_key_fingerprint = Some("SHA256:trusted".into());

    let (profile, change) = profile_from_editor(
        Some(&existing),
        "new",
        "",
        "SSH",
        "old.example",
        "22",
        "alice",
        "Private key",
        "/tmp/id_ed25519",
        false,
        "",
        "115200",
        "8",
        "1",
        "none",
        "none",
        None,
    )
    .expect("profile should update");

    assert_eq!(profile.id, existing.id);
    assert_eq!(
        profile
            .ssh()
            .expect("updated profile should be SSH")
            .credential_storage,
        None
    );
    assert_eq!(
        profile
            .ssh()
            .expect("updated profile should be SSH")
            .host_key_fingerprint,
        existing
            .ssh()
            .expect("existing profile should be SSH")
            .host_key_fingerprint
    );
    assert!(matches!(
        change,
        CredentialChange::Delete(CredentialStorage::SystemKeyring)
    ));
}

#[test]
fn switching_to_ssh_agent_discards_password_input_and_credential_reference() {
    let mut existing = SessionProfile::new("old", "old.example", "alice");
    let ssh = existing.ssh_mut().expect("profile should be SSH");
    ssh.credential_storage = Some(CredentialStorage::SystemKeyring);
    ssh.host_key_fingerprint = Some("SHA256:trusted".into());

    let (profile, change, connection_password) = profile_from_editor_with_password(
        Some(&existing),
        "agent",
        "",
        "SSH",
        "old.example",
        "22",
        "alice",
        "SSH agent",
        "/must/not/be/used",
        "~",
        "",
        "must-not-be-used",
        true,
        "system-keyring",
        "must-not-be-used",
        false,
        "",
        "115200",
        "8",
        "1",
        "none",
        "none",
        None,
    )
    .expect("agent profile should update");

    let ssh = profile.ssh().expect("updated profile should be SSH");
    assert_eq!(ssh.auth, AuthMethod::SshAgent);
    assert_eq!(ssh.credential_storage, None);
    assert_eq!(ssh.host_key_fingerprint.as_deref(), Some("SHA256:trusted"));
    assert!(connection_password.is_none());
    assert!(matches!(
        change,
        CredentialChange::Delete(CredentialStorage::SystemKeyring)
    ));
}

#[test]
fn group_management_persists_and_moves_profiles_to_ungrouped_on_delete() {
    let path = std::env::temp_dir().join(format!("ax-ssh-groups-{}.json", Uuid::new_v4()));
    let mut sessions = SessionStore::default();
    let mut profile = SessionProfile::new("server", "server.example", "alice");
    profile.group_name = "Production".into();
    sessions.upsert(profile.clone());
    let state = Arc::new(Mutex::new(AppState::new(ConfigStore::new(&path), sessions)));

    update_session_group(&state, "new-group", "", "Staging").expect("group should be added");
    update_session_group(&state, "rename-group", "Staging", "QA").expect("group should be renamed");
    update_session_group(&state, "delete-group", "Production", "")
        .expect("group should be removed");

    let app = state.lock().expect("state should remain readable");
    assert_eq!(app.sessions.groups, ["QA"]);
    assert_eq!(app.sessions.sessions[0].group_name, "");
    assert_eq!(
        app.config.load().expect("saved state should load"),
        app.sessions
    );
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn duplicate_session_name_gets_a_unique_copy_suffix() {
    let mut sessions = SessionStore::default();
    sessions.upsert(SessionProfile::new("server Copy", "copy.example", "alice"));
    sessions.upsert(SessionProfile::new(
        "server Copy 2",
        "copy2.example",
        "alice",
    ));

    assert_eq!(
        duplicate_session_name(&sessions, "server").unwrap(),
        "server Copy 3"
    );
}

#[test]
fn duplicating_a_profile_keeps_connection_trust_without_reusing_credentials() {
    let path = std::env::temp_dir().join(format!("ax-ssh-duplicate-{}.json", Uuid::new_v4()));
    let mut sessions = SessionStore::default();
    let mut source = SessionProfile::new("production", "server.example", "alice");
    source.group_name = "Production".into();
    let source_ssh = source.ssh_mut().expect("profile should be SSH");
    source_ssh.credential_storage = Some(CredentialStorage::SystemKeyring);
    source_ssh.host_key_fingerprint = Some("SHA256:trusted".into());
    source_ssh.x11_forwarding = true;
    sessions.upsert(source.clone());
    let state = Arc::new(Mutex::new(AppState::new(ConfigStore::new(&path), sessions)));

    duplicate_session_profile(&state, source.id).expect("profile should duplicate");

    let app = state.lock().expect("state should remain readable");
    assert_eq!(app.sessions.sessions.len(), 2);
    let duplicate = app
        .sessions
        .sessions
        .iter()
        .find(|profile| profile.id != source.id)
        .expect("duplicate should be present");
    assert_eq!(duplicate.name, "production Copy");
    assert_eq!(duplicate.group_name, source.group_name);
    let duplicate_ssh = duplicate.ssh().expect("duplicate should be SSH");
    let source_ssh = source.ssh().expect("source should be SSH");
    assert_eq!(duplicate_ssh.host, source_ssh.host);
    assert_eq!(duplicate_ssh.port, source_ssh.port);
    assert_eq!(duplicate_ssh.username, source_ssh.username);
    assert_eq!(duplicate_ssh.auth, source_ssh.auth);
    assert!(duplicate_ssh.x11_forwarding);
    assert_eq!(duplicate_ssh.credential_storage, None);
    assert_eq!(
        duplicate_ssh.host_key_fingerprint.as_deref(),
        Some("SHA256:trusted")
    );
    assert_eq!(
        app.config.load().expect("saved state should load"),
        app.sessions
    );
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn server_export_removes_identity_credentials_and_host_trust() {
    let mut sessions = SessionStore::default();
    let mut source = SessionProfile::new("production", "server.example", "alice");
    source.group_name = "Production".into();
    let source_ssh = source.ssh_mut().expect("profile should be SSH");
    source_ssh.credential_storage = Some(CredentialStorage::SystemKeyring);
    source_ssh.host_key_fingerprint = Some("SHA256:trusted".into());
    source_ssh.x11_forwarding = true;
    sessions.upsert(source.clone());

    let text =
        export_session_profile(&sessions, &source.id.to_string()).expect("profile should export");
    let exported: SessionTransferEnvelope =
        serde_json::from_str(&text).expect("export should be valid JSON");

    assert_eq!(exported.format, SESSION_TRANSFER_FORMAT);
    assert_eq!(exported.version, SESSION_TRANSFER_VERSION);
    assert_eq!(exported.kind, SessionTransferKind::Server);
    assert_eq!(exported.profiles.len(), 1);
    let profile = &exported.profiles[0];
    assert_eq!(profile.id, Uuid::nil());
    assert_eq!(profile.group_name, "Production");
    let ssh = profile.ssh().expect("exported profile should be SSH");
    assert_eq!(ssh.credential_storage, None);
    assert_eq!(ssh.host_key_fingerprint, None);
    assert!(ssh.x11_forwarding);
}

#[test]
fn group_export_round_trips_profiles_and_empty_groups_without_security_state() {
    let mut sessions = SessionStore::default();
    sessions
        .add_group("Production")
        .expect("group should be added");
    sessions.add_group("Empty").expect("group should be added");
    let mut source = SessionProfile::new("server", "server.example", "alice");
    source.group_name = "Production".into();
    let source_id = source.id;
    let source_ssh = source.ssh_mut().expect("profile should be SSH");
    source_ssh.credential_storage = Some(CredentialStorage::SystemKeyring);
    source_ssh.host_key_fingerprint = Some("SHA256:trusted".into());
    source_ssh.x11_forwarding = true;
    sessions.upsert(source);

    let text =
        export_session_group(&sessions, "Production").expect("populated group should export");
    let exported: SessionTransferEnvelope =
        serde_json::from_str(&text).expect("export should be valid JSON");
    assert_eq!(exported.kind, SessionTransferKind::Group);
    assert_eq!(exported.group_name.as_deref(), Some("Production"));
    assert_eq!(exported.profiles.len(), 1);
    let exported_profile = &exported.profiles[0];
    assert_eq!(exported_profile.id, Uuid::nil());
    let exported_ssh = exported_profile
        .ssh()
        .expect("exported profile should be SSH");
    assert_eq!(exported_ssh.credential_storage, None);
    assert_eq!(exported_ssh.host_key_fingerprint, None);
    assert!(exported_ssh.x11_forwarding);

    let mut imported = SessionStore::default();
    let (count, imported_group) =
        import_session_transfer_into_store(&mut imported, &text, SessionImportMode::Automatic, "")
            .expect("group export should import");
    assert_eq!(count, 1);
    assert_eq!(imported_group.as_deref(), Some("Production"));
    let imported_profile = imported
        .sessions
        .first()
        .expect("imported profile should be present");
    assert_ne!(imported_profile.id, source_id);
    assert_ne!(imported_profile.id, Uuid::nil());
    assert_eq!(imported_profile.group_name, "Production");
    let imported_ssh = imported_profile
        .ssh()
        .expect("imported profile should be SSH");
    assert_eq!(imported_ssh.credential_storage, None);
    assert_eq!(imported_ssh.host_key_fingerprint, None);
    assert!(imported_ssh.x11_forwarding);

    let empty_text = export_session_group(&sessions, "Empty").expect("empty group should export");
    let empty_export: SessionTransferEnvelope =
        serde_json::from_str(&empty_text).expect("empty export should be valid JSON");
    assert_eq!(empty_export.group_name.as_deref(), Some("Empty"));
    assert!(empty_export.profiles.is_empty());
}

#[test]
fn importing_a_server_rekeys_redacts_and_resolves_name_conflicts() {
    let mut sessions = SessionStore::default();
    sessions.upsert(SessionProfile::new(
        "production",
        "existing.example",
        "alice",
    ));
    let mut source = SessionProfile::new("production", "server.example", "bob");
    source.group_name = "Production".into();
    let source_id = source.id;
    let source_ssh = source.ssh_mut().expect("profile should be SSH");
    source_ssh.credential_storage = Some(CredentialStorage::EncryptedVault);
    source_ssh.host_key_fingerprint = Some("SHA256:trusted".into());
    let text = serde_json::to_string(&SessionTransferEnvelope {
        format: SESSION_TRANSFER_FORMAT.into(),
        version: SESSION_TRANSFER_VERSION,
        kind: SessionTransferKind::Server,
        group_name: None,
        profiles: vec![source],
    })
    .expect("fixture should serialize");

    let (count, imported_group) =
        import_session_transfer_into_store(&mut sessions, &text, SessionImportMode::Automatic, "")
            .expect("profile should import");

    assert_eq!(count, 1);
    assert_eq!(imported_group, None);
    assert!(sessions.groups.iter().any(|group| group == "Production"));
    let imported = sessions
        .sessions
        .iter()
        .find(|profile| profile.name == "production Copy")
        .expect("renamed import should be present");
    assert_ne!(imported.id, source_id);
    assert_ne!(imported.id, Uuid::nil());
    assert_eq!(imported.group_name, "Production");
    let ssh = imported.ssh().expect("imported profile should be SSH");
    assert_eq!(ssh.credential_storage, None);
    assert_eq!(ssh.host_key_fingerprint, None);
}

#[test]
fn duplicating_a_group_rekeys_profiles_and_keeps_empty_groups_supported() {
    let path = std::env::temp_dir().join(format!("ax-ssh-group-copy-{}.json", Uuid::new_v4()));
    let mut sessions = SessionStore::default();
    sessions
        .add_group("Production")
        .expect("group should be added");
    sessions.add_group("Empty").expect("group should be added");
    let mut source = SessionProfile::new("server", "server.example", "alice");
    source.group_name = "Production".into();
    let source_ssh = source.ssh_mut().expect("profile should be SSH");
    source_ssh.credential_storage = Some(CredentialStorage::SystemKeyring);
    source_ssh.host_key_fingerprint = Some("SHA256:trusted".into());
    sessions.upsert(source.clone());
    let state = Arc::new(Mutex::new(AppState::new(ConfigStore::new(&path), sessions)));

    duplicate_session_group(&state, "Production").expect("group should duplicate");
    duplicate_session_group(&state, "Empty").expect("empty group should duplicate");

    let app = state.lock().expect("state should remain readable");
    assert!(
        app.sessions
            .groups
            .iter()
            .any(|group| group == "Production Copy")
    );
    assert!(
        app.sessions
            .groups
            .iter()
            .any(|group| group == "Empty Copy")
    );
    let duplicate = app
        .sessions
        .sessions
        .iter()
        .find(|profile| profile.group_name == "Production Copy")
        .expect("duplicated group profile should be present");
    assert_ne!(duplicate.id, source.id);
    assert_eq!(duplicate.name, "server Copy");
    let ssh = duplicate.ssh().expect("duplicated profile should be SSH");
    assert_eq!(ssh.credential_storage, None);
    assert_eq!(ssh.host_key_fingerprint.as_deref(), Some("SHA256:trusted"));
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn session_import_rejects_oversized_clipboard_data() {
    let oversized = "x".repeat(MAX_SESSION_TRANSFER_BYTES + 1);
    assert!(parse_session_transfer(&oversized).is_err());
}

#[test]
fn automatic_group_import_normalizes_conflicts_and_preserves_empty_groups() {
    let mut sessions = SessionStore::default();
    sessions
        .add_group("Production")
        .expect("existing group should be added");
    let text = serde_json::to_string(&SessionTransferEnvelope {
        format: SESSION_TRANSFER_FORMAT.into(),
        version: SESSION_TRANSFER_VERSION,
        kind: SessionTransferKind::Group,
        group_name: Some(" Production ".into()),
        profiles: Vec::new(),
    })
    .expect("fixture should serialize");

    let (count, imported_group) =
        import_session_transfer_into_store(&mut sessions, &text, SessionImportMode::Automatic, "")
            .expect("empty group should import");

    assert_eq!(count, 0);
    assert_eq!(imported_group.as_deref(), Some("Production Copy"));
    assert!(
        sessions
            .groups
            .iter()
            .any(|group| group == "Production Copy")
    );
}

#[tokio::test]
async fn deleting_a_profile_keeps_open_terminal_tabs() {
    let path = std::env::temp_dir().join(format!("ax-ssh-delete-{}.json", Uuid::new_v4()));
    let mut sessions = SessionStore::default();
    let profile = SessionProfile::new("server", "server.example", "alice");
    sessions.upsert(profile.clone());
    let mut app = AppState::new(ConfigStore::new(&path), sessions);
    let terminal_id = app.open_terminal_tab(&profile);
    let state = Arc::new(Mutex::new(app));

    let persistence = state
        .lock()
        .expect("state should remain readable")
        .persistence_coordinator
        .clone();
    delete_session_profile(&state, &persistence, &profile.id.to_string())
        .await
        .expect("profile should be deleted");

    let app = state.lock().expect("state should remain readable");
    assert!(app.sessions.sessions.is_empty());
    assert!(app.terminal(terminal_id).is_some());
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn overlapping_profile_mutations_are_rejected() {
    let path = std::env::temp_dir().join(format!("ax-ssh-save-order-{}.json", Uuid::new_v4()));
    let original = SessionProfile::new("server", "server.example", "alice");
    let mut sessions = SessionStore::default();
    sessions.upsert(original.clone());
    let state = Arc::new(Mutex::new(AppState::new(ConfigStore::new(&path), sessions)));

    let first = begin_profile_mutation(&state, original.id, Some(&original))
        .expect("first save should start");
    assert!(begin_profile_mutation(&state, original.id, Some(&original)).is_err());
    let saved = SessionProfile {
        name: "saved".to_owned(),
        ..original.clone()
    };

    commit_profile_save(&state, &saved, Some(&original), first).expect("first save should commit");
    assert_eq!(
        state
            .lock()
            .expect("state should remain readable")
            .sessions
            .sessions[0]
            .name,
        "saved"
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn profile_delete_waits_for_an_in_progress_mutation_to_finish() {
    let path = std::env::temp_dir().join(format!("ax-ssh-save-delete-{}.json", Uuid::new_v4()));
    let original = SessionProfile::new("server", "server.example", "alice");
    let mut sessions = SessionStore::default();
    sessions.upsert(original.clone());
    let state = Arc::new(Mutex::new(AppState::new(ConfigStore::new(&path), sessions)));

    let save =
        begin_profile_mutation(&state, original.id, Some(&original)).expect("save should start");
    assert!(begin_profile_mutation(&state, original.id, Some(&original)).is_err());
    commit_profile_save(&state, &original, Some(&original), save).expect("save should commit");
    let delete = begin_profile_mutation(&state, original.id, Some(&original))
        .expect("delete should start after the save finishes");
    commit_profile_delete(&state, &original, delete).expect("delete should commit");

    assert!(
        state
            .lock()
            .expect("state should remain readable")
            .sessions
            .sessions
            .is_empty()
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn credential_storage_commit_requires_the_current_profile_mutation() {
    let path =
        std::env::temp_dir().join(format!("ax-ssh-credential-storage-{}.json", Uuid::new_v4()));
    let original = SessionProfile::new("server", "server.example", "alice");
    let mut sessions = SessionStore::default();
    sessions.upsert(original.clone());
    let state = Arc::new(Mutex::new(AppState::new(ConfigStore::new(&path), sessions)));

    let token = begin_profile_mutation(&state, original.id, Some(&original))
        .expect("credential storage mutation should start");
    commit_profile_credential_storage(&state, &original, CredentialStorage::SystemKeyring, token)
        .expect("credential storage should commit");

    assert_eq!(
        state
            .lock()
            .expect("state should remain readable")
            .sessions
            .sessions[0]
            .ssh()
            .expect("profile should remain SSH")
            .credential_storage,
        Some(CredentialStorage::SystemKeyring)
    );
    let _ = std::fs::remove_file(path);
}

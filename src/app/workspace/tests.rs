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
    ssh.x11_forwarding = X11ForwardingMode::Trusted;

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
        X11ForwardingMode::Untrusted,
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
    assert_eq!(
        profile
            .ssh()
            .expect("updated profile should be SSH")
            .x11_forwarding,
        X11ForwardingMode::Untrusted
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
        "trusted",
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
            vault_password_generated,
            previous_vault_password_generated,
        } => {
            assert_eq!(storage, CredentialStorage::SystemKeyring);
            assert_eq!(previous_storage, Some(CredentialStorage::SystemKeyring));
            assert_eq!(password.as_str(), "new-password");
            assert!(vault_password.is_none());
            assert!(!vault_password_generated);
            assert!(!previous_vault_password_generated);
        }
        CredentialChange::None | CredentialChange::Delete { .. } => {
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
        "off",
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
        "trusted",
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
fn remembering_new_password_without_vault_password_generates_hidden_unlock_key() {
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
        "",
        "trusted",
        "",
        "115200",
        "8",
        "1",
        "none",
        "none",
        None,
    )
    .expect("profile should save with a generated vault password");
    assert_eq!(
        profile.ssh().and_then(|ssh| ssh.credential_storage),
        Some(CredentialStorage::EncryptedVault)
    );
    assert!(
        profile
            .ssh()
            .is_some_and(|ssh| ssh.credential_vault_key_saved)
    );
    match change {
        CredentialChange::Store {
            storage,
            vault_password,
            vault_password_generated,
            ..
        } => {
            assert_eq!(storage, CredentialStorage::EncryptedVault);
            let vault_password = vault_password.expect("generated vault password should be stored");
            assert!(vault_password.len() >= 40);
            assert!(vault_password_generated);
        }
        CredentialChange::None | CredentialChange::Delete { .. } => {
            panic!("generated vault password should retain encrypted-vault storage")
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
fn remembering_with_a_user_vault_password_does_not_enable_automatic_unlock_marker() {
    let (profile, change, _) = profile_from_editor_with_password(
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
        "trusted",
        "",
        "115200",
        "8",
        "1",
        "none",
        "none",
        None,
    )
    .expect("profile should save with a user vault password");

    assert!(profile.ssh().is_some_and(|ssh| {
        ssh.credential_storage == Some(CredentialStorage::EncryptedVault)
            && !ssh.credential_vault_key_saved
    }));
    match change {
        CredentialChange::Store {
            vault_password,
            vault_password_generated,
            ..
        } => {
            assert_eq!(
                vault_password
                    .expect("user vault password should be passed to storage")
                    .as_str(),
                "vault-password"
            );
            assert!(!vault_password_generated);
        }
        CredentialChange::None | CredentialChange::Delete { .. } => {
            panic!("user vault password should store the encrypted credential")
        }
    }
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
        "trusted",
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
        X11ForwardingMode::Trusted,
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
        X11ForwardingMode::Off,
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
        CredentialChange::Delete {
            storage: CredentialStorage::SystemKeyring,
            vault_password_generated: false,
        }
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
        "off",
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
        CredentialChange::Delete {
            storage: CredentialStorage::SystemKeyring,
            vault_password_generated: false,
        }
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
    source_ssh.x11_forwarding = X11ForwardingMode::Untrusted;
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
    assert_eq!(duplicate_ssh.x11_forwarding, X11ForwardingMode::Untrusted);
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
    source_ssh.x11_forwarding = X11ForwardingMode::Untrusted;
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
    assert_eq!(ssh.x11_forwarding, X11ForwardingMode::Untrusted);
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
    source_ssh.x11_forwarding = X11ForwardingMode::Untrusted;
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
    assert_eq!(exported_ssh.x11_forwarding, X11ForwardingMode::Untrusted);

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
    assert_eq!(imported_ssh.x11_forwarding, X11ForwardingMode::Untrusted);

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

    commit_profile_save(&state, &saved, Some(&original), first, false)
        .expect("first save should commit");
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
    commit_profile_save(&state, &original, Some(&original), save, false)
        .expect("save should commit");
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
    commit_profile_credential_storage(
        &state,
        &original,
        CredentialStorage::SystemKeyring,
        false,
        token,
    )
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

struct PasswordClearFixture {
    root: PathBuf,
    profile: SessionProfile,
    state: Arc<Mutex<AppState>>,
    store: ax_ssh::credentials::CredentialStore,
}

impl PasswordClearFixture {
    fn new(storage: CredentialStorage, generated: bool) -> Self {
        let root = std::env::temp_dir().join(format!("ax-ssh-password-clear-{}", Uuid::new_v4()));
        let store = ax_ssh::credentials::CredentialStore::new(root.join("credentials"));
        let mut profile = SessionProfile::new("server", "server.example", "alice");
        profile.group_name = "Production".into();
        profile.system_icon_override = Some("ubuntu".into());
        let ssh = profile.ssh_mut().expect("fixture should be SSH");
        ssh.credential_storage = Some(storage);
        ssh.credential_vault_key_saved = generated;
        ssh.host_key_fingerprint = Some("SHA256:trusted".into());
        profile.detected_system =
            ax_ssh::config::SystemDetection::new(&profile, "Ubuntu 24.04 LTS", "ubuntu")
                .map(Box::new);
        if storage == CredentialStorage::EncryptedVault {
            store
                .save_vault_password(profile.id, "fixture-password", "fixture-unlock")
                .expect("temporary vault should save");
            if generated {
                store
                    .save_vault_unlock_password(profile.id, "fixture-unlock")
                    .expect("temporary unlock key should save");
            }
        }
        let mut sessions = SessionStore::default();
        sessions.upsert(profile.clone());
        let config = ConfigStore::new(root.join("sessions.json"));
        config.save(&sessions).expect("fixture config should save");
        Self {
            root,
            profile,
            state: Arc::new(Mutex::new(AppState::new(config, sessions))),
            store,
        }
    }

    fn persistence(&self) -> Arc<crate::app::state::PersistenceCoordinator> {
        self.state
            .lock()
            .expect("fixture state should be readable")
            .persistence_coordinator
            .clone()
    }

    fn cleanup(self) {
        std::fs::remove_dir_all(self.root).expect("temporary fixture should be removed");
    }
}

#[test]
fn password_clear_commit_preserves_profile_editor_and_open_terminal() {
    // This tests the keyring reference without accessing a real system keyring.
    let fixture = PasswordClearFixture::new(CredentialStorage::SystemKeyring, false);
    let (terminal_id, editor_identity) = {
        let mut app = fixture.state.lock().expect("state should be readable");
        let terminal_id = app.open_terminal_tab(&fixture.profile);
        assert!(app.open_session_editor_for_profile(fixture.profile.id));
        app.sessions.settings.terminal.scrollback_lines = 321;
        (terminal_id, app.active_editor_identity())
    };
    let token = begin_profile_mutation(&fixture.state, fixture.profile.id, Some(&fixture.profile))
        .expect("clear should start");
    commit_profile_password_clear(&fixture.state, &fixture.profile, token)
        .expect("clear should commit");

    let mut expected = fixture.profile.clone();
    expected
        .ssh_mut()
        .expect("expected profile should be SSH")
        .credential_storage = None;
    let mut app = fixture.state.lock().expect("state should be readable");
    assert_eq!(app.sessions.sessions, vec![expected.clone()]);
    assert_eq!(
        app.config.load().expect("config should load").sessions,
        vec![expected]
    );
    assert_eq!(app.active_editor_identity(), editor_identity);
    assert!(
        app.active_snapshot()
            .editor
            .expect("editor should remain open")
            .credential_storage
            .is_empty()
    );
    assert!(app.terminal(terminal_id).is_some());
    assert_eq!(app.sessions.settings.terminal.scrollback_lines, 321);
    assert_eq!(
        app.config.load().expect("config should load").settings,
        SessionStore::default().settings
    );
    assert!(!app.profile_mutation_is_pending(fixture.profile.id));
    drop(app);
    fixture.cleanup();
}

#[tokio::test]
async fn password_clear_removes_only_the_referenced_vault_record_and_unlock_key() {
    for generated in [false, true] {
        let fixture = PasswordClearFixture::new(CredentialStorage::EncryptedVault, generated);
        let other = SessionProfile::new("other", "other.example", "bob");
        fixture
            .store
            .save_vault_password(other.id, "other-password", "other-unlock")
            .expect("unrelated record should save");
        let persistence = fixture.persistence();
        clear_saved_password_in_store(
            &fixture.state,
            &persistence,
            &fixture.profile.id.to_string(),
            fixture.store.clone(),
        )
        .await
        .expect("saved password should clear immediately");

        assert_eq!(
            fixture
                .store
                .load_vault_password(fixture.profile.id, "fixture-unlock")
                .expect("cleared record should be absent"),
            None
        );
        assert_eq!(
            fixture
                .store
                .load_vault_unlock_password(fixture.profile.id)
                .expect("cleared unlock key should be absent"),
            None
        );
        assert_eq!(
            fixture
                .store
                .load_vault_password(other.id, "other-unlock")
                .expect("unrelated record should load"),
            Some("other-password".into())
        );
        let mut expected = fixture.profile.clone();
        let ssh = expected.ssh_mut().expect("expected profile should be SSH");
        ssh.credential_storage = None;
        ssh.credential_vault_key_saved = false;
        {
            let app = fixture.state.lock().expect("state should be readable");
            assert_eq!(app.sessions.sessions, vec![expected.clone()]);
            assert_eq!(
                app.config.load().expect("config should load").sessions,
                vec![expected]
            );
        }

        let error = clear_saved_password_in_store(
            &fixture.state,
            &persistence,
            &fixture.profile.id.to_string(),
            fixture.store.clone(),
        )
        .await
        .expect_err("a cleared profile must not mutate storage again");
        assert!(error.to_string().contains("no saved password"));
        fixture.cleanup();
    }
}

#[tokio::test]
async fn password_clear_restores_vault_and_unlock_key_if_config_is_unreadable() {
    let fixture = PasswordClearFixture::new(CredentialStorage::EncryptedVault, true);
    let record = fixture
        .root
        .join("credentials")
        .join(format!("{}.json", fixture.profile.id));
    let original_record = std::fs::read(&record).expect("encrypted record should exist");
    let config = fixture.root.join("sessions.json");
    std::fs::rename(&config, fixture.root.join("original.json"))
        .expect("original config should move");
    std::fs::create_dir(&config).expect("directory should block config replacement");
    let persistence = fixture.persistence();

    clear_saved_password_in_store(
        &fixture.state,
        &persistence,
        &fixture.profile.id.to_string(),
        fixture.store.clone(),
    )
    .await
    .expect_err("config failure should fail the clear");

    assert_eq!(
        std::fs::read(&record).expect("record should be restored"),
        original_record
    );
    assert_eq!(
        fixture
            .store
            .load_vault_password(fixture.profile.id, "fixture-unlock")
            .expect("restored password should decrypt"),
        Some("fixture-password".into())
    );
    assert_eq!(
        fixture
            .store
            .load_vault_unlock_password(fixture.profile.id)
            .expect("unlock key should be restored"),
        Some("fixture-unlock".into())
    );
    let app = fixture.state.lock().expect("state should be readable");
    assert_eq!(app.sessions.sessions, vec![fixture.profile.clone()]);
    assert!(!app.profile_mutation_is_pending(fixture.profile.id));
    drop(app);
    fixture.cleanup();
}

#[cfg(unix)]
#[tokio::test]
async fn password_clear_restores_vault_and_unlock_key_if_config_write_fails() {
    let fixture = PasswordClearFixture::new(CredentialStorage::EncryptedVault, true);
    // The destination fits the filesystem's component limit; its atomic-write
    // temporary name exceeds it. Loading succeeds and writing fails deterministically.
    let config = fixture.root.join(format!("{}.json", "c".repeat(240)));
    let original =
        std::fs::read(fixture.root.join("sessions.json")).expect("original config should exist");
    std::fs::write(&config, &original).expect("long destination name should be valid");
    fixture
        .state
        .lock()
        .expect("state should be readable")
        .config = ConfigStore::new(&config);
    let persistence = fixture.persistence();

    let error = clear_saved_password_in_store(
        &fixture.state,
        &persistence,
        &fixture.profile.id.to_string(),
        fixture.store.clone(),
    )
    .await
    .expect_err("atomic write failure should fail the clear");
    assert!(format!("{error:#}").contains("failed to create"));
    assert_eq!(
        std::fs::read(&config).expect("config should remain"),
        original
    );
    assert_eq!(
        fixture
            .store
            .load_vault_password(fixture.profile.id, "fixture-unlock")
            .expect("restored password should decrypt"),
        Some("fixture-password".into())
    );
    assert_eq!(
        fixture
            .store
            .load_vault_unlock_password(fixture.profile.id)
            .expect("unlock key should be restored"),
        Some("fixture-unlock".into())
    );
    let app = fixture.state.lock().expect("state should be readable");
    assert_eq!(app.sessions.sessions, vec![fixture.profile.clone()]);
    assert!(!app.profile_mutation_is_pending(fixture.profile.id));
    drop(app);
    fixture.cleanup();
}

#[tokio::test]
async fn password_clear_removes_stale_references_when_vault_files_are_already_missing() {
    let fixture = PasswordClearFixture::new(CredentialStorage::EncryptedVault, true);
    fixture
        .store
        .delete_vault_password(fixture.profile.id)
        .expect("external password deletion should succeed");
    fixture
        .store
        .delete_vault_unlock_password(fixture.profile.id)
        .expect("external unlock deletion should succeed");
    let persistence = fixture.persistence();
    clear_saved_password_in_store(
        &fixture.state,
        &persistence,
        &fixture.profile.id.to_string(),
        fixture.store.clone(),
    )
    .await
    .expect("missing records should still allow clearing the reference");

    let app = fixture.state.lock().expect("state should be readable");
    let ssh = app.sessions.sessions[0]
        .ssh()
        .expect("profile should remain SSH");
    assert_eq!(ssh.credential_storage, None);
    assert!(!ssh.credential_vault_key_saved);
    assert_eq!(
        app.config.load().expect("config should load").sessions,
        app.sessions.sessions
    );
    assert!(!app.profile_mutation_is_pending(fixture.profile.id));
    drop(app);
    fixture.cleanup();
}

#[tokio::test]
async fn password_clear_releases_its_token_when_the_vault_cannot_be_read() {
    let fixture = PasswordClearFixture::new(CredentialStorage::EncryptedVault, true);
    let record = fixture
        .root
        .join("credentials")
        .join(format!("{}.json", fixture.profile.id));
    std::fs::remove_file(&record).expect("fixture record should remove");
    std::fs::create_dir(&record).expect("directory should block the vault read");
    let persistence = fixture.persistence();

    clear_saved_password_in_store(
        &fixture.state,
        &persistence,
        &fixture.profile.id.to_string(),
        fixture.store.clone(),
    )
    .await
    .expect_err("unreadable vault should fail the clear");

    assert_eq!(
        fixture
            .store
            .load_vault_unlock_password(fixture.profile.id)
            .expect("unlock key should remain"),
        Some("fixture-unlock".into())
    );
    let app = fixture.state.lock().expect("state should be readable");
    assert_eq!(app.sessions.sessions, vec![fixture.profile.clone()]);
    assert_eq!(
        app.config.load().expect("config should load").sessions,
        vec![fixture.profile.clone()]
    );
    assert!(!app.profile_mutation_is_pending(fixture.profile.id));
    drop(app);
    fixture.cleanup();
}

#[tokio::test]
async fn password_clear_rejects_an_overlapping_profile_mutation() {
    let fixture = PasswordClearFixture::new(CredentialStorage::EncryptedVault, true);
    let token = begin_profile_mutation(&fixture.state, fixture.profile.id, Some(&fixture.profile))
        .expect("first mutation should start");
    let persistence = fixture.persistence();
    let error = clear_saved_password_in_store(
        &fixture.state,
        &persistence,
        &fixture.profile.id.to_string(),
        fixture.store.clone(),
    )
    .await
    .expect_err("overlapping clear should fail");
    assert!(error.to_string().contains("already being modified"));
    assert_eq!(
        fixture
            .store
            .load_vault_password(fixture.profile.id, "fixture-unlock")
            .expect("original password should remain"),
        Some("fixture-password".into())
    );
    let app = fixture.state.lock().expect("state should be readable");
    assert_eq!(app.sessions.sessions, vec![fixture.profile.clone()]);
    assert!(app.profile_mutation_is_current(fixture.profile.id, token));
    drop(app);
    finish_profile_mutation(&fixture.state, fixture.profile.id, token);
    fixture.cleanup();
}

#[test]
fn password_clear_commit_rejects_a_changed_profile() {
    let fixture = PasswordClearFixture::new(CredentialStorage::SystemKeyring, false);
    let token = begin_profile_mutation(&fixture.state, fixture.profile.id, Some(&fixture.profile))
        .expect("clear should start");
    fixture
        .state
        .lock()
        .expect("state should be readable")
        .sessions
        .sessions[0]
        .name = "changed".into();
    commit_profile_password_clear(&fixture.state, &fixture.profile, token)
        .expect_err("stale clear must not overwrite the current profile");

    let mut expected = fixture.profile.clone();
    expected.name = "changed".into();
    let app = fixture.state.lock().expect("state should be readable");
    assert_eq!(app.sessions.sessions, vec![expected]);
    assert_eq!(
        app.config.load().expect("config should load").sessions,
        vec![fixture.profile.clone()]
    );
    assert!(!app.profile_mutation_is_pending(fixture.profile.id));
    drop(app);
    fixture.cleanup();
}

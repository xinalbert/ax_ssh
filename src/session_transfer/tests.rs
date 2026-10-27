use super::*;
use crate::config::{CredentialStorage, X11ForwardingMode};

#[test]
fn server_export_removes_identity_credentials_and_host_trust() {
    let mut sessions = SessionStore::default();
    let mut source = SessionProfile::new("production", "server.example", "alice");
    source.group_name = "Production".into();
    let source_ssh = source.ssh_mut().expect("profile should be SSH");
    source_ssh.credential_storage = Some(CredentialStorage::SystemKeyring);
    source_ssh.credential_vault_key_saved = true;
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
    assert!(!ssh.credential_vault_key_saved);
    assert_eq!(ssh.host_key_fingerprint, None);
    assert_eq!(ssh.x11_forwarding, X11ForwardingMode::Untrusted);
}

#[test]
fn transfer_rejects_unknown_envelopes_wrong_versions_and_profile_or_byte_overflow() {
    let profile = SessionProfile::new("fixture", "example.invalid", "fixture");
    let envelope = SessionTransferEnvelope {
        format: SESSION_TRANSFER_FORMAT.into(),
        version: SESSION_TRANSFER_VERSION,
        kind: SessionTransferKind::Server,
        group_name: None,
        profiles: vec![profile.clone()],
    };
    let mut value = serde_json::to_value(&envelope).unwrap();
    value["version"] = 2.into();
    assert!(parse_session_transfer(&value.to_string()).is_err());
    value["version"] = 1.into();
    value["unknown"] = true.into();
    assert!(parse_session_transfer(&value.to_string()).is_err());
    let mut group = envelope;
    group.kind = SessionTransferKind::Group;
    group.group_name = Some("fixture".into());
    group.profiles = vec![profile; MAX_SESSION_TRANSFER_PROFILES + 1];
    assert!(parse_session_transfer(&serde_json::to_string(&group).unwrap()).is_err());
    group.profiles.truncate(MAX_SESSION_TRANSFER_PROFILES);
    for profile in &mut group.profiles {
        profile.group_name = "fixture".into();
        profile.ssh_mut().unwrap().auth = AuthMethod::PrivateKey {
            path: "x".repeat(4096).into(),
        };
    }
    let store = SessionStore {
        sessions: group.profiles,
        groups: vec!["fixture".into()],
        ..Default::default()
    };
    assert!(export_session_group(&store, "fixture").is_err());
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

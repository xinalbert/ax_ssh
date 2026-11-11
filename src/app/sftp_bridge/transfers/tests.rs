use super::*;

#[tokio::test]
async fn cancelled_upload_producer_cannot_register_a_late_file() {
    let mut app = AppState::new(
        ConfigStore::new(
            std::env::temp_dir().join(format!("axssh-controls-{}.json", Uuid::new_v4())),
        ),
        SessionStore::default(),
    );
    let tab = app.open_sftp_tab(&SessionProfile::new(
        "fixture",
        "fixture.invalid",
        "fixture",
    ));
    let state = Arc::new(Mutex::new(app));
    let generation = submission_generation(&state, tab).expect("generation");
    {
        let mut app = state.lock().expect("state");
        request_all_transfer_actions(
            app.terminal_mut(tab).expect("tab"),
            ax_ssh::sftp::SftpTransferAction::Cancel,
        )
        .expect("cancel old producer");
    }
    let file = PreparedLocalUpload {
        id: Uuid::new_v4(),
        local_path: std::env::temp_dir().join("never-opened-control-fixture"),
        remote_path: "/late.bin".into(),
        remote_directories: Vec::new(),
        size: 1,
    };
    let error =
        queue_upload_file_for_generation(&state, tab, Uuid::new_v4(), file, Some(generation))
            .await
            .expect_err("old generation rejected before reading source");
    assert!(error.to_string().contains("cancelled"));
    let app = state.lock().expect("state");
    assert!(app.terminal(tab).expect("tab").sftp.transfers.is_empty());
    assert!(
        app.terminal(tab)
            .expect("tab")
            .sftp
            .transfer_intents
            .is_empty()
    );
}

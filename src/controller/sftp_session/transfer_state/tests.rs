use super::*;
use crate::ssh::SftpTestFixture;
use std::time::Duration;

mod batch;

fn item(id: Uuid, phase: SftpTransferPhase, pausable: bool) -> SftpTransferView {
    SftpTransferView {
        id,
        name: "fixture".into(),
        status: "arbitrary text".into(),
        downloaded_bytes: 0,
        total_bytes: 256 * 1024,
        conflict: false,
        phase,
        pausable,
    }
}

pub(in crate::controller::sftp_session) async fn worker() -> Worker {
    let template = SftpSession::idle(&Handle::current(), Arc::new(|| {}));
    let view = template.client().view().unwrap();
    template.shutdown().await.unwrap();
    let (_commands, receiver) = mpsc::channel(COMMAND_CAPACITY);
    let (_stop, cancelled) = watch::channel(false);
    let (_settings, updates) = watch::channel(Settings::default());
    Worker {
        runtime: Handle::current(),
        commands: receiver,
        stop: cancelled,
        settings: Settings::default(),
        updates,
        transport: None,
        events: None,
        profile: None,
        probe: None,
        probe_task: None,
        pending_secret: None,
        prompt: None,
        generation: 1,
        identity: ConnectionIdentity::Local,
        connected: false,
        status: String::new(),
        home: String::new(),
        path: String::new(),
        loading: false,
        has_more: false,
        truncated: false,
        request_id: 0,
        entries: vec![],
        local_path: String::new(),
        local_loading: false,
        local_entries: vec![],
        local_revision: 0,
        local_files: HashMap::new(),
        local_open: local_open::Opening::default(),
        preparation: transfers::Preparation::default(),
        icon_task: None,
        icons_dirty: false,
        transfers: BTreeMap::new(),
        last_batch: None,
        last_download_batch: None,
        mutation: None,
        editing: editing::Editing::default(),
        shared: Arc::new(Mutex::new(Shared {
            view,
            mutation: None,
        })),
        wake: Arc::new(|| {}),
    }
}

pub(in crate::controller::sftp_session) async fn finish(worker: &mut Worker) {
    worker.stop_transport().await.unwrap();
    worker.preparation.finish().await.unwrap();
    worker.local_open.finish().await.unwrap();
    worker.editing.finish().await.unwrap();
    if let Some(task) = worker.icon_task.take() {
        task.await.unwrap();
    }
}

#[test]
fn controls_require_capability_and_commit_waiting_state_only_after_enqueue_success() {
    use SftpTransferPhase as Phase;
    for (phase, pause, resume, cancel) in [
        (Phase::Queued, false, false, true),
        (Phase::Transferring, true, false, true),
        (Phase::Pausing, false, false, true),
        (Phase::Paused, false, true, true),
        (Phase::Resuming, false, false, true),
        (Phase::Cancelling, false, false, false),
        (Phase::Conflict, false, false, true),
        (Phase::Completed, false, false, false),
        (Phase::Cancelled, false, false, false),
        (Phase::Failed, false, false, false),
        (Phase::Skipped, false, false, false),
    ] {
        for pausable in [true, false] {
            for control in [Control::Pause, Control::Resume, Control::Cancel] {
                let mut view = item(Uuid::new_v4(), phase, pausable);
                let before = view.clone();
                assert_eq!(view.can_pause(), pause && pausable);
                assert_eq!(view.can_resume(), resume && pausable);
                assert_eq!(view.can_cancel(), cancel);
                let allowed = match control {
                    Control::Pause => pause && pausable,
                    Control::Resume => resume && pausable,
                    Control::Cancel => cancel,
                };
                let mut sent = 0;
                assert!(
                    view.request_control(control, || {
                        sent += 1;
                        anyhow::bail!("queue full")
                    })
                    .is_err()
                );
                assert_eq!(view, before);
                assert_eq!(sent, usize::from(allowed));
                assert_eq!(view.request_control(control, || Ok(())).is_ok(), allowed);
                if allowed {
                    assert!(!view.can_pause() && !view.can_resume());
                    if matches!(control, Control::Cancel) {
                        assert!(!view.can_cancel());
                    }
                }
            }
        }
    }
}

#[tokio::test]
async fn progress_preserves_pending_phases_and_terminal_events_cannot_revive_history() {
    use SftpTransferPhase as Phase;
    let mut worker = worker().await;
    let id = Uuid::new_v4();
    worker
        .transfers
        .insert(id, item(id, Phase::Transferring, true));
    worker
        .transfers
        .get_mut(&id)
        .unwrap()
        .request_control(Control::Pause, || Ok(()))
        .unwrap();
    worker.transfer(SftpTransferEvent::Progress {
        transfer_id: id,
        downloaded_bytes: 10,
        total_bytes: 100,
    });
    assert_eq!(worker.transfers[&id].phase, Phase::Pausing);
    worker.transfer(SftpTransferEvent::Paused {
        transfer_id: id,
        downloaded_bytes: 12,
        total_bytes: 100,
    });
    assert!(worker.transfers[&id].can_resume());
    worker
        .transfers
        .get_mut(&id)
        .unwrap()
        .request_control(Control::Resume, || Ok(()))
        .unwrap();
    worker.transfer(SftpTransferEvent::Progress {
        transfer_id: id,
        downloaded_bytes: 14,
        total_bytes: 100,
    });
    assert_eq!(worker.transfers[&id].phase, Phase::Resuming);
    worker.transfer(SftpTransferEvent::Resumed {
        transfer_id: id,
        downloaded_bytes: 15,
        total_bytes: 100,
    });
    assert_eq!(worker.transfers[&id].downloaded_bytes, 15);
    assert!(worker.transfers[&id].can_pause());
    worker.transfer(SftpTransferEvent::Completed {
        transfer_id: id,
        local_path: "/fixture".into(),
        total_bytes: 100,
    });
    let completed = worker.transfers[&id].clone();
    for event in [
        SftpTransferEvent::Progress {
            transfer_id: id,
            downloaded_bytes: 1,
            total_bytes: 100,
        },
        SftpTransferEvent::Queued {
            transfer_id: id,
            remote_path: "/fixture".into(),
            name: "late".into(),
            total_bytes: 100,
        },
        SftpTransferEvent::Failed {
            transfer_id: id,
            message: "late".into(),
        },
    ] {
        worker.transfer(event);
        assert_eq!(worker.transfers[&id], completed);
    }
    worker.transfer(SftpTransferEvent::Progress {
        transfer_id: Uuid::new_v4(),
        downloaded_bytes: 1,
        total_bytes: 100,
    });
    assert_eq!(worker.transfers.len(), 1);
    let upload = Uuid::new_v4();
    worker.transfer(SftpTransferEvent::Queued {
        transfer_id: upload,
        remote_path: String::new(),
        name: "upload".into(),
        total_bytes: 100,
    });
    worker.transfer(SftpTransferEvent::Started {
        transfer_id: upload,
        remote_path: String::new(),
        name: "upload".into(),
        total_bytes: 100,
    });
    assert!(!worker.transfers[&upload].can_pause());
    finish(&mut worker).await;
}

#[tokio::test]
async fn history_admission_reclaims_only_finished_rows_and_disconnect_retires_pending_controls() {
    use SftpTransferPhase as Phase;
    let mut worker = worker().await;
    for _ in 0..MAX_TRANSFERS {
        let id = Uuid::new_v4();
        worker
            .transfers
            .insert(id, item(id, Phase::Transferring, true));
    }
    assert!(worker.transfer_evictions(1).is_err());
    let id = *worker.transfers.keys().next().unwrap();
    worker.transfer(SftpTransferEvent::Failed {
        transfer_id: id,
        message: "fixture".into(),
    });
    assert_eq!(worker.transfer_evictions(1).unwrap(), vec![id]);
    assert_eq!(worker.transfers.len(), MAX_TRANSFERS);
    worker.transfer(SftpTransferEvent::Queued {
        transfer_id: Uuid::new_v4(),
        remote_path: "/child".into(),
        name: "child".into(),
        total_bytes: 10,
    });
    assert!(!worker.transfers.contains_key(&id));
    assert_eq!(worker.transfers.len(), MAX_TRANSFERS);
    worker.event(SshSessionEvent::Disconnected).await.unwrap();
    assert!(worker.transfers.values().all(|view| view.phase.finished()
        && !view.can_pause()
        && !view.can_resume()
        && !view.can_cancel()));
    assert!(worker.transfer_evictions(MAX_TRANSFERS).is_ok());
    finish(&mut worker).await;
}

pub(in crate::controller::sftp_session) async fn until(
    worker: &mut Worker,
    mut predicate: impl FnMut(&SshSessionEvent) -> bool,
) {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let event = worker.events.as_mut().unwrap().recv().await.unwrap();
            let done = predicate(&event);
            worker.event(event).await.unwrap();
            if done {
                break;
            }
        }
    })
    .await
    .unwrap();
}

async fn paused_download(fixture: &SftpTestFixture, directory: PathBuf) -> (Worker, Uuid) {
    let mut worker = worker().await;
    let (transport, events) = fixture.worker().await;
    worker.transport = Some(transport);
    worker.events = Some(events);
    until(&mut worker, |event| {
        matches!(
            event,
            SshSessionEvent::Sftp(SftpBrowserEvent::DirectoryPage { .. })
        )
    })
    .await;
    worker
        .queue_download("/remote/slow.bin".into(), directory)
        .unwrap();
    let id = *worker.transfers.keys().next().unwrap();
    tokio::time::timeout(Duration::from_secs(10), fixture.slow_read_started())
        .await
        .unwrap();
    until(&mut worker, |event| matches!(event, SshSessionEvent::SftpTransfer(SftpTransferEvent::Started {transfer_id,..}) if *transfer_id==id)).await;
    let before = worker.transfers[&id].clone();
    worker
        .dispatch_queued(QueuedCommand {
            generation: 0,
            command: Command::Pause(id),
        })
        .await
        .unwrap();
    assert_eq!(worker.transfers[&id], before);
    worker
        .dispatch_queued(QueuedCommand {
            generation: 1,
            command: Command::Pause(id),
        })
        .await
        .unwrap();
    assert_eq!(worker.transfers[&id].phase, SftpTransferPhase::Pausing);
    assert!(worker.dispatch(Command::Pause(id)).await.is_err());
    worker
        .transport
        .as_ref()
        .unwrap()
        .request_system_release()
        .unwrap();
    // Independent exec proves Pause was consumed before outstanding reads finish.
    until(&mut worker, |event| {
        matches!(event, SshSessionEvent::SystemRelease(None))
    })
    .await;
    fixture.release_slow_read();
    until(&mut worker, |event| matches!(event, SshSessionEvent::SftpTransfer(SftpTransferEvent::Paused {transfer_id,..}) if *transfer_id==id)).await;
    assert!(worker.transfers[&id].can_resume());
    assert!(worker.transfers[&id].downloaded_bytes > 0);
    assert!(worker.transfers[&id].downloaded_bytes < worker.transfers[&id].total_bytes);
    (worker, id)
}

#[tokio::test]
async fn real_download_pause_resume_preserves_bytes_and_generation_scoped_eligibility() {
    let fixture = SftpTestFixture::new(false).await;
    let content: Vec<_> = (0..256 * 1024).map(|index| (index % 251) as u8).collect();
    fixture.replace("/remote/slow.bin", &content);
    let directory = std::env::temp_dir().join(format!("axssh-pause-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&directory).unwrap();
    let (mut worker, id) = paused_download(&fixture, directory.clone()).await;
    assert!(!directory.join("slow.bin").exists());
    worker.dispatch(Command::Resume(id)).await.unwrap();
    assert_eq!(worker.transfers[&id].phase, SftpTransferPhase::Resuming);
    until(&mut worker, |event| matches!(event, SshSessionEvent::SftpTransfer(SftpTransferEvent::Resumed {transfer_id,..}) if *transfer_id==id)).await;
    until(&mut worker, |event| matches!(event, SshSessionEvent::SftpTransfer(SftpTransferEvent::Completed {transfer_id,..}) if *transfer_id==id)).await;
    assert_eq!(
        std::fs::read(directory.join("slow.bin")).unwrap(),
        fixture.contents("/remote/slow.bin")
    );
    assert!(!worker.transfers[&id].can_cancel());
    assert!(worker.dispatch(Command::Resume(id)).await.is_err());
    assert!(worker.dispatch(Command::Cancel(id)).await.is_err());
    finish(&mut worker).await;
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn paused_download_cancel_and_shutdown_join_and_remove_partial_files() {
    for cancel in [true, false] {
        let fixture = SftpTestFixture::new(false).await;
        let directory =
            std::env::temp_dir().join(format!("axssh-pause-cleanup-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let (mut worker, id) = paused_download(&fixture, directory.clone()).await;
        if cancel {
            worker.dispatch(Command::Cancel(id)).await.unwrap();
            until(&mut worker, |event| matches!(event, SshSessionEvent::SftpTransfer(SftpTransferEvent::Cancelled {transfer_id}) if *transfer_id==id)).await;
        }
        tokio::time::timeout(Duration::from_secs(10), finish(&mut worker))
            .await
            .unwrap();
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 0);
        std::fs::remove_dir_all(directory).unwrap();
    }
}

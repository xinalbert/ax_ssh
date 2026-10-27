use super::*;

#[test]
fn partial_admission_preserves_successes_skips_ineligible_and_bounds_each_error() {
    use SftpTransferPhase as Phase;
    let ids: Vec<_> = (1..=7).map(Uuid::from_u128).collect();
    let mut transfers: BTreeMap<_, _> = ids[..6]
        .iter()
        .map(|id| (*id, item(*id, Phase::Transferring, true)))
        .collect();
    transfers.get_mut(&ids[3]).unwrap().pausable = false;
    transfers.get_mut(&ids[4]).unwrap().phase = Phase::Completed;
    transfers.get_mut(&ids[5]).unwrap().phase = Phase::Pausing;
    let before = transfers.clone();
    let mut sent = vec![];
    let result = apply_batch(&mut transfers, &ids, Control::Pause, |id, control| {
        assert_eq!(control, Control::Pause);
        sent.push(id);
        if id == ids[1] {
            anyhow::bail!("{}", "界".repeat(600));
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(sent, ids[..3]);
    assert_eq!((result.requested, result.queued, result.skipped), (7, 2, 4));
    assert_eq!(result.failures.len(), 1);
    assert_eq!(result.failures[0].transfer_id, ids[1]);
    assert_eq!(result.failures[0].message.chars().count(), 512);
    for id in [ids[0], ids[2]] {
        assert_eq!(transfers[&id].phase, Phase::Pausing);
    }
    for id in &ids[1..6] {
        if *id != ids[2] {
            assert_eq!(transfers[id], before[id]);
        }
    }
    // A retry ignores already accepted rows and may admit the previously failed row.
    let retry = apply_batch(&mut transfers, &ids, Control::Pause, |id, _| {
        assert_eq!(id, ids[1]);
        Ok(())
    })
    .unwrap();
    assert_eq!((retry.queued, retry.skipped), (1, 6));
}

#[tokio::test]
async fn malformed_disconnected_and_stale_batches_leave_state_untouched() {
    let id = Uuid::new_v4();
    let mut transfers = BTreeMap::from([(id, item(id, SftpTransferPhase::Paused, true))]);
    let before = transfers.clone();
    let session = SftpSession::idle(&Handle::current(), Arc::new(|| {}));
    let client = session.client();
    for ids in [
        vec![],
        vec![id, id],
        (0..=MAX_SFTP_TRANSFER_BATCH)
            .map(|value| Uuid::from_u128(value as u128))
            .collect(),
    ] {
        assert!(
            client
                .control_transfers(ids.clone(), Control::Resume)
                .is_err()
        );
        assert!(
            apply_batch(&mut transfers, &ids, Control::Resume, |_, _| {
                panic!("invalid batch must not reach sender")
            })
            .is_err()
        );
        assert_eq!(transfers, before);
    }
    session.shutdown().await.unwrap();
    let mut worker = worker().await;
    worker.transfers = transfers;
    assert!(worker.control_batch(vec![id], Control::Resume).is_err());
    worker.connected = true;
    assert!(worker.control_batch(vec![id], Control::Resume).is_err());
    worker
        .dispatch_queued(QueuedCommand {
            generation: worker.generation - 1,
            command: Command::ControlBatch {
                ids: vec![id],
                control: Control::Resume,
            },
        })
        .await
        .unwrap();
    assert_eq!(worker.transfers, before);
    assert!(worker.last_batch.is_none());
    finish(&mut worker).await;
}

async fn wait_for(
    worker: &mut Worker,
    stage: &str,
    mut predicate: impl FnMut(&SshSessionEvent) -> bool,
) {
    let result = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let event = worker.events.as_mut().unwrap().recv().await.unwrap();
            let done = predicate(&event);
            worker.event(event).await.unwrap();
            if done {
                break;
            }
        }
    })
    .await;
    assert!(
        result.is_ok(),
        "batch stage {stage}: {:?}",
        worker.transfers
    );
}

async fn wait_for_phase(worker: &mut Worker, phase: SftpTransferPhase) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while !worker.transfers.values().all(|item| item.phase == phase) {
            let event = worker.events.as_mut().unwrap().recv().await.unwrap();
            worker.event(event).await.unwrap();
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn real_two_download_batches_pause_resume_or_cancel_and_clear_on_disconnect() {
    for control in [Control::Resume, Control::Cancel] {
        let fixture = SftpTestFixture::new(false).await;
        let content: Vec<_> = (0..256 * 1024).map(|index| (index % 251) as u8).collect();
        fixture.replace("/remote/slow.bin", &content);
        let directory = std::env::temp_dir().join(format!("axssh-batch-{}", Uuid::new_v4()));
        let mut worker = worker().await;
        let (transport, events) = fixture.worker().await;
        worker.transport = Some(transport);
        worker.events = Some(events);
        wait_for(&mut worker, "directory", |event| {
            matches!(
                event,
                SshSessionEvent::Sftp(SftpBrowserEvent::DirectoryPage { .. })
            )
        })
        .await;
        for index in 0..2 {
            let target = directory.join(index.to_string());
            std::fs::create_dir_all(&target).unwrap();
            worker
                .queue_download("/remote/slow.bin".into(), target)
                .unwrap();
        }
        let ids: Vec<_> = worker.transfers.keys().copied().collect();
        let mut started = HashSet::new();
        wait_for(&mut worker, "started", |event| {
            if let SshSessionEvent::SftpTransfer(SftpTransferEvent::Started {
                transfer_id, ..
            }) = event
            {
                started.insert(*transfer_id);
            }
            started.len() == 2
        })
        .await;
        tokio::time::timeout(Duration::from_secs(10), fixture.slow_read_started())
            .await
            .unwrap();
        worker.control_batch(ids.clone(), Control::Pause).unwrap();
        assert_eq!(worker.last_batch.as_ref().unwrap().queued, 2);
        worker
            .transport
            .as_ref()
            .unwrap()
            .request_system_release()
            .unwrap();
        wait_for(&mut worker, "barrier", |event| {
            matches!(event, SshSessionEvent::SystemRelease(None))
        })
        .await;
        fixture.release_slow_read();
        // The FIFO barrier may already consume Paused receipts. Check owner state
        // before waiting so early acknowledgements are retained rather than lost.
        wait_for_phase(&mut worker, SftpTransferPhase::Paused).await;
        assert!(worker.transfers.values().all(SftpTransferView::can_resume));
        let mut batch = ids.clone();
        batch.push(Uuid::new_v4());
        worker
            .dispatch(Command::ControlBatch {
                ids: batch,
                control,
            })
            .await
            .unwrap();
        let result = worker.last_batch.as_ref().unwrap();
        assert_eq!((result.requested, result.queued, result.skipped), (3, 2, 1));
        assert!(result.failures.is_empty());
        wait_for_phase(
            &mut worker,
            match control {
                Control::Resume => SftpTransferPhase::Completed,
                Control::Cancel => SftpTransferPhase::Cancelled,
                Control::Pause => unreachable!(),
            },
        )
        .await;
        worker.publish().unwrap();
        assert_eq!(
            worker.shared.lock().unwrap().view.last_batch,
            worker.last_batch
        );
        worker.dispatch(Command::Disconnect).await.unwrap();
        assert!(worker.last_batch.is_none());
        finish(&mut worker).await;
        for index in 0..2 {
            let target = directory.join(index.to_string());
            if control == Control::Resume {
                assert_eq!(std::fs::read(target.join("slow.bin")).unwrap(), content);
            } else {
                assert_eq!(std::fs::read_dir(target).unwrap().count(), 0);
            }
        }
        std::fs::remove_dir_all(directory).unwrap();
    }
}

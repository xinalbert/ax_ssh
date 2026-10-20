use super::*;
use ax_ssh::terminal::TerminalOutputChunk;
use tokio::time::timeout;

struct Fixture {
    state: Arc<Mutex<AppState>>,
    profile: SessionProfile,
    tab: Uuid,
    attempt: Uuid,
    directory: PathBuf,
}

impl Fixture {
    fn new(target: ConnectionTarget) -> Self {
        let directory = std::env::temp_dir().join(format!("axssh-monitor-{}", Uuid::new_v4()));
        std::fs::create_dir(&directory).expect("isolated monitor directory");
        let profile = SessionProfile::new("Fixture", "fixture.invalid", "fixture");
        let mut app = AppState::new(
            ConfigStore::new(directory.join("config.json")),
            SessionStore {
                sessions: vec![profile.clone()],
                ..Default::default()
            },
        );
        let tab = match target {
            ConnectionTarget::Terminal => app.open_terminal_tab(&profile),
            ConnectionTarget::Sftp => app.open_sftp_tab(&profile),
        };
        let attempt = Uuid::new_v4();
        let terminal = app.terminal_mut(tab).expect("terminal state");
        terminal.set_ssh_attempt(Some(attempt));
        terminal.worker_running = true;
        terminal.reconnect_enabled = false;
        terminal.sftp.local.path = directory.to_string_lossy().into_owned();
        Self {
            state: Arc::new(Mutex::new(app)),
            profile,
            tab,
            attempt,
            directory,
        }
    }

    fn monitor(&self, target: ConnectionTarget) -> (mpsc::Sender<SshSessionEvent>, JoinHandle<()>) {
        let (sender, events) = mpsc::channel(16);
        let task = spawn_session_monitor(
            &Handle::current(),
            self.state.clone(),
            slint::Weak::default(),
            self.tab,
            self.profile.clone(),
            self.attempt,
            events,
            None,
            false,
            target,
        )
        .expect("SSH monitor starts");
        (sender, task)
    }

    async fn wait(&self, predicate: impl Fn(&TerminalTabState) -> bool) {
        timeout(Duration::from_secs(3), async {
            loop {
                if self
                    .state
                    .lock()
                    .expect("application state")
                    .terminal(self.tab)
                    .is_some_and(&predicate)
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("controller state transition");
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.directory).expect("remove isolated monitor files");
    }
}

async fn finish(sender: mpsc::Sender<SshSessionEvent>, task: JoinHandle<()>) {
    drop(sender);
    timeout(Duration::from_secs(3), task)
        .await
        .expect("monitor shutdown deadline")
        .expect("monitor joins");
}

#[tokio::test]
async fn ssh_monitor_parses_output_handles_latency_and_preserves_connected_transport_on_auxiliary_errors()
 {
    let fixture = Fixture::new(ConnectionTarget::Terminal);
    let (sender, task) = fixture.monitor(ConnectionTarget::Terminal);
    for event in [
        SshSessionEvent::Authenticated,
        SshSessionEvent::Connected,
        SshSessionEvent::Output(TerminalOutputChunk::new(b"controller output\r\n".to_vec())),
        SshSessionEvent::Resized {
            columns: 1,
            rows: 1,
        },
        SshSessionEvent::LatencyMeasured {
            round_trip: Duration::from_millis(75),
            measured_at: std::time::Instant::now(),
        },
        SshSessionEvent::X11ForwardingEnabled,
        SshSessionEvent::X11ForwardingUnavailable("fixture X11 unavailable".into()),
        SshSessionEvent::MicrophoneUnavailable("fixture microphone unavailable".into()),
    ] {
        sender.send(event).await.expect("controller event");
    }
    fixture
        .wait(|terminal| terminal.status.contains("fixture microphone unavailable"))
        .await;
    {
        let mut app = fixture.state.lock().expect("application state");
        let terminal = app.terminal_mut(fixture.tab).expect("terminal retained");
        assert!(terminal.connected && terminal.worker_running);
        assert_eq!(
            terminal
                .connection_latency
                .snapshot(std::time::Instant::now())
                .milliseconds,
            75
        );
        let model = terminal.terminal.as_mut().expect("terminal model");
        assert!(
            model.size().columns() > 1,
            "late resize acknowledgement cannot shrink the grid"
        );
        let text = model
            .snapshot()
            .lines
            .iter()
            .flat_map(|line| line.runs.iter().map(|run| run.text.as_str()))
            .collect::<String>();
        assert!(text.contains("controller output"));
    }
    sender
        .send(SshSessionEvent::LatencyTimedOut)
        .await
        .expect("timeout");
    fixture
        .wait(|terminal| {
            terminal
                .connection_latency
                .snapshot(std::time::Instant::now())
                .state
                == "timeout"
        })
        .await;
    sender
        .send(SshSessionEvent::LatencyExpired)
        .await
        .expect("expiry");
    fixture
        .wait(|terminal| {
            terminal
                .connection_latency
                .snapshot(std::time::Instant::now())
                .state
                == "stale"
        })
        .await;
    sender
        .send(SshSessionEvent::LatencyUnavailable)
        .await
        .expect("unsupported latency");
    fixture
        .wait(|terminal| {
            terminal
                .connection_latency
                .snapshot(std::time::Instant::now())
                .state
                == "unavailable"
        })
        .await;
    finish(sender, task).await;
    let app = fixture.state.lock().expect("application state");
    let terminal = app.terminal(fixture.tab).expect("EOF preserves tab");
    assert!(!terminal.connected && !terminal.worker_running);
    assert!(terminal.terminal.is_some());
}

#[tokio::test]
async fn ssh_monitor_security_errors_require_retry_without_trusting_or_saving_credentials() {
    for event in [
        SshSessionEvent::HostKeyRejected {
            expected: None,
            actual: "SHA256:fixture".into(),
            public_key: None,
        },
        SshSessionEvent::HostKeyRejected {
            expected: Some("SHA256:previous".into()),
            actual: "SHA256:fixture".into(),
            public_key: None,
        },
        SshSessionEvent::HostKeyRevoked {
            actual: "SHA256:fixture".into(),
            public_key: None,
        },
        SshSessionEvent::AuthenticationFailed,
        SshSessionEvent::PrivateKeyFailed("fixture key unavailable".into()),
    ] {
        let fixture = Fixture::new(ConnectionTarget::Terminal);
        let host_key = matches!(
            event,
            SshSessionEvent::HostKeyRejected { .. } | SshSessionEvent::HostKeyRevoked { .. }
        );
        let revoked = matches!(event, SshSessionEvent::HostKeyRevoked { .. });
        let (sender, task) = fixture.monitor(ConnectionTarget::Terminal);
        sender.send(event).await.expect("security failure");
        finish(sender, task).await;
        let app = fixture.state.lock().expect("application state");
        let terminal = app.terminal(fixture.tab).expect("retry retains tab");
        assert!(!terminal.connected && !terminal.worker_running);
        if host_key {
            assert!(
                matches!(terminal.ssh_phase(), Some(SshConnectionPhase::AwaitingHostKey(prompt)) if prompt.revoked == revoked && prompt.fingerprint == "SHA256:fixture")
            );
        } else {
            assert!(matches!(
                terminal.ssh_phase(),
                Some(SshConnectionPhase::AwaitingAuthentication { .. })
            ));
        }
        assert!(
            app.sessions.sessions[0]
                .ssh()
                .expect("SSH profile")
                .host_key_fingerprint
                .is_none()
        );
        assert!(
            std::fs::read_dir(&fixture.directory)
                .expect("isolated files")
                .next()
                .is_none()
        );
    }
}

#[tokio::test]
async fn ssh_monitor_rejects_events_from_replaced_attempts() {
    let fixture = Fixture::new(ConnectionTarget::Terminal);
    let replacement = Uuid::new_v4();
    {
        let mut app = fixture.state.lock().expect("state");
        let terminal = app.terminal_mut(fixture.tab).expect("terminal");
        terminal.set_ssh_attempt(Some(replacement));
        terminal.status = "new attempt".into();
    }
    let (sender, task) = fixture.monitor(ConnectionTarget::Terminal);
    for event in [
        SshSessionEvent::Connected,
        SshSessionEvent::Output(TerminalOutputChunk::new(b"stale output".to_vec())),
        SshSessionEvent::HostKeyRevoked {
            actual: "old key".into(),
            public_key: None,
        },
        SshSessionEvent::Failed("old error".into()),
        SshSessionEvent::Disconnected,
    ] {
        sender.send(event).await.expect("late event");
    }
    finish(sender, task).await;
    let mut app = fixture.state.lock().expect("state");
    let terminal = app.terminal_mut(fixture.tab).expect("replacement retained");
    assert_eq!(
        terminal.ssh_route(),
        Some((fixture.profile.id, Some(replacement)))
    );
    assert_eq!(terminal.status, "new attempt");
    assert!(
        terminal
            .terminal
            .as_mut()
            .expect("terminal")
            .snapshot()
            .lines
            .iter()
            .all(|line| line.runs.iter().all(|run| !run.text.contains("stale")))
    );
}

#[tokio::test]
async fn sftp_monitor_routes_directory_transfer_and_write_events_to_the_current_tab() {
    let fixture = Fixture::new(ConnectionTarget::Sftp);
    let (sender, task) = fixture.monitor(ConnectionTarget::Sftp);
    let id = Uuid::new_v4();
    for event in [
        SshSessionEvent::Connected,
        SshSessionEvent::Sftp(SftpBrowserEvent::Opened {
            home: "/remote".into(),
        }),
        SshSessionEvent::Sftp(SftpBrowserEvent::DirectoryPage {
            request_id: None,
            path: "/remote".into(),
            entries: vec![],
            append: false,
            has_more: false,
            truncated: true,
        }),
        SshSessionEvent::SftpTransfer(SftpTransferEvent::Queued {
            transfer_id: id,
            remote_path: "/remote/file".into(),
            name: "file".into(),
            total_bytes: 100,
        }),
        SshSessionEvent::SftpTransfer(SftpTransferEvent::Started {
            transfer_id: id,
            remote_path: "/remote/file".into(),
            name: "file".into(),
            total_bytes: 100,
        }),
        SshSessionEvent::SftpTransfer(SftpTransferEvent::Progress {
            transfer_id: id,
            downloaded_bytes: 40,
            total_bytes: 100,
        }),
        SshSessionEvent::SftpTransfer(SftpTransferEvent::Paused {
            transfer_id: id,
            downloaded_bytes: 40,
            total_bytes: 100,
        }),
    ] {
        sender.send(event).await.expect("SFTP event");
    }
    fixture
        .wait(|terminal| {
            terminal
                .sftp
                .transfers
                .iter()
                .any(|transfer| transfer.id == id && transfer.phase == SftpTransferPhase::Paused)
        })
        .await;
    {
        let app = fixture.state.lock().expect("state");
        let sftp = &app.terminal(fixture.tab).expect("SFTP tab").sftp;
        assert_eq!(sftp.path, "/remote");
        assert!(sftp.truncated);
        assert_eq!(sftp.transfers[0].downloaded_bytes, 40);
    }
    for event in [
        SshSessionEvent::SftpTransfer(SftpTransferEvent::Resumed {
            transfer_id: id,
            downloaded_bytes: 40,
            total_bytes: 100,
        }),
        SshSessionEvent::SftpTransfer(SftpTransferEvent::Cancelled { transfer_id: id }),
        SshSessionEvent::SftpTransfer(SftpTransferEvent::DiscoveryFailed {
            transfer_id: Uuid::new_v4(),
            name: "unreadable".into(),
            message: "permission denied".into(),
        }),
        SshSessionEvent::SftpWrite(ax_ssh::sftp::SftpWriteEvent::Failed {
            operation_id: Uuid::new_v4(),
            message: "write rejected".into(),
        }),
    ] {
        sender.send(event).await.expect("terminal SFTP event");
    }
    fixture
        .wait(|terminal| terminal.sftp.status.contains("write rejected"))
        .await;
    {
        let app = fixture.state.lock().expect("state");
        let sftp = &app.terminal(fixture.tab).expect("SFTP tab").sftp;
        assert_eq!(sftp.transfers[0].phase, SftpTransferPhase::Cancelled);
        assert_eq!(sftp.transfers[1].phase, SftpTransferPhase::Failed);
    }
    finish(sender, task).await;
}

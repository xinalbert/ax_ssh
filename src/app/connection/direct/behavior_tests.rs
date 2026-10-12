use super::*;
use ax_ssh::terminal::TerminalOutputChunk;

#[tokio::test]
async fn direct_monitors_parse_output_and_finish_both_protocol_attempts() {
    for protocol in [DirectProtocol::Telnet, DirectProtocol::Serial] {
        for failure in [false, true] {
            let profile = match protocol {
                DirectProtocol::Telnet => SessionProfile::new_telnet("Fixture", "fixture.invalid"),
                DirectProtocol::Serial => SessionProfile::new_serial("Fixture", "injected-port"),
            };
            let mut app = AppState::new(
                ConfigStore::new(
                    std::env::temp_dir()
                        .join(format!("axssh-direct-monitor-{}.json", Uuid::new_v4())),
                ),
                SessionStore::default(),
            );
            let tab_id = app.open_terminal_tab(&profile);
            let attempt_id = Uuid::new_v4();
            let terminal = app.terminal_mut(tab_id).expect("terminal");
            match protocol {
                DirectProtocol::Telnet => terminal.set_telnet_attempt(Some(attempt_id)),
                DirectProtocol::Serial => terminal.set_serial_attempt(Some(attempt_id)),
            };
            terminal.reconnect_enabled = false;
            let state = Arc::new(Mutex::new(app));
            let (sender, receiver) = mpsc::channel(8);
            let task = spawn_direct_monitor(
                &Handle::current(),
                state.clone(),
                slint::Weak::default(),
                DirectAttempt {
                    tab_id,
                    profile,
                    attempt_id,
                    protocol,
                },
                receiver,
            );
            // Adapt actual transport event types through the same monitor adapters.
            let connected = match protocol {
                DirectProtocol::Telnet => DirectMonitorEvent::from(TelnetSessionEvent::Connected),
                DirectProtocol::Serial => DirectMonitorEvent::from(SerialSessionEvent::Connected {
                    port_name: "injected-port".into(),
                }),
            };
            sender.send(connected).await.expect("connected event");
            sender
                .send(DirectMonitorEvent::Output(TerminalOutputChunk::new(
                    b"direct output\r\n".to_vec(),
                )))
                .await
                .expect("output");
            sender
                .send(if failure {
                    DirectMonitorEvent::Failed("injected failure".into())
                } else {
                    DirectMonitorEvent::Disconnected
                })
                .await
                .expect("terminal event");
            drop(sender);
            timeout(Duration::from_secs(3), task)
                .await
                .expect("monitor deadline")
                .expect("monitor joins");
            let mut app = state.lock().expect("application state");
            let terminal = app.terminal_mut(tab_id).expect("tab retained");
            assert!(!terminal.connected && !terminal.worker_running);
            assert!(
                terminal
                    .telnet_route()
                    .is_none_or(|(_, attempt)| attempt.is_none())
            );
            assert!(
                terminal
                    .serial_route()
                    .is_none_or(|(_, attempt)| attempt.is_none())
            );
            // Connected re-arms the production reconnect policy. Both transport
            // errors and clean disconnects retire this attempt and schedule one retry.
            assert!(terminal.reconnecting);
            assert_eq!(terminal.reconnect_attempt, 1);
            assert_eq!(terminal.status, "Connection lost; reconnecting in 1s (1/5)");
            let text = terminal
                .terminal
                .as_mut()
                .expect("model")
                .snapshot()
                .lines
                .iter()
                .flat_map(|line| line.runs.iter().map(|run| run.text.as_str()))
                .collect::<String>();
            assert!(text.contains("direct output"));
        }
    }
}

#[tokio::test]
async fn direct_monitor_stream_end_retires_attempt_and_stale_events_do_not_retire_replacement() {
    for stale in [false, true] {
        let profile = SessionProfile::new_telnet("Fixture", "fixture.invalid");
        let mut app = AppState::new(
            ConfigStore::new(
                std::env::temp_dir().join(format!("axssh-direct-eof-{}.json", Uuid::new_v4())),
            ),
            SessionStore::default(),
        );
        let tab_id = app.open_terminal_tab(&profile);
        let attempt_id = Uuid::new_v4();
        let replacement = Uuid::new_v4();
        let terminal = app.terminal_mut(tab_id).expect("terminal");
        terminal.set_telnet_attempt(Some(if stale { replacement } else { attempt_id }));
        terminal.reconnect_enabled = false;
        terminal.status = "replacement".into();
        let state = Arc::new(Mutex::new(app));
        let (sender, receiver) = mpsc::channel(8);
        let task = spawn_direct_monitor(
            &Handle::current(),
            state.clone(),
            slint::Weak::default(),
            DirectAttempt {
                tab_id,
                profile: profile.clone(),
                attempt_id,
                protocol: DirectProtocol::Telnet,
            },
            receiver,
        );
        if stale {
            sender
                .send(TelnetSessionEvent::Connected)
                .await
                .expect("stale connected");
            sender
                .send(TelnetSessionEvent::Failed("old failure".into()))
                .await
                .expect("stale failure");
        }
        drop(sender);
        timeout(Duration::from_secs(3), task)
            .await
            .expect("monitor deadline")
            .expect("monitor joins");
        let app = state.lock().expect("state");
        let terminal = app.terminal(tab_id).expect("tab retained");
        if stale {
            assert_eq!(
                terminal.telnet_route(),
                Some((profile.id, Some(replacement)))
            );
            assert_eq!(terminal.status, "replacement");
        } else {
            assert_eq!(terminal.telnet_route(), Some((profile.id, None)));
            assert_eq!(terminal.status, "Telnet worker stopped");
        }
    }
}

use crate::app::state::TerminalWorker;
use std::sync::{Arc, Mutex};
use tokio::runtime::Handle;
use tokio::time::{Duration, timeout};

#[tokio::test]
async fn closed_tab_worker_is_drained_on_exit_and_disconnect_reaches_peer() {
    use crate::app::state::AppState;
    use ax_ssh::config::{ConfigStore, SessionProfile, SessionStore};
    use ax_ssh::telnet::{TelnetSessionEvent, TelnetSessionHandle};
    use tokio::io::AsyncReadExt;
    use uuid::Uuid;

    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let mut profile = SessionProfile::new_telnet("Fixture", "127.0.0.1");
    let ax_ssh::config::ConnectionProfile::Telnet(config) = &mut profile.connection else {
        panic!("Telnet config")
    };
    config.port = listener.local_addr().unwrap().port();
    let (worker, mut events) =
        TelnetSessionHandle::spawn(&Handle::current(), profile.id, config.clone(), 80, 24);
    let (mut peer, _) = timeout(Duration::from_secs(5), listener.accept())
        .await
        .unwrap()
        .unwrap();
    let mut negotiation = [0; 9];
    timeout(Duration::from_secs(5), peer.read_exact(&mut negotiation))
        .await
        .unwrap()
        .unwrap();
    let mut app = AppState::new(
        ConfigStore::new(std::env::temp_dir().join(format!("axssh-close-{}.json", Uuid::new_v4()))),
        SessionStore::default(),
    );
    let tab = app.open_terminal_tab(&profile);
    app.terminal_mut(tab).unwrap().worker = Some(TerminalWorker::Telnet(worker));
    let state = Arc::new(Mutex::new(app));
    crate::app::workspace::close_workspace_tab(
        tab,
        &state,
        &slint::Weak::default(),
        &Handle::current(),
    );
    let mut shutdowns = {
        let mut app = state.lock().unwrap();
        assert!(app.terminal(tab).is_none());
        assert!(
            app.drain_runtime_resources().0.is_empty(),
            "worker has already left tab state"
        );
        assert!(
            !app.worker_shutdowns.is_empty(),
            "tab cleanup must still have an owner"
        );
        std::mem::take(&mut app.worker_shutdowns)
    };
    shutdowns.reap();
    assert!(shutdowns.finish(Duration::from_secs(5)).await);
    let mut byte = [0];
    assert_eq!(
        timeout(Duration::from_secs(5), peer.read(&mut byte))
            .await
            .unwrap()
            .unwrap(),
        0
    );
    let mut disconnected = false;
    while let Some(event) = events.recv().await {
        if matches!(event, TelnetSessionEvent::Disconnected) {
            disconnected = true;
        }
    }
    assert!(
        disconnected,
        "worker must report graceful disconnect before exit"
    );
}

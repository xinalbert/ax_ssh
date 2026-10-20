use super::*;
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

#[tokio::test]
async fn immediate_exit_waits_for_already_started_cleanup() {
    let mut shutdowns = WorkerShutdowns::default();
    let (release, wait) = oneshot::channel();
    let (finished, observed) = oneshot::channel();
    shutdowns.tasks.spawn(async move {
        wait.await.unwrap();
        finished.send(()).unwrap();
        Ok(())
    });
    let finish = tokio::spawn(shutdowns.finish(Duration::from_secs(5)));
    tokio::task::yield_now().await;
    assert!(!finish.is_finished());
    release.send(()).unwrap();
    assert!(finish.await.unwrap());
    observed.await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn deadline_aborts_and_joins_remaining_cleanup_even_after_another_task_fails() {
    struct Lease(Arc<std::sync::atomic::AtomicBool>);
    impl Drop for Lease {
        fn drop(&mut self) {
            self.0.store(true, std::sync::atomic::Ordering::Release);
        }
    }
    let dropped = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let lease = Lease(dropped.clone());
    let mut shutdowns = WorkerShutdowns::default();
    shutdowns
        .tasks
        .spawn(async { anyhow::bail!("injected shutdown failure") });
    shutdowns.tasks.spawn(async move {
        let _lease = lease;
        std::future::pending::<()>().await;
        Ok(())
    });
    let finish = tokio::spawn(shutdowns.finish(Duration::from_secs(25)));
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_secs(25)).await;
    assert!(!finish.await.unwrap());
    assert!(dropped.load(std::sync::atomic::Ordering::Acquire));
}

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
            !app.worker_shutdowns.tasks.is_empty(),
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

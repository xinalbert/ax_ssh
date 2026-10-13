use super::super::{ConfigStore, SessionProfile, SessionStore};
use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Default)]
struct Backend {
    events: Option<std_mpsc::SyncSender<()>>,
    watches: Vec<PathBuf>,
    unwatches: Vec<PathBuf>,
    failures: usize,
    unwatch_failure: bool,
}

struct MockWatcher {
    backend: Arc<Mutex<Backend>>,
    drops: Arc<AtomicUsize>,
}

impl DirectoryWatcher for MockWatcher {
    fn watch(&mut self, path: &Path) -> notify::Result<()> {
        let mut backend = self.backend.lock().unwrap();
        backend.watches.push(path.to_owned());
        if backend.failures > 0 {
            backend.failures -= 1;
            Err(notify::Error::generic("injected registration failure"))
        } else {
            Ok(())
        }
    }

    fn unwatch(&mut self, path: &Path) -> notify::Result<()> {
        let mut backend = self.backend.lock().unwrap();
        backend.unwatches.push(path.to_owned());
        if backend.unwatch_failure {
            Err(notify::Error::generic("injected release failure"))
        } else {
            Ok(())
        }
    }
}

impl Drop for MockWatcher {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}

struct Fixture {
    state: Arc<Mutex<AppState>>,
    tab: Uuid,
    path: String,
    backend: Arc<Mutex<Backend>>,
    drops: Arc<AtomicUsize>,
    refreshes: Arc<Mutex<Vec<(u64, String)>>>,
}

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir()
            .join(format!("axssh-watch-{}", Uuid::new_v4()))
            .to_string_lossy()
            .into_owned();
        let mut app = AppState::new(
            ConfigStore::new(Path::new(&path).join("config.json")),
            SessionStore::default(),
        );
        let profile = SessionProfile::new("Fixture", "fixture.invalid", "fixture");
        let tab = app.open_sftp_tab(&profile);
        app.terminal_mut(tab)
            .unwrap()
            .sftp
            .local
            .complete(path.clone(), Vec::new(), false, 0);
        Self {
            state: Arc::new(Mutex::new(app)),
            tab,
            path,
            backend: Arc::default(),
            drops: Arc::default(),
            refreshes: Arc::default(),
        }
    }

    fn spawn(&self) -> Option<tokio::task::JoinHandle<()>> {
        let backend = self.backend.clone();
        let drops = self.drops.clone();
        let refreshes = self.refreshes.clone();
        spawn_directory_watch(
            &Handle::current(),
            self.state.clone(),
            self.tab,
            move |events| {
                backend.lock().unwrap().events = Some(events);
                Ok(MockWatcher { backend, drops })
            },
            move |id, path| refreshes.lock().unwrap().push((id, path)),
        )
    }

    fn notify(&self) {
        let events = self.backend.lock().unwrap().events.clone().unwrap();
        record_change(&events, Ok(notify::Event::new(EventKind::Any)));
    }

    fn complete(&self, path: String) {
        self.state
            .lock()
            .unwrap()
            .terminal_mut(self.tab)
            .unwrap()
            .sftp
            .local
            .complete(path, Vec::new(), false, 0);
    }
}

async fn poll_tasks() {
    for _ in 0..6 {
        tokio::task::yield_now().await;
    }
}

async fn advance(duration: Duration) {
    tokio::time::advance(duration).await;
    poll_tasks().await;
}

#[test]
fn notifications_ignore_access_and_coalesce_changes_and_backend_errors() {
    let (sender, receiver) = std_mpsc::sync_channel(1);
    record_change(
        &sender,
        Ok(notify::Event::new(EventKind::Access(
            notify::event::AccessKind::Any,
        ))),
    );
    assert!(receiver.try_recv().is_err());
    for _ in 0..1000 {
        record_change(&sender, Ok(notify::Event::new(EventKind::Any)));
    }
    assert!(receiver.try_recv().is_ok());
    assert!(receiver.try_recv().is_err());
    record_change(&sender, Err(notify::Error::generic("backend lost events")));
    assert!(receiver.try_recv().is_ok());
    drop(receiver);
    record_change(&sender, Ok(notify::Event::new(EventKind::Any)));
}

#[tokio::test(start_paused = true)]
async fn watcher_debounces_bursts_and_exits_when_its_tab_closes() {
    let fixture = Fixture::new();
    let task = fixture.spawn().unwrap();
    poll_tasks().await;
    assert!(fixture.spawn().is_none(), "one watcher per tab");
    assert_eq!(
        fixture.backend.lock().unwrap().watches,
        [PathBuf::from(&fixture.path)]
    );
    for _ in 0..1000 {
        fixture.notify();
    }
    advance(WATCH_CHECK_INTERVAL).await;
    assert!(fixture.refreshes.lock().unwrap().is_empty());
    advance(EVENT_DEBOUNCE).await;
    assert_eq!(
        fixture.refreshes.lock().unwrap().as_slice(),
        &[(1, fixture.path.clone())]
    );
    for _ in 0..20 {
        fixture.notify();
    }
    advance(WATCH_CHECK_INTERVAL).await;
    advance(EVENT_DEBOUNCE).await;
    assert_eq!(
        fixture.refreshes.lock().unwrap().len(),
        1,
        "in-flight listing is preserved"
    );
    fixture.complete(fixture.path.clone());
    assert_eq!(
        fixture
            .state
            .lock()
            .unwrap()
            .terminal_mut(fixture.tab)
            .unwrap()
            .sftp
            .local
            .begin_pending_auto_refresh()
            .unwrap(),
        Some((2, fixture.path.clone())),
        "changes during a listing request exactly one follow-up"
    );
    assert!(
        fixture
            .state
            .lock()
            .unwrap()
            .close_tab(fixture.tab)
            .is_some()
    );
    advance(WATCH_CHECK_INTERVAL).await;
    task.await.unwrap();
    assert_eq!(fixture.drops.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn directory_switch_discards_queued_events_and_rejects_changes_during_debounce() {
    let fixture = Fixture::new();
    let task = fixture.spawn().unwrap();
    poll_tasks().await;
    fixture.notify();
    let next = Path::new(&fixture.path)
        .join("next")
        .to_string_lossy()
        .into_owned();
    fixture.complete(next.clone());
    fixture.backend.lock().unwrap().unwatch_failure = true;
    advance(WATCH_CHECK_INTERVAL).await;
    assert_eq!(
        fixture.backend.lock().unwrap().unwatches,
        [PathBuf::from(&fixture.path)]
    );
    assert_eq!(fixture.backend.lock().unwrap().watches.len(), 2);
    advance(EVENT_DEBOUNCE).await;
    assert!(
        fixture.refreshes.lock().unwrap().is_empty(),
        "old queued event cannot refresh new path"
    );

    fixture.notify();
    advance(WATCH_CHECK_INTERVAL).await;
    fixture.complete(fixture.path.clone());
    advance(EVENT_DEBOUNCE).await;
    assert!(
        fixture.refreshes.lock().unwrap().is_empty(),
        "revalidate path after debounce"
    );
    advance(WATCH_CHECK_INTERVAL).await;
    fixture.notify();
    advance(WATCH_CHECK_INTERVAL).await;
    advance(EVENT_DEBOUNCE).await;
    assert_eq!(fixture.refreshes.lock().unwrap().len(), 1);
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert!(
        fixture
            .state
            .lock()
            .unwrap()
            .terminal_mut(fixture.tab)
            .unwrap()
            .sftp
            .local
            .mark_watcher_running()
    );
}

#[tokio::test(start_paused = true)]
async fn failed_registration_retries_at_deadline_and_new_path_retries_immediately() {
    let fixture = Fixture::new();
    fixture.backend.lock().unwrap().failures = 2;
    let task = fixture.spawn().unwrap();
    poll_tasks().await;
    assert_eq!(fixture.backend.lock().unwrap().watches.len(), 1);
    fixture.notify();
    advance(WATCH_CHECK_INTERVAL).await;
    advance(EVENT_DEBOUNCE).await;
    assert!(fixture.refreshes.lock().unwrap().is_empty());
    assert_eq!(fixture.backend.lock().unwrap().watches.len(), 1);
    advance(WATCH_RETRY_INTERVAL - WATCH_CHECK_INTERVAL - EVENT_DEBOUNCE).await;
    assert_eq!(fixture.backend.lock().unwrap().watches.len(), 2);
    let next = Path::new(&fixture.path)
        .join("next")
        .to_string_lossy()
        .into_owned();
    fixture.complete(next.clone());
    advance(WATCH_CHECK_INTERVAL).await;
    assert_eq!(
        fixture.backend.lock().unwrap().watches.last(),
        Some(&PathBuf::from(&next))
    );
    fixture.notify();
    advance(WATCH_CHECK_INTERVAL).await;
    advance(EVENT_DEBOUNCE).await;
    assert_eq!(fixture.refreshes.lock().unwrap().as_slice(), &[(1, next)]);
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
}

#[tokio::test(start_paused = true)]
async fn creation_failure_and_unpolled_cancellation_release_the_registration() {
    let fixture = Fixture::new();
    spawn_directory_watch::<MockWatcher>(
        &Handle::current(),
        fixture.state.clone(),
        fixture.tab,
        |_| Err(notify::Error::generic("backend unavailable")),
        |_, _| panic!("no refresh"),
    )
    .unwrap()
    .await
    .unwrap();
    let task = fixture.spawn().expect("creation failure permits restart");
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert_eq!(
        fixture.drops.load(Ordering::SeqCst),
        0,
        "native backend was never created"
    );
    let task = fixture
        .spawn()
        .expect("unpolled cancellation permits restart");
    poll_tasks().await;
    fixture
        .state
        .lock()
        .unwrap()
        .terminal_mut(fixture.tab)
        .unwrap()
        .sftp
        .local
        .loaded = false;
    advance(WATCH_CHECK_INTERVAL).await;
    task.await.unwrap();
    assert!(
        fixture.spawn().is_none(),
        "unloaded directories cannot acquire a watcher"
    );
    let mut app = fixture.state.lock().unwrap();
    let local = app.open_local_shell_tab();
    drop(app);
    assert!(current_local_directory(&fixture.state, Uuid::new_v4()).is_none());
    assert!(queue_local_change_refresh(&fixture.state, local, &fixture.path).is_none());
}

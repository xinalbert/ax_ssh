use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[tokio::test]
async fn dropped_receipt_does_not_cancel_registered_cleanup() {
    let mut shutdowns = WorkerShutdowns::default();
    let (release, wait) = oneshot::channel();
    let finished = Arc::new(AtomicBool::new(false));
    let observed = finished.clone();
    drop(shutdowns.start(&Handle::current(), async move {
        wait.await?;
        finished.store(true, Ordering::Release);
        Ok(())
    }));
    assert!(!shutdowns.is_empty());
    let finish = tokio::spawn(shutdowns.finish(Duration::from_secs(5)));
    tokio::task::yield_now().await;
    assert!(!finish.is_finished());
    release.send(()).unwrap();
    assert!(finish.await.unwrap());
    assert!(observed.load(Ordering::Acquire));
}

#[tokio::test(start_paused = true)]
async fn deadline_aborts_and_joins_remaining_cleanup_even_after_another_task_fails() {
    struct Lease(Arc<AtomicBool>);
    impl Drop for Lease {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Release);
        }
    }
    let dropped = Arc::new(AtomicBool::new(false));
    let lease = Lease(dropped.clone());
    let mut shutdowns = WorkerShutdowns::default();
    drop(shutdowns.start(&Handle::current(), async {
        anyhow::bail!("injected shutdown failure")
    }));
    drop(shutdowns.start(&Handle::current(), async move {
        let _lease = lease;
        std::future::pending::<()>().await;
        Ok(())
    }));
    let finish = tokio::spawn(shutdowns.finish(Duration::from_secs(25)));
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_secs(25)).await;
    assert!(!finish.await.unwrap());
    assert!(dropped.load(Ordering::Acquire));
}

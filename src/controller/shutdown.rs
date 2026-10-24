//! Retain cleanup ownership after tab removal and drain it at process exit.

use std::future::Future;

use anyhow::Result;
use tokio::runtime::Handle;
use tokio::sync::oneshot;
use tokio::task::JoinSet;
use tokio::time::{Duration, timeout};
use tracing::warn;

/// Tracks shutdown futures independently of either UI framework or live tabs.
#[derive(Default)]
pub struct WorkerShutdowns {
    tasks: JoinSet<Result<()>>,
}

impl WorkerShutdowns {
    #[cfg(feature = "egui-ui")]
    pub(crate) fn pending(&self) -> usize {
        self.tasks.len()
    }

    /// Registers cleanup before its session leaves application state.
    ///
    /// The returned receipt observes completion but does not own or cancel the
    /// task. Dropping it leaves cleanup in this registry.
    pub fn start(
        &mut self,
        runtime: &Handle,
        cleanup: impl Future<Output = Result<()>> + Send + 'static,
    ) -> oneshot::Receiver<()> {
        self.reap();
        let (done, completion) = oneshot::channel();
        self.tasks.spawn_on(
            async move {
                let result = cleanup.await;
                if done.send(()).is_err() {
                    tracing::trace!("worker cleanup retained by shutdown registry");
                }
                result
            },
            runtime,
        );
        completion
    }

    /// Reaps completed tasks without waiting on the caller's thread.
    pub fn reap(&mut self) {
        while let Some(result) = self.tasks.try_join_next() {
            report(result);
        }
    }

    /// Whether any registered task still needs to be reaped or joined.
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    /// Drains cleanup, then aborts and joins any tasks past the deadline.
    ///
    /// Returns false only when the deadline expired. Individual task errors are
    /// reported while the remaining tasks continue to drain.
    pub async fn finish(mut self, deadline: Duration) -> bool {
        if timeout(deadline, async {
            while let Some(result) = self.tasks.join_next().await {
                report(result);
            }
        })
        .await
        .is_ok()
        {
            return true;
        }
        self.tasks.abort_all();
        while let Some(result) = self.tasks.join_next().await {
            if !matches!(&result, Err(error) if error.is_cancelled()) {
                report(result);
            }
        }
        false
    }
}

fn report(result: std::result::Result<Result<()>, tokio::task::JoinError>) {
    match result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => warn!(%error, "failed to shut down terminal worker cleanly"),
        Err(error) => warn!(%error, "terminal worker shutdown task failed"),
    }
}

#[cfg(test)]
mod tests;

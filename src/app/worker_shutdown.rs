//! Own terminal cleanup independently of visible tabs, including during exit.

use anyhow::Result;
use tokio::runtime::Handle;
use tokio::sync::oneshot;
use tokio::task::JoinSet;
use tokio::time::{Duration, timeout};
use tracing::warn;

use super::state::TerminalWorker;

#[derive(Default)]
pub(super) struct WorkerShutdowns {
    tasks: JoinSet<Result<()>>,
}

impl WorkerShutdowns {
    pub(super) fn start(
        &mut self,
        runtime: &Handle,
        worker: TerminalWorker,
    ) -> oneshot::Receiver<()> {
        self.reap();
        let (done, completion) = oneshot::channel();
        self.tasks.spawn_on(
            async move {
                let result = worker.shutdown().await;
                // Tab close and process exit wait through the JoinSet. Workspace
                // replacement also keeps a receipt without taking task ownership.
                if done.send(()).is_err() {
                    tracing::trace!("worker cleanup retained by shutdown registry");
                }
                result
            },
            runtime,
        );
        completion
    }

    pub(super) fn reap(&mut self) {
        while let Some(result) = self.tasks.try_join_next() {
            report(result);
        }
    }

    /// The same owner drains tasks started by tab close and by process exit.
    pub(super) async fn finish(mut self, deadline: Duration) -> bool {
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

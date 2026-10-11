//! One bounded, ordered write worker per authenticated SFTP connection.

use std::future::Future;

use super::*;
use crate::sftp::SftpWriteResult;
use tokio::task::JoinHandle;

const WRITE_QUEUE_CAPACITY: usize = 16;

pub(super) struct SftpWriteWorker {
    commands: mpsc::Sender<(Uuid, SftpWriteOperation)>,
    cancellation: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<()>>,
}

impl SftpWriteWorker {
    pub(super) fn spawn(
        connection: SshConnection,
        session_id: Uuid,
        events: mpsc::Sender<SshSessionEvent>,
    ) -> Self {
        Self::spawn_with_operation(session_id, events, move |operation| {
            let connection = connection.clone();
            async move {
                let stream = connection
                    .open_sftp_stream()
                    .await
                    .context("cannot open SFTP write subsystem")?;
                execute_sftp_write(stream, operation).await
            }
        })
    }

    fn spawn_with_operation<F, Fut>(
        session_id: Uuid,
        events: mpsc::Sender<SshSessionEvent>,
        mut execute: F,
    ) -> Self
    where
        F: FnMut(SftpWriteOperation) -> Fut + Send + 'static,
        Fut: Future<Output = Result<SftpWriteResult>> + Send,
    {
        let (commands, mut receiver) = mpsc::channel(WRITE_QUEUE_CAPACITY);
        let (cancellation, cancelled) = oneshot::channel();
        let task = tokio::spawn(async move {
            tokio::select! {
                biased;
                _ = cancelled => {}
                _ = async {
                    while let Some((operation_id, operation)) = receiver.recv().await {
                        let event = write_event(operation_id, execute(operation).await);
                        if !send_event(&events, SshSessionEvent::SftpWrite(event), session_id).await {
                            break;
                        }
                    }
                } => {}
            }
        });
        Self {
            commands,
            cancellation: Some(cancellation),
            task: Some(task),
        }
    }

    pub(super) fn request(&self, id: Uuid, operation: SftpWriteOperation) -> Result<()> {
        self.commands
            .try_send((id, operation))
            .map_err(|error| anyhow::anyhow!("cannot queue SFTP write operation: {error}"))
    }

    pub(super) async fn shutdown(mut self) -> Result<()> {
        if let Some(cancel) = self.cancellation.take() {
            let _ = cancel.send(());
        }
        let Some(mut task) = self.task.take() else {
            return Ok(());
        };
        match timeout(SFTP_OPEN_SHUTDOWN_TIMEOUT, &mut task).await {
            Ok(result) => result.context("SFTP write worker failed during shutdown"),
            Err(_) => {
                task.abort();
                match task.await {
                    Err(error) if error.is_cancelled() => Ok(()),
                    result => result.context("failed to stop SFTP write worker"),
                }
            }
        }
    }
}

impl Drop for SftpWriteWorker {
    fn drop(&mut self) {
        if let Some(cancel) = self.cancellation.take() {
            let _ = cancel.send(());
        }
        if let Some(task) = self.task.as_ref() {
            task.abort();
        }
    }
}

fn write_event(operation_id: Uuid, result: Result<SftpWriteResult>) -> SftpWriteEvent {
    match result {
        Ok(SftpWriteResult::Completed { path }) => SftpWriteEvent::Completed { operation_id, path },
        Ok(SftpWriteResult::Updated {
            path,
            size,
            modified,
        }) => SftpWriteEvent::Updated {
            operation_id,
            path,
            size,
            modified,
        },
        Ok(SftpWriteResult::EditFile {
            path,
            data,
            expected_size,
            expected_modified,
        }) => SftpWriteEvent::EditFile {
            operation_id,
            path,
            data,
            expected_size,
            expected_modified,
        },
        Ok(SftpWriteResult::Metadata {
            path,
            size,
            modified,
        }) => SftpWriteEvent::Metadata {
            operation_id,
            path,
            size,
            modified,
        },
        Err(error) => SftpWriteEvent::Failed {
            operation_id,
            message: bounded_error_message(&error),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn pending_write_does_not_block_requests_and_shutdown_cancels_it() {
        let (events, _receiver) = mpsc::channel(1);
        let started = Arc::new(tokio::sync::Notify::new());
        let observed = started.clone();
        let worker = SftpWriteWorker::spawn_with_operation(Uuid::new_v4(), events, move |_| {
            observed.notify_one();
            std::future::pending::<Result<SftpWriteResult>>()
        });
        let operation = || SftpWriteOperation::CheckMetadata {
            path: "/srv/file".into(),
        };
        worker
            .request(Uuid::new_v4(), operation())
            .expect("first request");
        started.notified().await;
        for _ in 0..WRITE_QUEUE_CAPACITY {
            worker
                .request(Uuid::new_v4(), operation())
                .expect("bounded queued request");
        }
        assert!(worker.request(Uuid::new_v4(), operation()).is_err());
        timeout(Duration::from_secs(1), worker.shutdown())
            .await
            .expect("prompt shutdown")
            .expect("joined worker");
    }

    #[tokio::test]
    async fn writes_report_results_in_user_order() {
        let (events, mut receiver) = mpsc::channel(4);
        let worker =
            SftpWriteWorker::spawn_with_operation(Uuid::new_v4(), events, |operation| async move {
                let SftpWriteOperation::CheckMetadata { path } = operation else {
                    anyhow::bail!("unexpected operation");
                };
                Ok(SftpWriteResult::Completed { path })
            });
        for path in ["/first", "/second"] {
            worker
                .request(
                    Uuid::new_v4(),
                    SftpWriteOperation::CheckMetadata { path: path.into() },
                )
                .expect("request");
        }
        for expected in ["/first", "/second"] {
            let event = receiver.recv().await.expect("result");
            assert!(
                matches!(event, SshSessionEvent::SftpWrite(SftpWriteEvent::Completed { path, .. }) if path == expected)
            );
        }
        worker.shutdown().await.expect("shutdown");
    }
}

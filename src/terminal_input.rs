use std::sync::mpsc::{SyncSender, TrySendError};

use anyhow::Result;
use tokio::sync::mpsc;

/// Queue lossy pointer motion without consuming the last Tokio command slot.
pub(crate) fn try_queue_tokio_motion<T>(
    sender: &mpsc::Sender<T>,
    command: T,
    closed_error: &str,
) -> Result<bool> {
    if sender.capacity() <= 1 {
        if sender.is_closed() {
            anyhow::bail!("{closed_error}");
        }
        return Ok(false);
    }
    match sender.try_send(command) {
        Ok(()) => Ok(true),
        Err(mpsc::error::TrySendError::Full(_)) => Ok(false),
        Err(mpsc::error::TrySendError::Closed(_)) => anyhow::bail!("{closed_error}"),
    }
}

/// Queue lossy pointer motion for the local PTY's bounded synchronous channel.
pub(crate) fn try_queue_sync_motion<T>(
    sender: &SyncSender<T>,
    command: T,
    closed_error: &str,
) -> Result<bool> {
    match sender.try_send(command) {
        Ok(()) => Ok(true),
        Err(TrySendError::Full(_)) => Ok(false),
        Err(TrySendError::Disconnected(_)) => anyhow::bail!("{closed_error}"),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::sync_channel;

    use super::*;

    #[test]
    fn tokio_motion_preserves_one_reliable_slot_and_reports_closed() {
        let (sender, receiver) = mpsc::channel(2);

        assert!(
            try_queue_tokio_motion(&sender, 1, "worker stopped")
                .expect("first motion should queue")
        );
        assert!(
            !try_queue_tokio_motion(&sender, 2, "worker stopped")
                .expect("reserved-slot backpressure should be lossy")
        );

        drop(receiver);
        assert!(try_queue_tokio_motion(&sender, 3, "worker stopped").is_err());
    }

    #[test]
    fn sync_motion_drops_when_full_and_reports_disconnected() {
        let (sender, receiver) = sync_channel(1);
        sender.try_send(1).expect("test queue should accept input");

        assert!(
            !try_queue_sync_motion(&sender, 2, "worker stopped")
                .expect("full synchronous queue should be lossy")
        );

        drop(receiver);
        assert!(try_queue_sync_motion(&sender, 3, "worker stopped").is_err());
    }
}

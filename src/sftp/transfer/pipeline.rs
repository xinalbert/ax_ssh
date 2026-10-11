//! Ordered replies with a strict bound including completed, buffered replies.

use std::collections::VecDeque;
use std::future::{Future, poll_fn};
use std::pin::Pin;
use std::task::{Context, Poll};

use anyhow::Result;

pub(in crate::sftp) const REQUEST_WINDOW: usize = 16;

type RequestFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

struct PendingRequest<'a, T> {
    future: Option<RequestFuture<'a, T>>,
    result: Option<Result<T>>,
}

pub(in crate::sftp) struct RequestWindow<'a, T> {
    pending: VecDeque<PendingRequest<'a, T>>,
}

impl<'a, T> RequestWindow<'a, T> {
    pub(in crate::sftp) fn new() -> Self {
        Self {
            pending: VecDeque::with_capacity(REQUEST_WINDOW),
        }
    }

    pub(in crate::sftp) fn len(&self) -> usize {
        self.pending.len()
    }

    pub(in crate::sftp) async fn push<F>(&mut self, future: F) -> Result<()>
    where
        F: Future<Output = Result<T>> + Send + 'a,
    {
        if self.pending.len() >= REQUEST_WINDOW {
            anyhow::bail!("SFTP request window is full");
        }
        self.pending.push_back(PendingRequest {
            future: Some(Box::pin(future)),
            result: None,
        });
        // Start requests immediately, including while local input or a shared
        // rate limiter delays filling the rest of the window.
        poll_fn(|cx| {
            self.poll_requests(cx);
            Poll::Ready(())
        })
        .await;
        Ok(())
    }

    fn poll_requests(&mut self, cx: &mut Context<'_>) {
        for request in &mut self.pending {
            if let Some(future) = request.future.as_mut()
                && let Poll::Ready(result) = future.as_mut().poll(cx)
            {
                request.future = None;
                request.result = Some(result);
            }
        }
    }

    pub(in crate::sftp) async fn next(&mut self) -> Option<Result<T>> {
        poll_fn(|cx| {
            self.poll_requests(cx);
            match self.pending.front_mut() {
                None => Poll::Ready(None),
                Some(first) => match first.result.take() {
                    Some(result) => {
                        self.pending.pop_front();
                        Poll::Ready(Some(result))
                    }
                    None => Poll::Pending,
                },
            }
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::oneshot;

    #[tokio::test]
    async fn out_of_order_replies_stay_bounded_and_are_emitted_in_order() {
        let mut window = RequestWindow::new();
        let mut senders = Vec::new();
        for index in 0..REQUEST_WINDOW {
            let (sender, receiver) = oneshot::channel();
            senders.push(sender);
            window
                .push(async move { receiver.await.map_err(anyhow::Error::from).map(|()| index) })
                .await
                .expect("bounded request");
        }
        assert!(window.push(async { Ok(99) }).await.is_err());
        for sender in senders.into_iter().rev() {
            sender.send(()).expect("receiver is alive");
        }
        for expected in 0..REQUEST_WINDOW {
            assert_eq!(
                window.next().await.expect("reply").expect("success"),
                expected
            );
        }
        assert!(window.next().await.is_none());
    }

    #[tokio::test]
    async fn dropping_window_cancels_every_in_flight_request() {
        let mut window = RequestWindow::new();
        let mut senders = Vec::new();
        for _ in 0..REQUEST_WINDOW {
            let (sender, receiver) = oneshot::channel::<()>();
            senders.push(sender);
            window
                .push(async move { receiver.await.map_err(anyhow::Error::from) })
                .await
                .expect("request");
        }
        drop(window);
        assert!(senders.iter().all(oneshot::Sender::is_closed));
    }
}

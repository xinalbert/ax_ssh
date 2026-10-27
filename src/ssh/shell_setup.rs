//! Confirm the sole outstanding X11 request without losing early terminal bytes.

use super::*;

const MAX_SETUP_MESSAGES: usize = 64;
const MAX_SETUP_OUTPUT_BYTES: usize = 64 * 1024;

#[derive(Default)]
struct Reply {
    messages: usize,
    output: Vec<u8>,
}

impl Reply {
    fn accept(&mut self, message: Option<ChannelMsg>) -> Result<Option<bool>> {
        self.messages += 1;
        anyhow::ensure!(
            self.messages <= MAX_SETUP_MESSAGES,
            "too many messages before X11 confirmation"
        );
        match message {
            Some(ChannelMsg::Success) => Ok(Some(true)),
            Some(ChannelMsg::Failure) => Ok(Some(false)),
            Some(ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. }) => {
                anyhow::ensure!(
                    self.output.len().saturating_add(data.len()) <= MAX_SETUP_OUTPUT_BYTES,
                    "terminal output exceeded the X11 setup limit"
                );
                self.output.extend_from_slice(&data);
                Ok(None)
            }
            Some(ChannelMsg::Eof | ChannelMsg::Close) | None => {
                anyhow::bail!("SSH channel closed before X11 confirmation")
            }
            Some(_) => Ok(None),
        }
    }
}

pub(super) async fn request_x11(
    channel: &mut Channel<client::Msg>,
    forwarding: &x11::X11Forwarding,
) -> Result<(X11RequestStatus, Vec<u8>)> {
    let mut reply = Reply::default();
    let confirmed = timeout(CONNECT_TIMEOUT, async {
        let cookie = forwarding.fake_cookie_hex();
        channel
            .request_x11(
                true,
                false,
                x11::X11_AUTH_PROTOCOL,
                cookie.as_str(),
                forwarding.screen(),
            )
            .await
            .context("cannot send X11 forwarding request")?;
        loop {
            if let Some(accepted) = reply.accept(channel.wait().await)? {
                return Ok::<_, anyhow::Error>(accepted);
            }
        }
    })
    .await;
    let status = match confirmed {
        Ok(Ok(true)) => X11RequestStatus::Enabled,
        Ok(Ok(false)) => X11RequestStatus::Rejected,
        Ok(Err(error)) => return Err(error),
        Err(_) => X11RequestStatus::Rejected,
    };
    Ok((status, reply.output))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x11_reply_preserves_early_output_and_requires_explicit_success() {
        for accepted in [true, false] {
            let mut reply = Reply::default();
            assert_eq!(
                reply
                    .accept(Some(ChannelMsg::Data {
                        data: b"early".as_slice().into()
                    }))
                    .unwrap(),
                None
            );
            assert_eq!(
                reply
                    .accept(Some(ChannelMsg::ExtendedData {
                        data: b"stderr".as_slice().into(),
                        ext: 1
                    }))
                    .unwrap(),
                None
            );
            assert_eq!(
                reply
                    .accept(Some(if accepted {
                        ChannelMsg::Success
                    } else {
                        ChannelMsg::Failure
                    }))
                    .unwrap(),
                Some(accepted)
            );
            assert_eq!(reply.output, b"earlystderr");
        }
    }

    #[test]
    fn x11_reply_bounds_output_messages_and_rejects_closed_channels() {
        let mut reply = Reply::default();
        assert!(
            reply
                .accept(Some(ChannelMsg::Data {
                    data: vec![0; MAX_SETUP_OUTPUT_BYTES + 1].into()
                }))
                .is_err()
        );
        assert!(reply.output.is_empty());
        let mut reply = Reply::default();
        for _ in 0..MAX_SETUP_MESSAGES {
            assert_eq!(
                reply
                    .accept(Some(ChannelMsg::WindowAdjusted { new_size: 1 }))
                    .unwrap(),
                None
            );
        }
        assert!(reply.accept(Some(ChannelMsg::Success)).is_err());
        for message in [None, Some(ChannelMsg::Eof), Some(ChannelMsg::Close)] {
            assert!(Reply::default().accept(message).is_err());
        }
    }
}

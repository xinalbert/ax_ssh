//! SOCKS5 CONNECT negotiation for the loopback listener.

use std::net::{Ipv4Addr, Ipv6Addr};

use anyhow::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::{ForwardTarget, LocalIo, validate_target};

pub(super) async fn request(socket: &mut impl LocalIo) -> Result<ForwardTarget> {
    let [version, count] = read_bytes(socket).await?;
    anyhow::ensure!(version == 5 && count != 0, "invalid SOCKS5 greeting");
    let mut methods = [0; 255];
    let methods = &mut methods[..usize::from(count)];
    socket.read_exact(methods).await?;
    let supports_no_auth = methods.contains(&0);
    socket
        .write_all(&[5, if supports_no_auth { 0 } else { 255 }])
        .await?;
    anyhow::ensure!(
        supports_no_auth,
        "SOCKS5 client requires unsupported authentication"
    );

    let [version, command, reserved, address_type] = read_bytes(socket).await?;
    if version != 5 || reserved != 0 {
        reply(socket, 1).await?;
        anyhow::bail!("invalid SOCKS5 request");
    }
    if command != 1 {
        reply(socket, 7).await?;
        anyhow::bail!("only SOCKS5 CONNECT is supported");
    }
    let host = match address_type {
        1 => Ipv4Addr::from(read_bytes::<4>(socket).await?).to_string(),
        4 => Ipv6Addr::from(read_bytes::<16>(socket).await?).to_string(),
        3 => {
            let length = usize::from(socket.read_u8().await?);
            let mut name = [0; 255];
            socket.read_exact(&mut name[..length]).await?;
            match std::str::from_utf8(&name[..length]) {
                Ok(name) if name.is_ascii() => name.to_owned(),
                _ => {
                    reply(socket, 8).await?;
                    anyhow::bail!("invalid SOCKS5 host");
                }
            }
        }
        _ => {
            reply(socket, 8).await?;
            anyhow::bail!("unsupported SOCKS5 address type");
        }
    };
    let port = socket.read_u16().await?;
    let target = ForwardTarget::Tcp { host, port };
    if let Err(error) = validate_target(&target, false) {
        reply(socket, 8).await?;
        return Err(error);
    }
    Ok(target)
}

pub(super) async fn reply(socket: &mut impl LocalIo, code: u8) -> Result<()> {
    // SSH direct-tcpip does not return the remote socket's bind address.
    socket.write_all(&[5, code, 0, 1, 0, 0, 0, 0, 0, 0]).await?;
    Ok(())
}

async fn read_bytes<const N: usize>(socket: &mut impl LocalIo) -> Result<[u8; N]> {
    let mut bytes = [0; N];
    socket.read_exact(&mut bytes).await?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rejects_unsupported_socks5_methods_and_udp() {
        let (mut client, mut server) = tokio::io::duplex(64);
        let task = tokio::spawn(async move { request(&mut server).await });
        client.write_all(&[5, 1, 2]).await.unwrap();
        assert_eq!(read_bytes::<2>(&mut client).await.unwrap(), [5, 255]);
        assert!(task.await.unwrap().is_err());

        let (mut client, mut server) = tokio::io::duplex(64);
        let task = tokio::spawn(async move { request(&mut server).await });
        client.write_all(&[5, 1, 0, 5, 3, 0, 1]).await.unwrap();
        assert_eq!(read_bytes::<2>(&mut client).await.unwrap(), [5, 0]);
        assert_eq!(read_bytes::<10>(&mut client).await.unwrap()[1], 7);
        assert!(task.await.unwrap().is_err());
    }

    #[tokio::test]
    async fn parses_ipv6_socks5_target() {
        let (mut client, mut server) = tokio::io::duplex(64);
        let task = tokio::spawn(async move { request(&mut server).await });
        client.write_all(&[5, 1, 0, 5, 1, 0, 4]).await.unwrap();
        client
            .write_all(&Ipv6Addr::LOCALHOST.octets())
            .await
            .unwrap();
        client.write_u16(8080).await.unwrap();
        assert_eq!(
            task.await.unwrap().unwrap(),
            ForwardTarget::Tcp {
                host: "::1".to_owned(),
                port: 8080
            }
        );
    }
}

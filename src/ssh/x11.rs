//! Secure, bounded X11 forwarding for one SSH terminal worker.

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result};
use russh::client;
use russh::{Channel, ChannelOpenFailure};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
#[cfg(unix)]
use tokio::net::UnixStream;
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use tokio::time::{Duration, sleep, timeout};
use zeroize::Zeroizing;

use crate::config::X11Settings;
use crate::x_server::XServerPlan;

pub(super) const X11_AUTH_PROTOCOL: &str = "MIT-MAGIC-COOKIE-1";
const X11_COOKIE_BYTES: usize = 16;
const X11_CHANNEL_CAPACITY: usize = 8;
const XAUTH_TIMEOUT: Duration = Duration::from_secs(3);
const ENDPOINT_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const CHANNEL_ACCEPT_TIMEOUT: Duration = Duration::from_secs(2);
const SETUP_TIMEOUT: Duration = Duration::from_secs(5);
const SERVER_START_TIMEOUT: Duration = Duration::from_secs(10);
const SERVER_START_POLL_INTERVAL: Duration = Duration::from_millis(250);
const MAX_DISPLAY_CHARS: usize = 1_024;
const MAX_XAUTH_OUTPUT_BYTES: usize = 4 * 1_024;
const MAX_AUTH_PROTOCOL_BYTES: usize = 64;
const MAX_AUTH_DATA_BYTES: usize = 256;
const X11_SETUP_HEADER_BYTES: usize = 12;

trait AsyncReadWrite: AsyncRead + AsyncWrite + Unpin + Send {}

impl<T> AsyncReadWrite for T where T: AsyncRead + AsyncWrite + Unpin + Send {}

type LocalX11Stream = Box<dyn AsyncReadWrite>;

#[derive(Clone, Debug, PartialEq, Eq)]
enum X11Endpoint {
    Unix(PathBuf),
    Tcp { host: &'static str, port: u16 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DisplayTarget {
    query: String,
    screen: u32,
    endpoints: Vec<X11Endpoint>,
}

impl DisplayTarget {
    fn parse(display: &str) -> Result<Self> {
        let display = display.trim();
        if display.is_empty()
            || display.chars().count() > MAX_DISPLAY_CHARS
            || display.chars().any(char::is_control)
        {
            anyhow::bail!("DISPLAY is missing or invalid");
        }

        let (host, display_suffix) = display
            .rsplit_once(':')
            .context("DISPLAY must include a display number")?;
        let (display_number, screen) = match display_suffix.split_once('.') {
            Some((display_number, screen)) => (display_number, screen),
            None => (display_suffix, "0"),
        };
        let display_number = display_number
            .parse::<u16>()
            .context("DISPLAY contains an invalid display number")?;
        let screen = screen
            .parse::<u32>()
            .context("DISPLAY contains an invalid screen number")?;
        let tcp_port = 6_000u16
            .checked_add(display_number)
            .context("DISPLAY number is too large")?;

        let normalized_host = host.strip_prefix("tcp/").unwrap_or(host);
        let unix_display = normalized_host.is_empty() || normalized_host == "unix";
        let launchd_socket = normalized_host.starts_with('/');
        let loopback_display =
            matches!(normalized_host, "localhost" | "127.0.0.1" | "::1" | "[::1]");
        if !unix_display && !launchd_socket && !loopback_display {
            anyhow::bail!("DISPLAY must identify a local X server");
        }

        let mut endpoints = Vec::new();
        #[cfg(unix)]
        {
            if launchd_socket {
                endpoints.push(X11Endpoint::Unix(PathBuf::from(normalized_host)));
            }
            if unix_display || launchd_socket {
                endpoints.push(X11Endpoint::Unix(PathBuf::from(format!(
                    "/tmp/.X11-unix/X{display_number}"
                ))));
                endpoints.push(X11Endpoint::Unix(PathBuf::from(format!(
                    "/private/tmp/.X11-unix/X{display_number}"
                ))));
            }
        }
        endpoints.push(X11Endpoint::Tcp {
            host: if normalized_host == "::1" || normalized_host == "[::1]" {
                "::1"
            } else {
                "127.0.0.1"
            },
            port: tcp_port,
        });
        endpoints.dedup();

        Ok(Self {
            query: display.to_owned(),
            screen,
            endpoints,
        })
    }
}

enum LocalX11Auth {
    Cookie(Zeroizing<Vec<u8>>),
    None,
}

struct X11Credentials {
    fake_cookie: Zeroizing<[u8; X11_COOKIE_BYTES]>,
    local_auth: LocalX11Auth,
}

#[derive(Clone)]
pub(super) struct X11Session {
    endpoint: X11Endpoint,
    screen: u32,
    credentials: Arc<X11Credentials>,
}

impl X11Session {
    pub(super) async fn prepare(settings: X11Settings) -> Result<Self> {
        let plan = XServerPlan::resolve(settings).await?;
        let initial_error = match prepare_existing_server(&plan, None).await {
            Ok(session) => return Ok(session),
            Err(error) => error,
        };
        if !plan.launch_on_connect() {
            return Err(initial_error)
                .context("local X server is not ready and automatic startup is disabled");
        }

        let _launch_lock = X_SERVER_LAUNCH_LOCK
            .get_or_init(|| tokio::sync::Mutex::new(()))
            .lock()
            .await;
        if let Ok(session) = prepare_existing_server(&plan, None).await {
            return Ok(session);
        }
        let launched_display = plan.launch().await?;
        let mut last_error = None;
        let result = timeout(SERVER_START_TIMEOUT, async {
            loop {
                match prepare_existing_server(&plan, Some(&launched_display)).await {
                    Ok(session) => return session,
                    Err(error) => last_error = Some(error),
                }
                sleep(SERVER_START_POLL_INTERVAL).await;
            }
        })
        .await;
        match result {
            Ok(session) => Ok(session),
            Err(_) => Err(last_error.unwrap_or(initial_error))
                .context("local X server did not become ready before its timeout"),
        }
    }

    pub(super) fn screen(&self) -> u32 {
        self.screen
    }

    pub(super) fn fake_cookie_hex(&self) -> Zeroizing<String> {
        Zeroizing::new(encode_hex(self.credentials.fake_cookie.as_ref()))
    }

    pub(super) async fn relay(&self, request: X11ChannelRequest) -> Result<()> {
        let X11ChannelRequest { channel, reply } = request;
        let mut local = match connect_endpoint(&self.endpoint).await {
            Ok(stream) => stream,
            Err(error) => {
                reply.reject(ChannelOpenFailure::ConnectFailed).await;
                return Err(error).context("cannot connect to the local X server");
            }
        };

        timeout(CHANNEL_ACCEPT_TIMEOUT, reply.accept())
            .await
            .context("timed out accepting an SSH X11 channel")?;
        let mut remote = channel.into_stream();
        timeout(
            SETUP_TIMEOUT,
            relay_authenticated_setup(
                &mut remote,
                local.as_mut(),
                self.credentials.fake_cookie.as_ref(),
                &self.credentials.local_auth,
            ),
        )
        .await
        .context("timed out validating X11 authentication setup")??;
        tokio::io::copy_bidirectional(&mut remote, local.as_mut())
            .await
            .context("X11 bidirectional relay failed")?;
        Ok(())
    }
}

static X_SERVER_LAUNCH_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

async fn prepare_existing_server(
    plan: &XServerPlan,
    preferred_display: Option<&str>,
) -> Result<X11Session> {
    let mut displays = Vec::new();
    if let Some(display) = preferred_display {
        displays.push(display.to_owned());
    }
    for display in plan.display_candidates().await {
        if !displays.contains(&display) {
            displays.push(display);
        }
    }
    let mut last_error = None;
    for display in displays {
        match prepare_display(plan, &display).await {
            Ok(session) => return Ok(session),
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("no local X display candidate is available")))
}

async fn prepare_display(plan: &XServerPlan, display: &str) -> Result<X11Session> {
    let display = DisplayTarget::parse(display)?;
    let endpoint = probe_endpoints(&display.endpoints).await?;
    let local_auth = match load_xauth_cookie(&display.query).await {
        Ok(cookie) => LocalX11Auth::Cookie(Zeroizing::new(cookie)),
        Err(_error) if plan.allow_no_auth() => LocalX11Auth::None,
        Err(error) => return Err(error),
    };
    let fake_cookie = Zeroizing::new(rand::random::<[u8; X11_COOKIE_BYTES]>());
    Ok(X11Session {
        endpoint,
        screen: display.screen,
        credentials: Arc::new(X11Credentials {
            fake_cookie,
            local_auth,
        }),
    })
}

pub(super) struct X11ChannelRequest {
    channel: Channel<client::Msg>,
    reply: client::ChannelOpenHandle,
}

impl X11ChannelRequest {
    pub(super) async fn reject(self, reason: ChannelOpenFailure) {
        self.reply.reject(reason).await;
    }
}

#[derive(Clone)]
pub(super) struct X11Dispatcher {
    sender: mpsc::Sender<X11ChannelRequest>,
    enabled: Arc<AtomicBool>,
}

impl X11Dispatcher {
    pub(super) fn channel() -> (Self, mpsc::Receiver<X11ChannelRequest>) {
        let (sender, receiver) = mpsc::channel(X11_CHANNEL_CAPACITY);
        (
            Self {
                sender,
                enabled: Arc::new(AtomicBool::new(false)),
            },
            receiver,
        )
    }

    pub(super) fn enable(&self) {
        self.enabled.store(true, Ordering::Release);
    }

    pub(super) fn disable(&self) {
        self.enabled.store(false, Ordering::Release);
    }

    pub(super) async fn dispatch(
        &self,
        channel: Channel<client::Msg>,
        reply: client::ChannelOpenHandle,
    ) {
        if !self.enabled.load(Ordering::Acquire) {
            reply
                .reject(ChannelOpenFailure::AdministrativelyProhibited)
                .await;
            return;
        }

        let request = X11ChannelRequest { channel, reply };
        match self.sender.try_send(request) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(request)) => {
                request
                    .reply
                    .reject(ChannelOpenFailure::ResourceShortage)
                    .await;
            }
            Err(mpsc::error::TrySendError::Closed(request)) => {
                request
                    .reply
                    .reject(ChannelOpenFailure::AdministrativelyProhibited)
                    .await;
            }
        }
    }
}

async fn load_xauth_cookie(display: &str) -> Result<Vec<u8>> {
    for executable in xauth_candidates() {
        let Ok(output) = run_xauth(&executable, display).await else {
            continue;
        };
        if let Ok(cookie) = parse_xauth_output(&output) {
            return Ok(cookie);
        }
    }
    anyhow::bail!("no exact MIT-MAGIC-COOKIE-1 authorization is available")
}

fn xauth_candidates() -> Vec<PathBuf> {
    let mut candidates = vec![PathBuf::from("xauth")];
    #[cfg(target_os = "macos")]
    {
        candidates.push(PathBuf::from("/opt/X11/bin/xauth"));
        candidates.push(PathBuf::from("/usr/X11/bin/xauth"));
    }
    candidates
}

async fn run_xauth(executable: &PathBuf, display: &str) -> Result<Vec<u8>> {
    let mut child = Command::new(executable)
        .arg("list")
        .arg(display)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .context("failed to start xauth")?;
    let stdout = child
        .stdout
        .take()
        .context("xauth stdout was not captured")?;
    let mut output = Vec::new();
    let mut limited = stdout.take((MAX_XAUTH_OUTPUT_BYTES + 1) as u64);
    match timeout(XAUTH_TIMEOUT, limited.read_to_end(&mut output)).await {
        Ok(Ok(_)) => {}
        Ok(Err(error)) => {
            terminate_child(&mut child).await;
            return Err(error).context("failed to read xauth output");
        }
        Err(_) => {
            terminate_child(&mut child).await;
            anyhow::bail!("xauth output timed out");
        }
    }
    if output.len() > MAX_XAUTH_OUTPUT_BYTES {
        terminate_child(&mut child).await;
        anyhow::bail!("xauth output exceeded its size limit");
    }
    let status = match timeout(XAUTH_TIMEOUT, child.wait()).await {
        Ok(status) => status.context("failed to wait for xauth")?,
        Err(_) => {
            terminate_child(&mut child).await;
            anyhow::bail!("xauth did not exit before its timeout");
        }
    };
    if !status.success() {
        anyhow::bail!("xauth did not find authorization for this display");
    }
    Ok(output)
}

async fn terminate_child(child: &mut Child) {
    let _ = child.kill().await;
    let _ = child.wait().await;
}

fn parse_xauth_output(output: &[u8]) -> Result<Vec<u8>> {
    let output = std::str::from_utf8(output).context("xauth output is not UTF-8")?;
    let mut selected: Option<Vec<u8>> = None;
    for line in output.lines().filter(|line| !line.trim().is_empty()) {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() != 3 || fields[1] != X11_AUTH_PROTOCOL {
            continue;
        }
        let cookie = decode_cookie(fields[2])?;
        if selected.is_some() {
            anyhow::bail!("xauth returned ambiguous authorization entries");
        }
        selected = Some(cookie);
    }
    selected.context("xauth returned no MIT-MAGIC-COOKIE-1 entry")
}

fn decode_cookie(value: &str) -> Result<Vec<u8>> {
    if value.len() != X11_COOKIE_BYTES * 2 || !value.is_ascii() {
        anyhow::bail!("X11 authorization cookie has an invalid length");
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = decode_hex_nibble(pair[0])?;
            let low = decode_hex_nibble(pair[1])?;
            Ok((high << 4) | low)
        })
        .collect()
}

fn decode_hex_nibble(value: u8) -> Result<u8> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => anyhow::bail!("X11 authorization cookie is not hexadecimal"),
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

async fn probe_endpoints(endpoints: &[X11Endpoint]) -> Result<X11Endpoint> {
    for endpoint in endpoints {
        if connect_endpoint(endpoint).await.is_ok() {
            return Ok(endpoint.clone());
        }
    }
    anyhow::bail!("no reachable local X server endpoint was found")
}

async fn connect_endpoint(endpoint: &X11Endpoint) -> Result<LocalX11Stream> {
    timeout(ENDPOINT_CONNECT_TIMEOUT, async {
        match endpoint {
            X11Endpoint::Unix(path) => {
                #[cfg(unix)]
                {
                    let stream = UnixStream::connect(path).await?;
                    Ok::<LocalX11Stream, anyhow::Error>(Box::new(stream))
                }
                #[cfg(not(unix))]
                {
                    let _ = path;
                    anyhow::bail!("Unix X11 sockets are unavailable on this platform")
                }
            }
            X11Endpoint::Tcp { host, port } => {
                let stream = TcpStream::connect((*host, *port)).await?;
                Ok::<LocalX11Stream, anyhow::Error>(Box::new(stream))
            }
        }
    })
    .await
    .context("local X server connection timed out")?
}

async fn relay_authenticated_setup<R, W>(
    remote: &mut R,
    local: &mut W,
    fake_cookie: &[u8],
    local_auth: &LocalX11Auth,
) -> Result<()>
where
    R: AsyncRead + Unpin + ?Sized,
    W: AsyncWrite + Unpin + ?Sized,
{
    let mut setup = read_setup_packet(remote).await?;
    rewrite_setup_packet(&mut setup, fake_cookie, local_auth)?;
    local
        .write_all(&setup)
        .await
        .context("failed to send authenticated setup to the local X server")?;
    local.flush().await.context("failed to flush X11 setup")?;
    Ok(())
}

async fn read_setup_packet<R>(reader: &mut R) -> Result<Zeroizing<Vec<u8>>>
where
    R: AsyncRead + Unpin + ?Sized,
{
    let mut header = [0u8; X11_SETUP_HEADER_BYTES];
    reader
        .read_exact(&mut header)
        .await
        .context("failed to read the X11 setup header")?;
    let byte_order = header[0];
    let auth_protocol_len = read_u16(&header[6..8], byte_order)? as usize;
    let auth_data_len = read_u16(&header[8..10], byte_order)? as usize;
    validate_auth_lengths(auth_protocol_len, auth_data_len)?;
    let total_len = setup_packet_len(auth_protocol_len, auth_data_len)?;
    let mut packet = Zeroizing::new(vec![0u8; total_len]);
    packet[..X11_SETUP_HEADER_BYTES].copy_from_slice(&header);
    reader
        .read_exact(&mut packet[X11_SETUP_HEADER_BYTES..])
        .await
        .context("failed to read the X11 authentication setup")?;
    Ok(packet)
}

fn rewrite_setup_packet(
    packet: &mut Zeroizing<Vec<u8>>,
    fake_cookie: &[u8],
    local_auth: &LocalX11Auth,
) -> Result<()> {
    if packet.len() < X11_SETUP_HEADER_BYTES {
        anyhow::bail!("X11 setup packet is truncated");
    }
    let byte_order = packet[0];
    let auth_protocol_len = read_u16(&packet[6..8], byte_order)? as usize;
    let auth_data_len = read_u16(&packet[8..10], byte_order)? as usize;
    validate_auth_lengths(auth_protocol_len, auth_data_len)?;
    let expected_len = setup_packet_len(auth_protocol_len, auth_data_len)?;
    if packet.len() != expected_len {
        anyhow::bail!("X11 setup packet has an invalid length");
    }

    let protocol_start = X11_SETUP_HEADER_BYTES;
    let protocol_end = protocol_start + auth_protocol_len;
    if &packet[protocol_start..protocol_end] != X11_AUTH_PROTOCOL.as_bytes() {
        anyhow::bail!("X11 setup uses an unsupported authorization protocol");
    }
    let data_start = protocol_start + padded_len(auth_protocol_len)?;
    let data_end = data_start + auth_data_len;
    if fake_cookie.len() != auth_data_len || &packet[data_start..data_end] != fake_cookie {
        anyhow::bail!("X11 setup authorization was rejected");
    }
    match local_auth {
        LocalX11Auth::Cookie(real_cookie) if real_cookie.len() == auth_data_len => {
            packet[data_start..data_end].copy_from_slice(real_cookie);
        }
        LocalX11Auth::Cookie(_) => {
            anyhow::bail!("local X11 authorization cookie has an invalid length");
        }
        LocalX11Auth::None => {
            packet[6..10].fill(0);
            packet.truncate(X11_SETUP_HEADER_BYTES);
        }
    }
    Ok(())
}

fn validate_auth_lengths(protocol_len: usize, data_len: usize) -> Result<()> {
    if protocol_len == 0
        || protocol_len > MAX_AUTH_PROTOCOL_BYTES
        || data_len == 0
        || data_len > MAX_AUTH_DATA_BYTES
    {
        anyhow::bail!("X11 setup authorization fields exceed their limits");
    }
    Ok(())
}

fn setup_packet_len(protocol_len: usize, data_len: usize) -> Result<usize> {
    X11_SETUP_HEADER_BYTES
        .checked_add(padded_len(protocol_len)?)
        .and_then(|length| length.checked_add(padded_len(data_len).ok()?))
        .context("X11 setup packet length overflow")
}

fn padded_len(length: usize) -> Result<usize> {
    length
        .checked_add(3)
        .map(|length| length & !3)
        .context("X11 setup field length overflow")
}

fn read_u16(bytes: &[u8], byte_order: u8) -> Result<u16> {
    let bytes: [u8; 2] = bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("X11 setup integer is truncated"))?;
    match byte_order {
        b'l' => Ok(u16::from_le_bytes(bytes)),
        b'B' => Ok(u16::from_be_bytes(bytes)),
        _ => anyhow::bail!("X11 setup uses an invalid byte order"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAKE_COOKIE: [u8; X11_COOKIE_BYTES] = [0x11; X11_COOKIE_BYTES];
    const REAL_COOKIE: [u8; X11_COOKIE_BYTES] = [0xaa; X11_COOKIE_BYTES];

    #[test]
    fn display_parsing_accepts_only_local_endpoints() {
        let unix = DisplayTarget::parse(":2.1").expect("local Unix display should parse");
        assert_eq!(unix.screen, 1);
        assert!(unix.endpoints.iter().any(|endpoint| {
            matches!(
                endpoint,
                X11Endpoint::Tcp {
                    host: "127.0.0.1",
                    port: 6002
                }
            )
        }));

        let ipv6 = DisplayTarget::parse("[::1]:3").expect("loopback display should parse");
        assert_eq!(
            ipv6.endpoints.last(),
            Some(&X11Endpoint::Tcp {
                host: "::1",
                port: 6003,
            })
        );
        assert!(DisplayTarget::parse("remote.example:0").is_err());
        assert!(DisplayTarget::parse(":65535").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn launchd_display_prefers_its_socket_path() {
        let display = DisplayTarget::parse("/private/tmp/launchd/org.xquartz:0")
            .expect("launchd display should parse");
        assert_eq!(
            display.endpoints.first(),
            Some(&X11Endpoint::Unix(PathBuf::from(
                "/private/tmp/launchd/org.xquartz"
            )))
        );
    }

    #[test]
    fn exact_xauth_output_is_parsed_without_global_fallback() {
        let output = b"host/unix:0  MIT-MAGIC-COOKIE-1  00112233445566778899aabbccddeeff\n";
        assert_eq!(
            parse_xauth_output(output).expect("cookie should parse"),
            vec![
                0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
                0xee, 0xff,
            ]
        );
        assert!(parse_xauth_output(b"host/unix:0  OTHER-AUTH  0011\n").is_err());
        assert!(parse_xauth_output(b"host/unix:0 MIT-MAGIC-COOKIE-1 0011\n").is_err());
        assert!(
            parse_xauth_output(
                b"host/unix:0 MIT-MAGIC-COOKIE-1 00112233445566778899aabbccddeeff\n\
                  host/unix:0 MIT-MAGIC-COOKIE-1 00112233445566778899aabbccddeeff\n"
            )
            .is_err()
        );
    }

    #[test]
    fn setup_cookie_is_rewritten_for_both_byte_orders() {
        for byte_order in [b'l', b'B'] {
            let mut packet = Zeroizing::new(setup_packet(byte_order, &FAKE_COOKIE));
            let real_auth = LocalX11Auth::Cookie(Zeroizing::new(REAL_COOKIE.to_vec()));
            rewrite_setup_packet(&mut packet, &FAKE_COOKIE, &real_auth)
                .expect("valid cookie should be rewritten");
            let data_start = X11_SETUP_HEADER_BYTES + padded_len(X11_AUTH_PROTOCOL.len()).unwrap();
            assert_eq!(
                &packet[data_start..data_start + X11_COOKIE_BYTES],
                &REAL_COOKIE
            );
        }
    }

    #[test]
    fn setup_rejects_invalid_order_lengths_protocol_and_cookie() {
        let mut invalid_order = Zeroizing::new(setup_packet(b'l', &FAKE_COOKIE));
        invalid_order[0] = b'?';
        let real_auth = LocalX11Auth::Cookie(Zeroizing::new(REAL_COOKIE.to_vec()));
        assert!(rewrite_setup_packet(&mut invalid_order, &FAKE_COOKIE, &real_auth).is_err());

        let mut oversized = Zeroizing::new(setup_packet(b'l', &FAKE_COOKIE));
        oversized[6..8].copy_from_slice(&u16::MAX.to_le_bytes());
        assert!(rewrite_setup_packet(&mut oversized, &FAKE_COOKIE, &real_auth).is_err());

        let mut invalid_protocol = Zeroizing::new(setup_packet(b'l', &FAKE_COOKIE));
        invalid_protocol[X11_SETUP_HEADER_BYTES] = b'X';
        assert!(rewrite_setup_packet(&mut invalid_protocol, &FAKE_COOKIE, &real_auth).is_err());

        let mut wrong_cookie = Zeroizing::new(setup_packet(b'l', &[0x22; X11_COOKIE_BYTES]));
        assert!(rewrite_setup_packet(&mut wrong_cookie, &FAKE_COOKIE, &real_auth).is_err());
    }

    #[test]
    fn explicit_no_auth_rewrite_strips_local_authorization_fields() {
        for byte_order in [b'l', b'B'] {
            let mut packet = Zeroizing::new(setup_packet(byte_order, &FAKE_COOKIE));
            rewrite_setup_packet(&mut packet, &FAKE_COOKIE, &LocalX11Auth::None)
                .expect("explicit compatibility mode should strip local auth");
            assert_eq!(packet.len(), X11_SETUP_HEADER_BYTES);
            assert_eq!(&packet[6..10], &[0, 0, 0, 0]);
        }
    }

    fn setup_packet(byte_order: u8, cookie: &[u8; X11_COOKIE_BYTES]) -> Vec<u8> {
        let protocol = X11_AUTH_PROTOCOL.as_bytes();
        let total = setup_packet_len(protocol.len(), cookie.len()).unwrap();
        let mut packet = vec![0u8; total];
        packet[0] = byte_order;
        let protocol_len = protocol.len() as u16;
        let cookie_len = cookie.len() as u16;
        let (protocol_len, cookie_len) = if byte_order == b'l' {
            (protocol_len.to_le_bytes(), cookie_len.to_le_bytes())
        } else {
            (protocol_len.to_be_bytes(), cookie_len.to_be_bytes())
        };
        packet[6..8].copy_from_slice(&protocol_len);
        packet[8..10].copy_from_slice(&cookie_len);
        packet[X11_SETUP_HEADER_BYTES..X11_SETUP_HEADER_BYTES + protocol.len()]
            .copy_from_slice(protocol);
        let data_start = X11_SETUP_HEADER_BYTES + padded_len(protocol.len()).unwrap();
        packet[data_start..data_start + cookie.len()].copy_from_slice(cookie);
        packet
    }
}

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
fn untrusted_xauth_generation_uses_a_private_authority_file_and_expiry() {
    assert_eq!(
        untrusted_xauth_generate_arguments("/private/authority", "unix:0"),
        [
            "-f",
            "/private/authority",
            "generate",
            "unix:0",
            X11_AUTH_PROTOCOL,
            "untrusted",
            "timeout",
            UNTRUSTED_X11_COOKIE_TIMEOUT_SECONDS,
        ]
    );
}

#[cfg(unix)]
#[test]
fn private_xauth_directory_is_created_owner_only() {
    let directory = PrivateXauthDirectory::create().expect("private directory should be created");
    let permissions = fs::metadata(&directory.path)
        .expect("private directory metadata should be available")
        .permissions();
    assert_eq!(
        std::os::unix::fs::PermissionsExt::mode(&permissions) & 0o777,
        0o700
    );
}

#[test]
fn setup_cookie_is_rewritten_for_both_byte_orders() {
    for byte_order in *b"lB" {
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
    for byte_order in *b"lB" {
        let mut packet = Zeroizing::new(setup_packet(byte_order, &FAKE_COOKIE));
        rewrite_setup_packet(&mut packet, &FAKE_COOKIE, &LocalX11Auth::None)
            .expect("explicit compatibility mode should strip local auth");
        assert_eq!(packet.len(), X11_SETUP_HEADER_BYTES);
        assert_eq!(&packet[6..10], &[0, 0, 0, 0]);
    }
}

#[tokio::test]
async fn setup_gate_rejects_foreign_cookies_and_malformed_packets_before_preparation() {
    for byte_order in *b"lB" {
        let valid = setup_packet(byte_order, &FAKE_COOKIE);
        let mut invalid_order = valid.clone();
        invalid_order[0] = b'?';
        let mut oversized = valid.clone();
        oversized[6..8].fill(0xff);
        let mut invalid_protocol = valid.clone();
        invalid_protocol[X11_SETUP_HEADER_BYTES] = b'X';
        let cases = [
            Vec::new(),
            valid[..X11_SETUP_HEADER_BYTES - 1].to_vec(),
            valid[..valid.len() - 1].to_vec(),
            invalid_order,
            oversized,
            invalid_protocol,
            setup_packet(byte_order, &REAL_COOKIE),
        ];
        for packet in cases {
            let calls = std::cell::Cell::new(0);
            let result =
                authenticate_then_prepare(&mut packet.as_slice(), &FAKE_COOKIE, || async {
                    calls.set(calls.get() + 1);
                    Ok(())
                })
                .await;
            let error = result.expect_err("invalid setup must fail before local preparation");
            assert!(!error.is::<X11PreparationError>());
            assert_eq!(calls.get(), 0);
        }
    }
}

#[tokio::test]
async fn setup_gate_waits_for_the_complete_cookie_at_every_byte_split() {
    for byte_order in *b"lB" {
        let packet = setup_packet(byte_order, &FAKE_COOKIE);
        for split in 0..=packet.len() {
            let (mut sender, mut remote) = tokio::io::duplex(128);
            sender.write_all(&packet[..split]).await.unwrap();
            let calls = std::cell::Cell::new(0);
            let mut gate = Box::pin(authenticate_then_prepare(
                &mut remote,
                &FAKE_COOKIE,
                || async {
                    calls.set(calls.get() + 1);
                    Ok(())
                },
            ));
            if split < packet.len() {
                tokio::select! {
                    biased;
                    _ = &mut gate => panic!("partial setup must not finish authentication"),
                    _ = std::future::ready(()) => {}
                }
                assert_eq!(calls.get(), 0);
                sender.write_all(&packet[split..]).await.unwrap();
            }
            let (setup, ()) = gate.await.expect("complete matching setup should pass");
            assert_eq!(*setup, packet);
            assert_eq!(calls.get(), 1);
        }
    }
}

#[tokio::test(start_paused = true)]
async fn setup_timeout_does_not_prepare_a_local_server() {
    let (mut sender, mut remote) = tokio::io::duplex(128);
    sender.write_all(b"l").await.unwrap();
    let calls = std::cell::Cell::new(0);
    let started = Instant::now();
    let error = authenticate_then_prepare(&mut remote, &FAKE_COOKIE, || async {
        calls.set(calls.get() + 1);
        Ok(())
    })
    .await
    .expect_err("incomplete authentication must time out");
    assert_eq!(
        error.to_string(),
        "timed out validating X11 authentication setup"
    );
    assert_eq!(Instant::now() - started, SETUP_TIMEOUT);
    assert_eq!(calls.get(), 0);
}

#[tokio::test]
async fn cancelling_setup_does_not_prepare_a_local_server() {
    let (_sender, mut remote) = tokio::io::duplex(128);
    let calls = std::cell::Cell::new(0);
    let mut gate = Box::pin(authenticate_then_prepare(
        &mut remote,
        &FAKE_COOKIE,
        || async {
            calls.set(calls.get() + 1);
            Ok(())
        },
    ));
    tokio::select! {
        biased;
        _ = &mut gate => panic!("empty setup must remain pending"),
        _ = std::future::ready(()) => {}
    }
    drop(gate);
    assert_eq!(calls.get(), 0);
}

#[tokio::test]
async fn authenticated_relay_preserves_setup_and_following_bytes_in_both_directions() {
    for byte_order in *b"lB" {
        for no_auth in [false, true] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("fake local X server should bind");
            let endpoint = X11Endpoint::Tcp {
                host: "127.0.0.1",
                port: listener.local_addr().unwrap().port(),
            };
            let local_auth = if no_auth {
                LocalX11Auth::None
            } else {
                LocalX11Auth::Cookie(Zeroizing::new(REAL_COOKIE.to_vec()))
            };
            let packet = setup_packet(byte_order, &FAKE_COOKIE);
            let mut expected = if no_auth {
                let mut header = packet[..X11_SETUP_HEADER_BYTES].to_vec();
                header[6..10].fill(0);
                header
            } else {
                setup_packet(byte_order, &REAL_COOKIE)
            };
            expected.extend_from_slice(b"following X11 request");
            let (mut application, mut remote) = tokio::io::duplex(256);
            let calls = std::cell::Cell::new(0);
            let relay = async {
                let (setup, session) =
                    authenticate_then_prepare(&mut remote, &FAKE_COOKIE, || async {
                        calls.set(calls.get() + 1);
                        Ok(X11Session {
                            endpoint,
                            credentials: Arc::new(X11Credentials {
                                fake_cookie: Zeroizing::new(FAKE_COOKIE),
                                local_auth,
                            }),
                        })
                    })
                    .await
                    .expect("matching cookie should permit local preparation");
                session.relay(&mut remote, setup).await.unwrap();
            };
            let application = async {
                let mut bytes = packet;
                bytes.extend_from_slice(b"following X11 request");
                application.write_all(&bytes).await.unwrap();
                application.shutdown().await.unwrap();
                let mut response = Vec::new();
                application.read_to_end(&mut response).await.unwrap();
                assert_eq!(response, b"X11 server response");
            };
            let server = async {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                stream.read_to_end(&mut bytes).await.unwrap();
                assert_eq!(bytes, expected);
                stream.write_all(b"X11 server response").await.unwrap();
                stream.shutdown().await.unwrap();
            };
            timeout(Duration::from_secs(2), async {
                tokio::join!(relay, application, server);
            })
            .await
            .expect("authenticated relay should finish without losing bytes or EOF");
            assert_eq!(calls.get(), 1);
        }
    }
}

fn setup_packet(byte_order: u8, cookie: &[u8; X11_COOKIE_BYTES]) -> Vec<u8> {
    let protocol = X11_AUTH_PROTOCOL.as_bytes();
    let total = setup_packet_len(protocol.len(), cookie.len()).unwrap();
    let mut packet = vec![0u8; total];
    packet[0] = byte_order;
    let major_version = if byte_order == b'l' {
        11u16.to_le_bytes()
    } else {
        11u16.to_be_bytes()
    };
    packet[2..4].copy_from_slice(&major_version);
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

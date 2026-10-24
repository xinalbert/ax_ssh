use super::*;

#[test]
fn protocol_routes_reject_attempts_from_other_protocols() {
    let profile_id = Uuid::from_u128(1);
    let ssh_attempt = Uuid::from_u128(2);
    let telnet_attempt = Uuid::from_u128(3);
    let serial_attempt = Uuid::from_u128(4);
    for (identity, ssh, telnet, serial) in [
        (
            ConnectionIdentity::Ssh {
                profile_id,
                attempt_id: None,
            },
            true,
            false,
            false,
        ),
        (
            ConnectionIdentity::Sftp {
                profile_id,
                attempt_id: None,
            },
            true,
            false,
            false,
        ),
        (
            ConnectionIdentity::Telnet {
                profile_id,
                attempt_id: None,
            },
            false,
            true,
            false,
        ),
        (
            ConnectionIdentity::Serial {
                profile_id,
                attempt_id: None,
            },
            false,
            false,
            true,
        ),
        (ConnectionIdentity::Local, false, false, false),
    ] {
        let mut current = identity;
        assert_eq!(current.set_ssh_attempt(Some(ssh_attempt)), ssh);
        assert_eq!(current.set_telnet_attempt(Some(telnet_attempt)), telnet);
        assert_eq!(current.set_serial_attempt(Some(serial_attempt)), serial);
        assert_eq!(
            current.ssh_route(),
            ssh.then_some((profile_id, Some(ssh_attempt)))
        );
        assert_eq!(
            current.telnet_route(),
            telnet.then_some((profile_id, Some(telnet_attempt)))
        );
        assert_eq!(
            current.serial_route(),
            serial.then_some((profile_id, Some(serial_attempt)))
        );
        assert_eq!(
            current.profile_id(),
            (!identity.is_local()).then_some(profile_id)
        );
    }
}

#[test]
fn replacing_one_attempt_preserves_duplicate_profile_and_sftp_identity() {
    let profile_id = Uuid::from_u128(1);
    let first_attempt = Uuid::from_u128(2);
    let replacement = Uuid::from_u128(3);
    let companion_attempt = Uuid::from_u128(4);
    let mut first = ConnectionIdentity::Ssh {
        profile_id,
        attempt_id: Some(first_attempt),
    };
    let duplicate = first;
    let companion = ConnectionIdentity::Sftp {
        profile_id,
        attempt_id: Some(companion_attempt),
    };
    assert!(first.set_ssh_attempt(Some(replacement)));
    assert_ne!(first.ssh_route(), Some((profile_id, Some(first_attempt))));
    assert_eq!(
        duplicate.ssh_route(),
        Some((profile_id, Some(first_attempt)))
    );
    assert_eq!(
        companion.ssh_route(),
        Some((profile_id, Some(companion_attempt)))
    );
    assert!(companion.is_sftp());
    assert!(!first.is_sftp());
    assert!(first.set_ssh_attempt(None));
    assert_eq!(first.ssh_route(), Some((profile_id, None)));
}

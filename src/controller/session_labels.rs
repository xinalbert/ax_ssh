//! Pure saved-session presentation shared by the independent frontends.

use std::net::Ipv4Addr;

use crate::config::SessionProfile;

/// Formats a sidebar endpoint using the configured single mask character.
///
/// This is presentation only: it masks username characters and the second IPv4
/// octet, preserving hostnames, IPv6, and serial ports. Full connection details
/// remain available to tooltips, editors, and transport owners.
pub fn profile_sidebar_endpoint(profile: &SessionProfile, mask_character: &str) -> String {
    match &profile.connection {
        crate::config::ConnectionProfile::Ssh(config) => format!(
            "{}@{}:{}",
            mask_username(&config.username, mask_character),
            mask_ipv4_host(&config.host, mask_character),
            config.port,
        ),
        crate::config::ConnectionProfile::Telnet(config) => format!(
            "telnet://{}:{}",
            mask_ipv4_host(&config.host, mask_character),
            config.port,
        ),
        crate::config::ConnectionProfile::Serial(config) => {
            format!("Serial {} @ {}", config.port_name, config.baud_rate)
        }
    }
}

fn mask_username(username: &str, mask_character: &str) -> String {
    let characters = username.chars().collect::<Vec<_>>();
    match characters.len() {
        0 => mask_character.to_owned(),
        1 => format!("{}{}", characters[0], mask_character),
        2..=4 => format!(
            "{}{}{}",
            characters[0],
            mask_character,
            characters[characters.len() - 1],
        ),
        _ => {
            let prefix = characters[..2].iter().collect::<String>();
            let suffix = characters[characters.len() - 2..]
                .iter()
                .collect::<String>();
            format!("{prefix}{mask_character}{suffix}")
        }
    }
}

fn mask_ipv4_host(host: &str, mask_character: &str) -> String {
    let Ok(address) = host.parse::<Ipv4Addr>() else {
        return host.to_owned();
    };
    let [first, _, third, fourth] = address.octets();
    format!("{first}.{mask_character}.{third}.{fourth}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidebar_endpoints_mask_username_and_ipv4_middle_octets() {
        let mut profile = SessionProfile::new("private", "192.168.1.202", "zhushixin");

        assert_eq!(
            profile_sidebar_endpoint(&profile, "*"),
            "zh*in@192.*.1.202:22"
        );

        let ssh = profile.ssh_mut().expect("profile should be SSH");
        ssh.host = "server.example.com".into();
        ssh.username = "root".into();
        assert_eq!(
            profile_sidebar_endpoint(&profile, "#"),
            "r#t@server.example.com:22"
        );
    }

    #[test]
    fn sidebar_labels_preserve_protocols_unicode_and_original_profiles() {
        for (username, expected) in [
            ("", "#"),
            ("李", "李#"),
            ("李雷", "李#雷"),
            ("alice", "al#ce"),
            ("阿尔伯特辛", "阿尔#特辛"),
        ] {
            let profile = SessionProfile::new("user name", "192.168.1.202", username);
            let original = profile.clone();
            assert_eq!(
                profile_sidebar_endpoint(&profile, "#"),
                format!("{expected}@192.#.1.202:22")
            );
            assert_eq!(profile, original);
        }
        for host in ["server.example", "2001:db8::1", "192.168.01.2"] {
            let profile = SessionProfile::new("host", host, "root");
            assert_eq!(
                profile_sidebar_endpoint(&profile, "•"),
                format!("r•t@{host}:22")
            );
        }
        let telnet = SessionProfile::new_telnet("legacy", "10.20.30.40");
        assert_eq!(
            profile_sidebar_endpoint(&telnet, "*"),
            "telnet://10.*.30.40:23"
        );
        let serial = SessionProfile::new_serial("console", "/dev/cu.usbserial");
        assert_eq!(
            profile_sidebar_endpoint(&serial, "*"),
            "Serial /dev/cu.usbserial @ 115200"
        );
    }
}

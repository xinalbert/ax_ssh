//! UI-independent grouping of saved session profiles.

use std::collections::BTreeSet;
use std::net::Ipv4Addr;

use ax_ssh::config::{SessionProfile, SessionStore, normalize_group_name};

pub(super) struct SessionGroup<'a> {
    pub(super) name: String,
    pub(super) profiles: Vec<&'a SessionProfile>,
}

pub(super) fn session_groups(sessions: &SessionStore) -> Vec<SessionGroup<'_>> {
    let mut groups = sessions
        .groups
        .iter()
        .map(|name| SessionGroup {
            name: normalize_group_name(name),
            profiles: Vec::new(),
        })
        .collect::<Vec<_>>();
    for profile in &sessions.sessions {
        let group_name = normalize_group_name(&profile.group_name);
        if let Some(group) = groups.iter_mut().find(|group| group.name == group_name) {
            group.profiles.push(profile);
        } else {
            groups.push(SessionGroup {
                name: group_name,
                profiles: vec![profile],
            });
        }
    }
    groups
}

pub(super) fn group_options(sessions: &SessionStore) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut options = vec!["Ungrouped".to_owned()];
    for group_name in &sessions.groups {
        let group_name = normalize_group_name(group_name);
        if !group_name.is_empty() && seen.insert(group_name.clone()) {
            options.push(group_name);
        }
    }
    for profile in &sessions.sessions {
        let group_name = normalize_group_name(&profile.group_name);
        if !group_name.is_empty() && seen.insert(group_name.clone()) {
            options.push(group_name);
        }
    }
    options
}

pub(super) fn profile_endpoint(profile: &SessionProfile) -> String {
    match &profile.connection {
        ax_ssh::config::ConnectionProfile::Ssh(config) => {
            format!("{}@{}:{}", config.username, config.host, config.port)
        }
        ax_ssh::config::ConnectionProfile::Telnet(config) => {
            format!("telnet://{}:{}", config.host, config.port)
        }
        ax_ssh::config::ConnectionProfile::Serial(config) => {
            format!("Serial {} @ {}", config.port_name, config.baud_rate)
        }
    }
}

pub(super) fn profile_sidebar_endpoint(profile: &SessionProfile, mask_character: &str) -> String {
    match &profile.connection {
        ax_ssh::config::ConnectionProfile::Ssh(config) => format!(
            "{}@{}:{}",
            mask_username(&config.username, mask_character),
            mask_ipv4_host(&config.host, mask_character),
            config.port,
        ),
        ax_ssh::config::ConnectionProfile::Telnet(config) => format!(
            "telnet://{}:{}",
            mask_ipv4_host(&config.host, mask_character),
            config.port,
        ),
        ax_ssh::config::ConnectionProfile::Serial(config) => {
            format!("Serial {} @ {}", config.port_name, config.baud_rate)
        }
    }
}

pub(super) fn profile_sidebar_details(profile: &SessionProfile) -> String {
    match &profile.connection {
        ax_ssh::config::ConnectionProfile::Ssh(_) => {
            format!("SSH · {}", profile_endpoint(profile))
        }
        ax_ssh::config::ConnectionProfile::Telnet(_) => {
            format!("Telnet · {}", profile_endpoint(profile))
        }
        ax_ssh::config::ConnectionProfile::Serial(config) => {
            format!("Serial · {} · {} baud", config.port_name, config.baud_rate)
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

pub(super) fn compact_label(value: &str, fallback: &str, max_chars: usize) -> String {
    let value = value.trim();
    let value = if value.is_empty() { fallback } else { value };
    if max_chars == 0 {
        value.to_owned()
    } else {
        value.chars().take(max_chars).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_options_are_normalized_unique_and_keep_first_seen_order() {
        let mut production = SessionProfile::new("prod", "prod.example", "alice");
        production.group_name = " Production ".into();
        let mut staging = SessionProfile::new("stage", "stage.example", "bob");
        staging.group_name = "Staging".into();
        let mut production_again = SessionProfile::new("prod-2", "prod-2.example", "carol");
        production_again.group_name = "Production".into();
        let sessions = SessionStore {
            groups: vec!["Empty".into(), "Production".into()],
            sessions: vec![production, staging, production_again],
            ..SessionStore::default()
        };

        assert_eq!(
            group_options(&sessions),
            ["Ungrouped", "Empty", "Production", "Staging"]
        );
    }

    #[test]
    fn compact_labels_keep_the_first_two_unicode_characters() {
        assert_eq!(compact_label("Production", "Un", 2), "Pr");
        assert_eq!(compact_label("生产环境", "Un", 2), "生产");
        assert_eq!(compact_label("Production", "Un", 0), "Production");
        assert_eq!(compact_label("", "Un", 2), "Un");
    }

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
    fn sidebar_details_identify_protocol_with_full_non_secret_connection_information() {
        let ssh = SessionProfile::new("private", "192.168.1.202", "zhushixin");
        assert_eq!(
            profile_sidebar_details(&ssh),
            "SSH · zhushixin@192.168.1.202:22"
        );

        let telnet = SessionProfile::new_telnet("legacy", "10.20.30.40");
        assert_eq!(
            profile_sidebar_details(&telnet),
            "Telnet · telnet://10.20.30.40:23"
        );

        let serial = SessionProfile::new_serial("console", "/dev/cu.usbserial");
        assert_eq!(
            profile_sidebar_details(&serial),
            "Serial · /dev/cu.usbserial · 115200 baud"
        );
    }
}

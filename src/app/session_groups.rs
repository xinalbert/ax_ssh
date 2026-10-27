//! UI-independent grouping of saved session profiles.

use std::collections::BTreeSet;

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

pub(super) use ax_ssh::controller::profile_sidebar_endpoint;

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

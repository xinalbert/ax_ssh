//! UI-independent grouping of saved session profiles.

use std::collections::BTreeSet;
use std::net::Ipv4Addr;

use ax_ssh::config::{SessionProfile, SessionStore, normalize_group_name};

pub(super) struct SessionGroup<'a> {
    pub(super) name: String,
    pub(super) profiles: Vec<&'a SessionProfile>,
}

pub(super) fn session_groups(sessions: &SessionStore) -> Vec<SessionGroup<'_>> {
    let mut groups: Vec<SessionGroup<'_>> = Vec::new();
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
    for profile in &sessions.sessions {
        let group_name = normalize_group_name(&profile.group_name);
        if !group_name.is_empty() && seen.insert(group_name.clone()) {
            options.push(group_name);
        }
    }
    options
}

pub(super) fn profile_endpoint(profile: &SessionProfile) -> String {
    format!("{}@{}:{}", profile.username, profile.host, profile.port)
}

pub(super) fn profile_sidebar_endpoint(profile: &SessionProfile, mask_character: &str) -> String {
    format!(
        "{}@{}:{}",
        mask_username(&profile.username, mask_character),
        mask_ipv4_host(&profile.host, mask_character),
        profile.port,
    )
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
    let [first, _, _, last] = address.octets();
    format!("{first}.{mask_character}.{last}")
}

pub(super) fn compact_label(value: &str, fallback: &str) -> String {
    let value = value.trim();
    let value = if value.is_empty() { fallback } else { value };
    value.chars().take(2).collect()
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
            sessions: vec![production, staging, production_again],
            ..SessionStore::default()
        };

        assert_eq!(
            group_options(&sessions),
            ["Ungrouped", "Production", "Staging"]
        );
    }

    #[test]
    fn compact_labels_keep_the_first_two_unicode_characters() {
        assert_eq!(compact_label("Production", "Un"), "Pr");
        assert_eq!(compact_label("生产环境", "Un"), "生产");
        assert_eq!(compact_label("", "Un"), "Un");
    }

    #[test]
    fn sidebar_endpoints_mask_username_and_ipv4_middle_octets() {
        let mut profile = SessionProfile::new("private", "192.168.1.202", "zhushixin");
        profile.port = 22;

        assert_eq!(
            profile_sidebar_endpoint(&profile, "*"),
            "zh*in@192.*.202:22"
        );

        profile.host = "server.example.com".into();
        profile.username = "root".into();
        assert_eq!(
            profile_sidebar_endpoint(&profile, "#"),
            "r#t@server.example.com:22"
        );
    }
}

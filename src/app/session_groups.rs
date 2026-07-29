//! UI-independent grouping of saved session profiles.

use std::collections::BTreeSet;

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
}

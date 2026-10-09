use super::*;
use ax_ssh::config::{SystemDetection, system_icon_id};

impl AppState {
    pub(in crate::app) fn system_detection(&self, profile_id: Uuid) -> Option<&SystemDetection> {
        let profile = self
            .sessions
            .sessions
            .iter()
            .find(|profile| profile.id == profile_id)?;
        self.system_detections
            .get(&profile_id)
            .filter(|system| system.matches_profile(profile))
            .or_else(|| profile.cached_system())
    }

    // One in-flight request per destination, including SSH and SFTP tabs.
    pub(in crate::app) fn request_system_detection(
        &mut self,
        profile: &SessionProfile,
        tab_id: Uuid,
        attempt_id: Uuid,
    ) {
        let Some(current) = self
            .sessions
            .sessions
            .iter()
            .find(|current| current.id == profile.id)
        else {
            return;
        };
        if current.ssh().map(|ssh| {
            (
                &ssh.host,
                ssh.port,
                &ssh.username,
                &ssh.host_key_fingerprint,
            )
        }) != profile.ssh().map(|ssh| {
            (
                &ssh.host,
                ssh.port,
                &ssh.username,
                &ssh.host_key_fingerprint,
            )
        }) {
            return;
        }
        let release = self
            .system_detection(profile.id)
            .map(|system| system.release().to_owned());
        let manual = current
            .system_icon_override
            .as_deref()
            .and_then(system_icon_id)
            .is_some();
        if let Some(terminal) = self.terminal_mut(tab_id) {
            terminal.system_release = release.clone();
        }
        if release.is_some() || manual {
            return;
        }
        if let Some(&(owner, attempt)) = self.system_release_probes.get(&profile.id)
            && self.terminal(owner).is_some_and(|terminal| {
                terminal.worker_running && terminal.ssh_route() == Some((profile.id, Some(attempt)))
            })
        {
            return;
        }
        let queued = self
            .terminal(tab_id)
            .filter(|terminal| terminal.ssh_route() == Some((profile.id, Some(attempt_id))))
            .and_then(|terminal| terminal.worker.as_ref())
            .and_then(|worker| match worker {
                TerminalWorker::Ssh(worker) => Some(worker.request_system_release()),
                _ => None,
            });
        match queued {
            Some(Ok(())) => {
                self.system_release_probes
                    .insert(profile.id, (tab_id, attempt_id));
            }
            Some(Err(error)) => tracing::debug!(%error, "could not queue SSH system release probe"),
            None => {}
        }
    }

    pub(in crate::app) fn accept_system_detection(
        &mut self,
        profile: &SessionProfile,
        tab_id: Uuid,
        attempt_id: Uuid,
        release: Option<&str>,
    ) -> Option<SystemDetection> {
        if self.system_release_probes.get(&profile.id) != Some(&(tab_id, attempt_id))
            || !self.terminal(tab_id).is_some_and(|terminal| {
                terminal.connected
                    && terminal.worker_running
                    && terminal.ssh_route() == Some((profile.id, Some(attempt_id)))
            })
        {
            return None;
        }
        self.system_release_probes.remove(&profile.id);
        let release = release?;
        let system = SystemDetection::new(
            profile,
            release,
            super::super::system_icons::platform_for_release(release),
        )?;
        let current = self
            .sessions
            .sessions
            .iter()
            .find(|current| current.id == profile.id)?;
        if !system.matches_profile(current) {
            return None;
        }
        self.system_detections.insert(profile.id, system.clone());
        Some(system)
    }

    pub(in crate::app) fn clear_system_detection(&mut self, profile_id: Uuid) {
        self.forget_system_detection(profile_id);
        for tab in &mut self.tabs {
            if let WorkspaceTabKind::Terminal(terminal) = &mut tab.kind
                && terminal.profile_id() == Some(profile_id)
            {
                terminal.system_release = None;
            }
        }
    }

    pub(in crate::app) fn forget_system_detection(&mut self, profile_id: Uuid) {
        self.system_detections.remove(&profile_id);
        self.system_release_probes.remove(&profile_id);
    }

    // Called in a blocking task under the process-wide persistence gate. Wait
    // for profile/credential transactions before changing their expected value.
    pub(in crate::app) fn save_system_detection(
        &mut self,
        profile_id: Uuid,
        system: &SystemDetection,
    ) -> Result<bool> {
        if self.profile_mutation_is_pending(profile_id) {
            return Ok(false);
        }
        if self.system_detections.get(&profile_id) != Some(system)
            || !self
                .sessions
                .sessions
                .iter()
                .any(|profile| profile.id == profile_id && system.matches_profile(profile))
        {
            return Ok(true);
        }
        // UI Settings previews are not committed by a background discovery.
        let mut candidate = self.config.load()?;
        if let Some(profile) = candidate
            .sessions
            .iter_mut()
            .find(|profile| profile.id == profile_id)
        {
            if !system.matches_profile(profile) {
                return Ok(true);
            }
            profile.detected_system = Some(Box::new(system.clone()));
        } else {
            return Ok(true);
        }
        self.config.save(&candidate)?;
        if let Some(profile) = self
            .sessions
            .sessions
            .iter_mut()
            .find(|profile| profile.id == profile_id)
        {
            profile.detected_system = Some(Box::new(system.clone()));
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (AppState, SessionProfile, Uuid, Uuid, PathBuf) {
        let path = std::env::temp_dir().join(format!("axssh-system-icon-{}", Uuid::new_v4()));
        let config = ConfigStore::new(path.join("sessions.json"));
        let profile = SessionProfile::new("Server", "server.example", "alice");
        let mut store = SessionStore::default();
        store.upsert(profile.clone());
        config.save(&store).unwrap();
        let mut app = AppState::new(config, store);
        let tab = app.open_terminal_tab(&profile);
        let attempt = Uuid::new_v4();
        let terminal = app.terminal_mut(tab).unwrap();
        terminal.set_ssh_attempt(Some(attempt));
        terminal.connected = true;
        terminal.worker_running = true;
        app.system_release_probes.insert(profile.id, (tab, attempt));
        (app, profile, tab, attempt, path)
    }

    #[test]
    fn discovery_persists_once_and_is_available_after_restart_without_tabs() {
        let (mut app, profile, tab, attempt, path) = fixture();
        let system = app
            .accept_system_detection(&profile, tab, attempt, Some("Ubuntu 24.04 LTS"))
            .unwrap();
        assert!(
            app.accept_system_detection(&profile, tab, attempt, Some("Debian 12"))
                .is_none()
        );
        app.sessions.settings.terminal.scrollback_lines = 321;
        assert!(app.save_system_detection(profile.id, &system).unwrap());
        let disk = app.config.load().unwrap();
        assert_eq!(
            disk.settings.terminal.scrollback_lines,
            SessionStore::default().settings.terminal.scrollback_lines
        );
        assert_eq!(app.sessions.settings.terminal.scrollback_lines, 321);
        let mut restored = AppState::new(app.config.clone(), disk);
        assert_eq!(
            restored.profile_system_releases()[&profile.id],
            "Ubuntu 24.04 LTS"
        );
        let tab = restored.open_sftp_tab(&profile);
        restored
            .terminal_mut(tab)
            .unwrap()
            .set_ssh_attempt(Some(attempt));
        restored.request_system_detection(&profile, tab, attempt);
        assert!(restored.system_release_probes.is_empty());
        assert_eq!(
            restored.terminal(tab).unwrap().system_release.as_deref(),
            Some("Ubuntu 24.04 LTS")
        );
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn failed_probe_remains_uncached_and_reset_rejects_late_results() {
        let (mut app, profile, tab, attempt, path) = fixture();
        assert!(
            app.accept_system_detection(&profile, tab, attempt, None)
                .is_none()
        );
        assert!(app.system_detection(profile.id).is_none());
        assert!(app.system_release_probes.is_empty());
        app.system_release_probes.insert(profile.id, (tab, attempt));
        app.clear_system_detection(profile.id);
        assert!(
            app.accept_system_detection(&profile, tab, attempt, Some("Ubuntu"))
                .is_none()
        );
        assert!(
            app.config.load().unwrap().sessions[0]
                .cached_system()
                .is_none()
        );
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn profile_transactions_delay_cache_write_and_preserve_manual_choice() {
        let (mut app, profile, tab, attempt, path) = fixture();
        let system = app
            .accept_system_detection(&profile, tab, attempt, Some("Debian 12"))
            .unwrap();
        let token = app.begin_profile_mutation(profile.id);
        assert!(!app.save_system_detection(profile.id, &system).unwrap());
        let mut edited = app.sessions.clone();
        edited.sessions[0].system_icon_override = Some("ubuntu".into());
        edited.sessions[0].name = "Renamed".into();
        app.config.save(&edited).unwrap();
        app.sessions = edited;
        app.finish_profile_mutation(profile.id, token);
        assert!(app.save_system_detection(profile.id, &system).unwrap());
        let saved = app.config.load().unwrap();
        assert_eq!(
            saved.sessions[0].system_icon_override.as_deref(),
            Some("ubuntu")
        );
        assert_eq!(saved.sessions[0].name, "Renamed");
        assert_eq!(
            saved.sessions[0].cached_system().unwrap().icon_id(),
            Some("debian")
        );
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn reset_or_destination_change_prevents_pending_cache_write() {
        for reset in [true, false] {
            let (mut app, profile, tab, attempt, path) = fixture();
            let system = app
                .accept_system_detection(&profile, tab, attempt, Some("Ubuntu"))
                .unwrap();
            if reset {
                app.clear_system_detection(profile.id);
            } else {
                app.sessions.sessions[0].ssh_mut().unwrap().host = "other.example".into();
            }
            assert!(app.save_system_detection(profile.id, &system).unwrap());
            assert!(
                app.config.load().unwrap().sessions[0]
                    .cached_system()
                    .is_none()
            );
            std::fs::remove_dir_all(path).unwrap();
        }
    }

    #[test]
    fn manual_icons_and_unknown_success_skip_detection() {
        let (mut app, profile, tab, attempt, path) = fixture();
        app.system_release_probes.clear();
        app.sessions.sessions[0].system_icon_override = Some("macos".into());
        app.request_system_detection(&profile, tab, attempt);
        assert!(app.system_release_probes.is_empty());
        assert_eq!(app.tab_summaries()[0].system_icon.as_deref(), Some("macos"));
        app.sessions.settings.appearance.local_shell_icon_override = Some("windows".into());
        app.open_local_shell_tab();
        assert_eq!(
            app.tab_summaries()[1].system_icon.as_deref(),
            Some("windows")
        );
        app.sessions.sessions[0].system_icon_override = None;
        app.sessions.sessions[0].detected_system =
            SystemDetection::new(&profile, "FreeBSD 14.3", "").map(Box::new);
        app.request_system_detection(&profile, tab, attempt);
        assert!(app.system_release_probes.is_empty());
        assert_eq!(
            app.terminal(tab).unwrap().system_release.as_deref(),
            Some("FreeBSD 14.3")
        );
        std::fs::remove_dir_all(path).unwrap();
    }
}

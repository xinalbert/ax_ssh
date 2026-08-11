use super::*;

impl SessionEditorState {
    pub(super) fn snapshot(&self, sessions: &SessionStore) -> SessionEditorSnapshot {
        let Some(profile) = self.profile_id.and_then(|profile_id| {
            sessions
                .sessions
                .iter()
                .find(|profile| profile.id == profile_id)
        }) else {
            return SessionEditorSnapshot {
                draft_id: self.draft_id,
                group_name: self.group_name.clone(),
                default_credential_storage: sessions
                    .settings
                    .credential_storage
                    .as_setting()
                    .to_owned(),
                ..SessionEditorSnapshot::default()
            };
        };
        let credential_storage = profile
            .ssh()
            .and_then(|ssh| ssh.credential_storage)
            .map(|storage| storage.as_setting().to_owned())
            .unwrap_or_default();
        let default_credential_storage =
            sessions.settings.credential_storage.as_setting().to_owned();
        let (
            protocol,
            host,
            port,
            username,
            auth_method,
            private_key_path,
            sftp_remote_path,
            sftp_local_path,
            x11_forwarding,
            serial_port,
            serial_baud_rate,
            serial_data_bits,
            serial_stop_bits,
            serial_parity,
            serial_flow_control,
        ) = match &profile.connection {
            ax_ssh::config::ConnectionProfile::Ssh(config) => {
                let (auth_method, private_key_path) = match &config.auth {
                    ax_ssh::config::AuthMethod::Password => ("Password", String::new()),
                    ax_ssh::config::AuthMethod::PrivateKey { path } => {
                        ("Private key", path.to_string_lossy().into_owned())
                    }
                    ax_ssh::config::AuthMethod::SshAgent => ("SSH agent", String::new()),
                };
                (
                    "SSH",
                    config.host.clone(),
                    config.port.to_string(),
                    config.username.clone(),
                    auth_method,
                    private_key_path,
                    if config.sftp_remote_path.trim().is_empty() {
                        "~".to_owned()
                    } else {
                        config.sftp_remote_path.clone()
                    },
                    if config.sftp_local_path.is_empty() {
                        default_local_directory()
                    } else {
                        config.sftp_local_path.clone()
                    },
                    config.x11_forwarding,
                    String::new(),
                    "115200".to_owned(),
                    "8",
                    "1",
                    "none",
                    "none",
                )
            }
            ax_ssh::config::ConnectionProfile::Telnet(config) => (
                "Telnet",
                config.host.clone(),
                config.port.to_string(),
                String::new(),
                "Password",
                String::new(),
                String::new(),
                String::new(),
                false,
                String::new(),
                "115200".to_owned(),
                "8",
                "1",
                "none",
                "none",
            ),
            ax_ssh::config::ConnectionProfile::Serial(config) => (
                "Serial",
                String::new(),
                "23".to_owned(),
                String::new(),
                "Password",
                String::new(),
                String::new(),
                String::new(),
                false,
                config.port_name.clone(),
                config.baud_rate.to_string(),
                config.data_bits.as_setting(),
                config.stop_bits.as_setting(),
                config.parity.as_setting(),
                config.flow_control.as_setting(),
            ),
        };
        SessionEditorSnapshot {
            draft_id: self.draft_id,
            profile_id: Some(profile.id),
            name: profile.name.clone(),
            group_name: profile.group_name.clone(),
            protocol,
            host,
            port,
            username,
            auth_method,
            private_key_path,
            sftp_remote_path,
            sftp_local_path,
            credential_storage,
            default_credential_storage,
            x11_forwarding,
            serial_port,
            serial_baud_rate,
            serial_data_bits,
            serial_stop_bits,
            serial_parity,
            serial_flow_control,
        }
    }
}

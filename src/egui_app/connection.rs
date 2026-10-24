//! Temporary connection drafts; no persistence, worker handles or trust decisions.

use anyhow::{Context, Result};
use ax_ssh::config::{AuthMethod, ConnectionProfile, SessionProfile, X11ForwardingMode};
use ax_ssh::controller::{TerminalClient, TerminalPrompt, TerminalPromptKind, TerminalView};
use eframe::egui;
use zeroize::Zeroizing;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Protocol {
    Local,
    #[default]
    Ssh,
    Telnet,
    Serial,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Authentication {
    #[default]
    Password,
    Key,
    Agent,
}

pub(super) struct ConnectionForm {
    base: Option<SessionProfile>,
    protocol: Protocol,
    authentication: Authentication,
    automatic_reconnect: bool,
    host: String,
    port: u16,
    username: String,
    key_path: String,
    device: String,
    baud: u32,
    shell: String,
    secret: Zeroizing<String>,
    prompt_id: Option<uuid::Uuid>,
    last_action_frame: Option<u64>,
}

impl Default for ConnectionForm {
    fn default() -> Self {
        Self {
            base: None,
            protocol: Protocol::Ssh,
            authentication: Authentication::Password,
            automatic_reconnect: false,
            host: String::new(),
            port: 22,
            username: String::new(),
            key_path: String::new(),
            device: String::new(),
            baud: 115_200,
            shell: ax_ssh::local_shell::SYSTEM_SHELL.into(),
            secret: Zeroizing::new(String::new()),
            prompt_id: None,
            last_action_frame: None,
        }
    }
}

enum Action {
    Connect,
    Disconnect,
    Respond(uuid::Uuid),
}

impl ConnectionForm {
    pub(super) fn show(
        &mut self,
        ui: &mut egui::Ui,
        client: &TerminalClient,
        view: &TerminalView,
    ) -> Result<()> {
        let prompt_id = view.prompt.as_ref().map(|prompt| prompt.id);
        if self.prompt_id != prompt_id {
            self.secret = Zeroizing::new(String::new());
            self.prompt_id = prompt_id;
        }
        let mut action = None;
        if let Some(prompt) = &view.prompt {
            action = self.prompt(ui, prompt);
        } else {
            egui::CollapsingHeader::new("Connection")
                .default_open(self.protocol != Protocol::Local)
                .show(ui, |ui| {
                    self.fields(ui, true);
                    if self.protocol != Protocol::Local {
                        ui.checkbox(&mut self.automatic_reconnect, "Automatically retry (up to 5 attempts; fresh SSH credentials when needed)");
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Connect").clicked() {
                            action = Some(Action::Connect);
                        }
                        if ui.button("Disconnect").clicked() {
                            action = Some(Action::Disconnect);
                        }
                        ui.label("Temporary; replaces this terminal. Credentials are not saved.");
                    });
                });
        }
        if let Some(action) = action {
            let frame = ui.ctx().cumulative_frame_nr();
            if !ui.is_sizing_pass() && self.last_action_frame != Some(frame) {
                self.last_action_frame = Some(frame);
                self.apply(action, &client.for_view(view))?;
            }
        }
        Ok(())
    }

    pub(super) fn from_profile(profile: &SessionProfile) -> Self {
        let mut form = Self {
            base: Some(profile.clone()),
            ..Self::default()
        };
        match &profile.connection {
            ConnectionProfile::Ssh(config) => {
                form.host = config.host.clone();
                form.port = config.port;
                form.username = config.username.clone();
                match &config.auth {
                    AuthMethod::Password => form.authentication = Authentication::Password,
                    AuthMethod::PrivateKey { path } => {
                        form.authentication = Authentication::Key;
                        form.key_path = path.to_string_lossy().into_owned();
                    }
                    AuthMethod::SshAgent => form.authentication = Authentication::Agent,
                }
            }
            ConnectionProfile::Telnet(config) => {
                form.protocol = Protocol::Telnet;
                form.host = config.host.clone();
                form.port = config.port;
            }
            ConnectionProfile::Serial(config) => {
                form.protocol = Protocol::Serial;
                form.device = config.port_name.clone();
                form.baud = config.baud_rate;
            }
        }
        form
    }

    pub(super) fn local() -> Self {
        Self {
            protocol: Protocol::Local,
            ..Default::default()
        }
    }

    pub(super) fn fields(&mut self, ui: &mut egui::Ui, include_secret: bool) {
        ui.horizontal_wrapped(|ui| {
            let previous = self.protocol;
            if include_secret {
                ui.selectable_value(&mut self.protocol, Protocol::Local, "Local");
            }
            ui.selectable_value(&mut self.protocol, Protocol::Ssh, "SSH");
            ui.selectable_value(&mut self.protocol, Protocol::Telnet, "Telnet");
            ui.selectable_value(&mut self.protocol, Protocol::Serial, "Serial");
            if self.protocol != previous {
                self.secret = Zeroizing::new(String::new());
                self.port = if self.protocol == Protocol::Telnet {
                    23
                } else {
                    22
                };
            }
        });
        egui::Grid::new("temporary-connection-fields")
            .num_columns(2)
            .show(ui, |ui| match self.protocol {
                Protocol::Local => {
                    ui.label("Shell");
                    edit(ui, &mut self.shell, 4096);
                    ui.end_row();
                }
                Protocol::Ssh | Protocol::Telnet => {
                    ui.label("Host");
                    edit(ui, &mut self.host, 512);
                    ui.end_row();
                    ui.label("Port");
                    ui.add(egui::DragValue::new(&mut self.port).range(1..=65535));
                    ui.end_row();
                    if self.protocol == Protocol::Ssh {
                        ui.label("Username");
                        edit(ui, &mut self.username, 256);
                        ui.end_row();
                        ui.label("Authentication");
                        ui.horizontal_wrapped(|ui| {
                            let previous = self.authentication;
                            ui.selectable_value(
                                &mut self.authentication,
                                Authentication::Password,
                                "Password",
                            );
                            ui.selectable_value(
                                &mut self.authentication,
                                Authentication::Key,
                                "Private key",
                            );
                            ui.selectable_value(
                                &mut self.authentication,
                                Authentication::Agent,
                                "SSH agent",
                            );
                            if previous != self.authentication {
                                self.secret = Zeroizing::new(String::new());
                            }
                        });
                        ui.end_row();
                        if self.authentication == Authentication::Key {
                            ui.label("Key path");
                            edit(ui, &mut self.key_path, 4096);
                            ui.end_row();
                        }
                        if include_secret && self.authentication != Authentication::Agent {
                            ui.label("Password / passphrase");
                            self.secret_edit(ui);
                            ui.end_row();
                        }
                    }
                }
                Protocol::Serial => {
                    ui.label("Device path");
                    edit(ui, &mut self.device, 512);
                    ui.end_row();
                    ui.label("Baud (8N1, no flow control)");
                    ui.add(egui::DragValue::new(&mut self.baud).range(1..=4_000_000));
                    ui.end_row();
                }
            });
    }

    fn prompt(&mut self, ui: &mut egui::Ui, prompt: &TerminalPrompt) -> Option<Action> {
        ui.strong(format!(
            "SSH · {}@{}:{}",
            prompt.username, prompt.host, prompt.port
        ));
        let allowed = match &prompt.kind {
            TerminalPromptKind::HostKey {
                expected,
                actual,
                confirmable,
            } => {
                ui.label(format!("Observed key: {actual}"));
                if let Some(expected) = expected {
                    ui.label(format!("Expected key: {expected}"));
                }
                if *confirmable {
                    ui.label("Verify the fingerprint with the server owner before trusting it for this connection.");
                } else {
                    ui.label("Host key blocked by trust policy or changed during connection. Disconnect and check the server and known_hosts.");
                }
                *confirmable
            }
            TerminalPromptKind::Authentication => {
                ui.label("Enter fresh credentials to connect or retry.");
                true
            }
        };
        let mut action = None;
        if allowed {
            ui.horizontal(|ui| {
                ui.label("Password / key passphrase (empty for agent)");
                self.secret_edit(ui);
            });
            let label = if matches!(prompt.kind, TerminalPromptKind::HostKey { .. }) {
                "Trust once and connect"
            } else {
                "Retry"
            };
            if ui.button(label).clicked() {
                action = Some(Action::Respond(prompt.id));
            }
        }
        if ui.button("Cancel / disconnect").clicked() {
            action = Some(Action::Disconnect);
        }
        action
    }

    fn secret_edit(&mut self, ui: &mut egui::Ui) {
        let mut output = egui::TextEdit::singleline(&mut *self.secret)
            .id(ui.scope_id().with("temporary-connection-secret"))
            .password(true)
            .char_limit(16 * 1024)
            .show(ui);
        // egui edit history must not restore a submitted credential.
        output.state.clear_undoer();
        output.state.store(ui.ctx(), output.response.id);
    }

    pub(super) fn profile(&self) -> Result<SessionProfile> {
        let mut profile = match self.protocol {
            Protocol::Ssh => {
                let mut profile =
                    SessionProfile::new("Temporary SSH", self.host.trim(), self.username.trim());
                let config = profile.ssh_mut().context("missing SSH draft")?;
                config.port = self.port;
                config.x11_forwarding = X11ForwardingMode::Off;
                config.auth = match self.authentication {
                    Authentication::Password => AuthMethod::Password,
                    Authentication::Key => AuthMethod::PrivateKey {
                        path: self.key_path.trim().into(),
                    },
                    Authentication::Agent => AuthMethod::SshAgent,
                };
                profile
            }
            Protocol::Telnet => SessionProfile::new_telnet("Temporary Telnet", self.host.trim()),
            Protocol::Serial => SessionProfile::new_serial("Temporary Serial", self.device.trim()),
            Protocol::Local => anyhow::bail!("local shells do not use a remote profile"),
        };
        match &mut profile.connection {
            ConnectionProfile::Telnet(config) => config.port = self.port,
            ConnectionProfile::Serial(config) => config.baud_rate = self.baud,
            ConnectionProfile::Ssh(_) => {}
        }
        if let Some(base) = &self.base {
            let mut merged = base.clone();
            match (&mut merged.connection, &profile.connection) {
                (ConnectionProfile::Ssh(target), ConnectionProfile::Ssh(draft)) => {
                    if target.host != draft.host || target.port != draft.port {
                        target.host_key_fingerprint = None;
                        merged.detected_system = None;
                    }
                    target.host = draft.host.clone();
                    target.port = draft.port;
                    target.username = draft.username.clone();
                    target.auth = draft.auth.clone();
                }
                (ConnectionProfile::Serial(target), ConnectionProfile::Serial(draft)) => {
                    if target.port_name != draft.port_name {
                        target.usb_vendor_id = None;
                        target.usb_product_id = None;
                        target.usb_serial_number = None;
                    }
                    target.port_name = draft.port_name.clone();
                    target.baud_rate = draft.baud_rate;
                }
                _ => {
                    merged.connection = profile.connection;
                    merged.detected_system = None;
                }
            }
            profile = merged;
        }
        profile.validate()?;
        Ok(profile)
    }

    fn apply(&mut self, action: Action, client: &TerminalClient) -> Result<()> {
        // Taking the value drops/zeroizes it on validation and admission failures too.
        let secret = std::mem::replace(&mut self.secret, Zeroizing::new(String::new()));
        match action {
            Action::Connect if self.protocol == Protocol::Local => {
                client.local(self.shell.trim().into())
            }
            Action::Connect => {
                let mut profile = self.profile()?;
                if let Some(ssh) = profile.ssh_mut() {
                    ssh.x11_forwarding = X11ForwardingMode::Off;
                }
                client.connect_with_reconnect(profile, secret, self.automatic_reconnect)
            }
            Action::Disconnect => client.disconnect(),
            Action::Respond(id) => client.respond(id, secret),
        }
    }
}

fn edit(ui: &mut egui::Ui, text: &mut String, limit: usize) {
    ui.add(egui::TextEdit::singleline(text).char_limit(limit));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editing_saved_endpoints_preserves_identity_and_unrelated_settings() {
        let mut profile = SessionProfile::new("Saved fixture", "old.example", "fixture");
        let ssh = profile.ssh_mut().unwrap();
        ssh.host_key_fingerprint = Some("fixture-pin".into());
        ssh.sftp_remote_path = "/keep".into();
        ssh.x11_forwarding = X11ForwardingMode::Trusted;
        let mut form = ConnectionForm::from_profile(&profile);
        assert_eq!(form.profile().unwrap(), profile);
        form.host = "new.example".into();
        let edited = form.profile().unwrap();
        assert_eq!(edited.id, profile.id);
        assert!(edited.ssh().unwrap().host_key_fingerprint.is_none());
        assert_eq!(edited.ssh().unwrap().sftp_remote_path, "/keep");
        assert_eq!(
            edited.ssh().unwrap().x11_forwarding,
            X11ForwardingMode::Trusted
        );
    }

    #[test]
    fn drafts_validate_without_storage_or_optional_ssh_services() {
        let mut form = ConnectionForm::default();
        assert!(form.profile().is_err());
        form.host = "localhost".into();
        form.username = "fixture".into();
        let profile = form.profile().unwrap();
        let ssh = profile.ssh().unwrap();
        assert_eq!(ssh.x11_forwarding, X11ForwardingMode::Off);
        assert_eq!(ssh.credential_storage, None);
        assert_eq!(ssh.host_key_fingerprint, None);
        form.protocol = Protocol::Telnet;
        form.port = 2323;
        assert_eq!(form.profile().unwrap().telnet().unwrap().port, 2323);
        form.protocol = Protocol::Serial;
        form.device = "fixture-device".into();
        form.baud = 9600;
        assert_eq!(form.profile().unwrap().serial().unwrap().baud_rate, 9600);
    }

    #[tokio::test]
    async fn validation_failure_discards_the_submitted_secret() {
        let terminal = ax_ssh::controller::TerminalSession::spawn(
            &tokio::runtime::Handle::current(),
            "axssh-no-such-fixture-shell".into(),
            std::sync::Arc::new(|| {}),
        );
        let mut form = ConnectionForm {
            secret: Zeroizing::new("fixture-ephemeral-input".into()),
            ..Default::default()
        };
        assert!(form.apply(Action::Connect, &terminal.client()).is_err());
        assert!(form.secret.is_empty());
        terminal.shutdown().await.unwrap();
    }

    #[test]
    fn password_editor_does_not_keep_undo_history() {
        let ctx = egui::Context::default();
        let mut form = ConnectionForm {
            secret: Zeroizing::new("fixture-ephemeral-input".into()),
            ..Default::default()
        };
        let mut editor_id = None;
        let mut output = ctx.run_ui(Default::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                editor_id = Some(ui.scope_id().with("temporary-connection-secret"));
                form.secret_edit(ui);
            });
        });
        output.textures_delta.clear();
        form.secret = Zeroizing::new(String::new());
        let state = egui::text_edit::TextEditState::load(&ctx, editor_id.unwrap()).unwrap();
        assert!(
            state
                .undoer()
                .undo(&(Default::default(), String::new()))
                .is_none()
        );
    }
}

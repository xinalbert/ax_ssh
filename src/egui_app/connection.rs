//! Temporary connection drafts; no persistence, worker handles or trust decisions.

use super::i18n::tr;
use anyhow::{Context, Result};
use ax_ssh::config::{AuthMethod, ConnectionProfile, SessionProfile, X11ForwardingMode};
use ax_ssh::controller::{TerminalClient, TerminalConnectionOptions, TerminalPrompt, TerminalView};
use eframe::egui;
use zeroize::Zeroizing;

mod prompt;
pub(super) use prompt::{PromptAction, secret_edit, ssh_prompt};

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
    x11: X11ForwardingMode,
    automatic_reconnect: bool,
    microphone_forwarding: bool,
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
            x11: X11ForwardingMode::Off,
            automatic_reconnect: false,
            microphone_forwarding: false,
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
            egui::CollapsingHeader::new(tr!("Connection", ui.ctx())).id_salt("temporary-connection")
                .default_open(self.protocol != Protocol::Local)
                .show(ui, |ui| {
                    self.fields(ui, true);
                    if self.protocol != Protocol::Local {
                        ui.checkbox(&mut self.automatic_reconnect, tr!("Automatically retry (up to 5 attempts; fresh SSH credentials when needed)", ui.ctx()));
                    }
                    ui.horizontal(|ui| {
                        if ui.button(tr!("Connect", ui.ctx())).clicked() {
                            action = Some(Action::Connect);
                        }
                        if ui.button(tr!("Disconnect", ui.ctx())).clicked() {
                            action = Some(Action::Disconnect);
                        }
                        ui.label(tr!("Temporary; replaces this terminal. Credentials are not saved.", ui.ctx()));
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
                form.x11 = config.x11_forwarding;
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
                ui.selectable_value(
                    &mut self.protocol,
                    Protocol::Local,
                    tr!("Local Shell", ui.ctx()),
                );
            }
            ui.selectable_value(&mut self.protocol, Protocol::Ssh, "SSH");
            ui.selectable_value(&mut self.protocol, Protocol::Telnet, "Telnet");
            ui.selectable_value(
                &mut self.protocol,
                Protocol::Serial,
                tr!("Serial", ui.ctx()),
            );
            if self.protocol != previous {
                self.secret = Zeroizing::new(String::new());
                self.microphone_forwarding = false;
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
                    ui.label(tr!("Local shell", ui.ctx()));
                    edit(ui, &mut self.shell, 4096);
                    ui.end_row();
                }
                Protocol::Ssh | Protocol::Telnet => {
                    ui.label(tr!("Host", ui.ctx()));
                    edit(ui, &mut self.host, 512);
                    ui.end_row();
                    ui.label(tr!("Port", ui.ctx()));
                    ui.add(egui::DragValue::new(&mut self.port).range(1..=65535));
                    ui.end_row();
                    if self.protocol == Protocol::Ssh {
                        ui.label(tr!("Username", ui.ctx()));
                        edit(ui, &mut self.username, 256);
                        ui.end_row();
                        if include_secret {
                            ui.label(tr!("MICROPHONE", ui.ctx()));
                            ui.checkbox(
                                &mut self.microphone_forwarding,
                                tr!("Allow microphone forwarding for this connection", ui.ctx()),
                            );
                            ui.end_row();
                            ui.label("");
                            ui.label(tr!(
                                "PCM audio forwarded to the Linux SSH session",
                                ui.ctx()
                            ));
                            ui.end_row();
                        }
                        ui.label(tr!("Authentication", ui.ctx()));
                        ui.horizontal_wrapped(|ui| {
                            let previous = self.authentication;
                            ui.selectable_value(
                                &mut self.authentication,
                                Authentication::Password,
                                tr!("Password", ui.ctx()),
                            );
                            ui.selectable_value(
                                &mut self.authentication,
                                Authentication::Key,
                                tr!("Private key", ui.ctx()),
                            );
                            ui.selectable_value(
                                &mut self.authentication,
                                Authentication::Agent,
                                tr!("SSH agent", ui.ctx()),
                            );
                            if previous != self.authentication {
                                self.secret = Zeroizing::new(String::new());
                            }
                        });
                        ui.end_row();
                        if self.authentication == Authentication::Key {
                            ui.label(tr!("Private key path", ui.ctx()));
                            edit(ui, &mut self.key_path, 4096);
                            ui.end_row();
                        }
                        if include_secret && self.authentication != Authentication::Agent {
                            ui.label(tr!("Password / passphrase", ui.ctx()));
                            self.secret_edit(ui);
                            ui.end_row();
                        }
                        ui.label(tr!("X11 forwarding", ui.ctx()));
                        egui::ComboBox::from_id_salt("ssh-x11-mode")
                            .selected_text(x11_label(self.x11, ui.ctx()))
                            .show_ui(ui, |ui| {
                                for mode in [
                                    X11ForwardingMode::Off,
                                    X11ForwardingMode::Untrusted,
                                    X11ForwardingMode::Trusted,
                                ] {
                                    ui.selectable_value(
                                        &mut self.x11,
                                        mode,
                                        x11_label(mode, ui.ctx()),
                                    );
                                }
                            })
                            .response
                            .accessible_name(tr!("SSH X11 forwarding mode", ui.ctx()));
                        ui.end_row();
                    }
                }
                Protocol::Serial => {
                    ui.label(tr!("Serial port name", ui.ctx()));
                    edit(ui, &mut self.device, 512);
                    ui.end_row();
                    ui.label(tr!("Baud (8N1, no flow control)", ui.ctx()));
                    ui.add(egui::DragValue::new(&mut self.baud).range(1..=4_000_000));
                    ui.end_row();
                }
            });
    }

    fn prompt(&mut self, ui: &mut egui::Ui, prompt: &TerminalPrompt) -> Option<Action> {
        ssh_prompt(ui, prompt, &mut self.secret).map(|action| match action {
            PromptAction::Respond(id) => Action::Respond(id),
            PromptAction::Disconnect => Action::Disconnect,
        })
    }

    fn secret_edit(&mut self, ui: &mut egui::Ui) {
        secret_edit(ui, &mut self.secret);
    }

    pub(super) fn profile(&self) -> Result<SessionProfile> {
        let mut profile = match self.protocol {
            Protocol::Ssh => {
                let mut profile =
                    SessionProfile::new("Temporary SSH", self.host.trim(), self.username.trim());
                let config = profile.ssh_mut().context("missing SSH draft")?;
                config.port = self.port;
                config.x11_forwarding = self.x11;
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
                    target.x11_forwarding = draft.x11_forwarding;
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
                let profile = self.profile()?;
                client.connect_with_options(
                    profile,
                    secret,
                    TerminalConnectionOptions {
                        automatic_reconnect: self.automatic_reconnect,
                        microphone_forwarding: self.microphone_forwarding,
                    },
                )
            }
            Action::Disconnect => {
                self.microphone_forwarding = false;
                client.disconnect()
            }
            Action::Respond(id) => client.respond(id, secret),
        }
    }
}

fn edit(ui: &mut egui::Ui, text: &mut String, limit: usize) {
    ui.add(egui::TextEdit::singleline(text).char_limit(limit));
}

fn x11_label(mode: X11ForwardingMode, ctx: &egui::Context) -> &'static str {
    match mode {
        X11ForwardingMode::Off => tr!("Off", ctx),
        X11ForwardingMode::Untrusted => tr!("Untrusted (-X)", ctx),
        X11ForwardingMode::Trusted => tr!("Trusted (-Y)", ctx),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ax_ssh::controller::TerminalPromptKind;

    fn texts(output: &egui::FullOutput) -> Vec<(String, egui::Rect)> {
        fn collect(shape: &egui::epaint::Shape, result: &mut Vec<(String, egui::Rect)>) {
            match shape {
                egui::epaint::Shape::Text(text) => {
                    result.push((text.galley.job.text.clone(), text.visual_bounding_rect()))
                }
                egui::epaint::Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect(shape, result);
                    }
                }
                _ => {}
            }
        }
        let mut result = Vec::new();
        for shape in &output.shapes {
            collect(&shape.shape, &mut result);
        }
        result
    }

    fn draw_prompt(
        ctx: &egui::Context,
        form: &mut ConnectionForm,
        prompt: &TerminalPrompt,
        events: Vec<egui::Event>,
    ) -> (Option<Action>, egui::FullOutput) {
        let mut action = None;
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1600.0, 1200.0),
                )),
                events,
                focused: true,
                ..Default::default()
            },
            |ui| {
                action = form.prompt(ui, prompt);
            },
        );
        (action, output)
    }

    #[test]
    fn localized_trust_prompts_preserve_fingerprints_and_never_confirm_a_blocked_key() {
        use crate::egui_app::i18n::{Translator, current};
        let fonts = crate::egui_app::fonts::load().unwrap();
        for language in ax_ssh::config::UiLanguage::bundled() {
            let ctx = egui::Context::default();
            ctx.set_fonts(fonts.clone());
            Translator::new(language, None).apply(&ctx);
            let mut form = ConnectionForm {
                secret: Zeroizing::new("fixture-must-remain-private".into()),
                ..Default::default()
            };
            let mut prompt = TerminalPrompt {
                id: uuid::Uuid::new_v4(),
                host: "user-entered.example".into(),
                port: 2222,
                username: "original-user".into(),
                kind: TerminalPromptKind::HostKey {
                    expected: None,
                    actual: "SHA256:observed-fixture".into(),
                    confirmable: true,
                },
            };
            let (_, output) = draw_prompt(&ctx, &mut form, &prompt, vec![]);
            let painted = texts(&output);
            assert!(
                painted
                    .iter()
                    .any(|(text, _)| text == "SHA256:observed-fixture")
            );
            assert!(
                painted
                    .iter()
                    .all(|(text, _)| !text.contains("fixture-must-remain-private"))
            );
            let label = current(&ctx).text("", "Trust once and connect");
            let position = painted
                .iter()
                .find(|(text, _)| text == label)
                .unwrap()
                .1
                .center();
            output.drop_without_applying_deltas();
            let click = |pressed| {
                vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]
            };
            let (_, output) = draw_prompt(&ctx, &mut form, &prompt, click(true));
            output.drop_without_applying_deltas();
            let (action, output) = draw_prompt(&ctx, &mut form, &prompt, click(false));
            assert!(
                matches!(action, Some(Action::Respond(id)) if id == prompt.id),
                "{}",
                language.as_setting()
            );
            output.drop_without_applying_deltas();
            if let TerminalPromptKind::HostKey {
                confirmable,
                expected,
                ..
            } = &mut prompt.kind
            {
                *confirmable = false;
                *expected = Some("SHA256:expected-fixture".into());
            }
            let (_, output) = draw_prompt(&ctx, &mut form, &prompt, click(true));
            assert!(texts(&output).iter().all(|(text, _)| text != label));
            assert!(
                texts(&output)
                    .iter()
                    .any(|(text, _)| text == "SHA256:expected-fixture")
            );
            output.drop_without_applying_deltas();
            let (action, output) = draw_prompt(&ctx, &mut form, &prompt, click(false));
            assert!(!matches!(action, Some(Action::Respond(_))));
            output.drop_without_applying_deltas();
            assert_eq!(prompt.host, "user-entered.example");
        }
    }

    #[tokio::test]
    async fn changing_language_preserves_connection_expansion_and_draft_values() {
        use crate::egui_app::i18n::Translator;
        use ax_ssh::config::UiLanguage;
        let terminal = ax_ssh::controller::TerminalSession::spawn(
            &tokio::runtime::Handle::current(),
            "axssh-no-such-fixture-shell".into(),
            std::sync::Arc::new(|| {}),
        );
        let client = terminal.client();
        let view = client.view().unwrap();
        let profile = SessionProfile::new(
            "Untranslated saved name",
            "original.example",
            "original-user",
        );
        let mut form = ConnectionForm::from_profile(&profile);
        let ctx = egui::Context::default();
        ctx.set_fonts(crate::egui_app::fonts::load().unwrap());
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        let output = ctx.run_ui(Default::default(), |ui| {
            form.show(ui, &client, &view).unwrap();
        });
        assert!(texts(&output).iter().any(|(text, _)| text == "Host"));
        let title = texts(&output)
            .into_iter()
            .find(|(text, _)| text == "Connection")
            .unwrap()
            .1
            .center();
        output.drop_without_applying_deltas();
        for pressed in [true, false] {
            let output = ctx.run_ui(
                egui::RawInput {
                    events: vec![
                        egui::Event::PointerMoved(title),
                        egui::Event::PointerButton {
                            pos: title,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    ..Default::default()
                },
                |ui| {
                    form.show(ui, &client, &view).unwrap();
                },
            );
            output.drop_without_applying_deltas();
        }
        for language in UiLanguage::bundled() {
            Translator::new(language, None).apply(&ctx);
            let output = ctx.run_ui(Default::default(), |ui| {
                form.show(ui, &client, &view).unwrap();
            });
            let host = crate::egui_app::i18n::current(&ctx).text("", "Host");
            assert!(texts(&output).iter().all(|(text, _)| text != host));
            output.drop_without_applying_deltas();
            assert_eq!(form.profile().unwrap(), profile);
        }
        terminal.shutdown().await.unwrap();
    }

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
        form.x11 = X11ForwardingMode::Untrusted;
        assert_eq!(
            form.profile().unwrap().ssh().unwrap().x11_forwarding,
            X11ForwardingMode::Untrusted
        );
    }

    #[test]
    fn localized_x11_mode_selection_updates_the_saved_draft_without_changing_trust() {
        use crate::egui_app::i18n::{Translator, current};
        let fonts = crate::egui_app::fonts::load().unwrap();
        for language in ax_ssh::config::UiLanguage::bundled() {
            let ctx = egui::Context::default();
            ctx.set_fonts(fonts.clone());
            Translator::new(language, None).apply(&ctx);
            let mut profile = SessionProfile::new("Settings", "fixture.example", "fixture");
            let ssh = profile.ssh_mut().unwrap();
            ssh.x11_forwarding = X11ForwardingMode::Off;
            ssh.host_key_fingerprint = Some("fixture-existing-pin".into());
            let mut form = ConnectionForm::from_profile(&profile);
            let draw = |form: &mut ConnectionForm, events| {
                ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1600.0, 1200.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| form.fields(ui, false),
                )
            };
            let output = draw(&mut form, vec![]);
            let position = texts(&output)
                .into_iter()
                .find(|(text, _)| text == current(&ctx).text("", "Off"))
                .unwrap()
                .1
                .center();
            output.drop_without_applying_deltas();
            let click = |position, pressed| {
                vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]
            };
            for pressed in [true, false] {
                draw(&mut form, click(position, pressed)).drop_without_applying_deltas();
            }
            let output = draw(&mut form, vec![]);
            let position = texts(&output)
                .into_iter()
                .find(|(text, _)| text == current(&ctx).text("", "Untrusted (-X)"))
                .unwrap()
                .1
                .center();
            output.drop_without_applying_deltas();
            for pressed in [true, false] {
                draw(&mut form, click(position, pressed)).drop_without_applying_deltas();
            }
            let edited = form.profile().unwrap();
            assert_eq!(
                edited.ssh().unwrap().x11_forwarding,
                X11ForwardingMode::Untrusted,
                "{}",
                language.as_setting()
            );
            assert_eq!(
                edited.ssh().unwrap().host_key_fingerprint,
                profile.ssh().unwrap().host_key_fingerprint
            );
            assert_eq!(edited.id, profile.id);
        }
    }

    #[test]
    fn drafts_validate_without_storage_or_optional_ssh_services() {
        let mut form = ConnectionForm::default();
        assert!(!form.microphone_forwarding);
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

    #[test]
    fn localized_microphone_choice_is_explicit_and_never_becomes_profile_data() {
        use crate::egui_app::i18n::{Translator, current};
        let fonts = crate::egui_app::fonts::load().unwrap();
        for language in ax_ssh::config::UiLanguage::bundled() {
            let ctx = egui::Context::default();
            ctx.set_fonts(fonts.clone());
            Translator::new(language, None).apply(&ctx);
            let mut profile = SessionProfile::new("fixture", "fixture.example", "fixture");
            profile.ssh_mut().unwrap().x11_forwarding = X11ForwardingMode::Off;
            let mut form = ConnectionForm::from_profile(&profile);
            assert!(!form.microphone_forwarding);
            let draw = |form: &mut ConnectionForm, events, include_secret| {
                ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1600.0, 1200.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| form.fields(ui, include_secret),
                )
            };
            let output = draw(&mut form, vec![], true);
            let label = current(&ctx).text("", "Allow microphone forwarding for this connection");
            let position = texts(&output)
                .into_iter()
                .find(|(text, _)| text == label)
                .unwrap()
                .1
                .center();
            output.drop_without_applying_deltas();
            for pressed in [true, false] {
                draw(
                    &mut form,
                    vec![
                        egui::Event::PointerMoved(position),
                        egui::Event::PointerButton {
                            pos: position,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    true,
                )
                .drop_without_applying_deltas();
            }
            assert!(form.microphone_forwarding);
            assert_eq!(form.profile().unwrap(), profile);
            assert!(!ConnectionForm::from_profile(&form.profile().unwrap()).microphone_forwarding);
            let output = draw(&mut form, vec![], false);
            assert!(texts(&output).into_iter().all(|(text, _)| text != label));
            output.drop_without_applying_deltas();
            let output = draw(&mut form, vec![], true);
            let position = texts(&output)
                .into_iter()
                .find(|(text, _)| text == "Telnet")
                .unwrap()
                .1
                .center();
            output.drop_without_applying_deltas();
            for pressed in [true, false] {
                draw(
                    &mut form,
                    vec![
                        egui::Event::PointerMoved(position),
                        egui::Event::PointerButton {
                            pos: position,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    true,
                )
                .drop_without_applying_deltas();
            }
            assert!(form.protocol == Protocol::Telnet);
            assert!(!form.microphone_forwarding);
        }
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

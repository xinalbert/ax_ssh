//! Shared frontend SSH trust/authentication prompt; decisions remain controller-owned.
use super::super::i18n::tr;
use ax_ssh::controller::{TerminalPrompt, TerminalPromptKind};
use eframe::egui;
use uuid::Uuid;
use zeroize::Zeroizing;

pub(crate) enum PromptAction {
    Respond(Uuid),
    Disconnect,
}

pub(crate) fn ssh_prompt(
    ui: &mut egui::Ui,
    prompt: &TerminalPrompt,
    secret: &mut Zeroizing<String>,
) -> Option<PromptAction> {
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
            ui.horizontal_wrapped(|ui| {
                ui.label(tr!("Detected key", ui.ctx()));
                ui.monospace(actual);
            });
            if let Some(expected) = expected {
                ui.horizontal_wrapped(|ui| {
                    ui.label(tr!("Expected key", ui.ctx()));
                    ui.monospace(expected);
                });
            }
            if *confirmable {
                ui.label(tr!("Verify the fingerprint with the server owner before trusting it for this connection.", ui.ctx()));
            } else {
                ui.label(tr!("Host key blocked by trust policy or changed during connection. Disconnect and check the server and known_hosts.", ui.ctx()));
            }
            *confirmable
        }
        TerminalPromptKind::Authentication => {
            ui.label(tr!(
                "Enter fresh credentials to connect or retry.",
                ui.ctx()
            ));
            true
        }
    };
    let mut action = None;
    if allowed {
        ui.horizontal(|ui| {
            ui.label(tr!("Password / key passphrase (empty for agent)", ui.ctx()));
            secret_edit(ui, secret);
        });
        let label = if matches!(prompt.kind, TerminalPromptKind::HostKey { .. }) {
            tr!("Trust once and connect", ui.ctx())
        } else {
            tr!("Retry", ui.ctx())
        };
        if ui.button(label).clicked() {
            action = Some(PromptAction::Respond(prompt.id));
        }
    }
    if ui.button(tr!("Cancel / disconnect", ui.ctx())).clicked() {
        action = Some(PromptAction::Disconnect);
    }
    action
}

pub(crate) fn secret_edit(ui: &mut egui::Ui, secret: &mut Zeroizing<String>) {
    let mut output = egui::TextEdit::singleline(&mut **secret)
        .id(ui.scope_id().with("temporary-connection-secret"))
        .password(true)
        .char_limit(16 * 1024)
        .show(ui);
    // egui edit history must not restore a submitted credential.
    output.state.clear_undoer();
    output.state.store(ui.ctx(), output.response.id);
}

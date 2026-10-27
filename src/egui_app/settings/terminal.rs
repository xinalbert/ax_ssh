//! Terminal settings reuse the domain's ranges; the controller applies saved values.

use super::super::i18n::tr;
use ax_ssh::config::{self, TerminalSettings};
use eframe::egui;

pub(super) fn show(
    ui: &mut egui::Ui,
    settings: &mut TerminalSettings,
    appearance: &mut config::AppearanceSettings,
    available_shells: &[String],
) {
    super::shells::show(ui, settings, available_shells);
    ui.separator();
    ui.add(
        egui::Slider::new(
            &mut settings.scrollback_lines,
            config::MIN_SCROLLBACK_LINES..=config::MAX_SCROLLBACK_LINES,
        )
        .text(tr!("Scrollback", ui.ctx())),
    );
    ui.horizontal_wrapped(|ui| {
        ui.label(tr!("Default terminal size", ui.ctx()));
        ui.add(
            egui::DragValue::new(&mut settings.default_columns)
                .range(config::MIN_TERMINAL_COLUMNS..=config::MAX_TERMINAL_COLUMNS)
                .suffix(format!(" {}", tr!("columns", ui.ctx()))),
        );
        ui.add(
            egui::DragValue::new(&mut settings.default_rows)
                .range(config::MIN_TERMINAL_ROWS..=config::MAX_TERMINAL_ROWS)
                .suffix(format!(" {}", tr!("rows", ui.ctx()))),
        );
    });
    ui.checkbox(
        &mut settings.alternate_scroll_to_home,
        tr!(
            "Move to line start when scrolling the alternate screen",
            ui.ctx()
        ),
    );
    ui.separator();
    #[cfg(target_os = "macos")]
    {
        ui.checkbox(
            &mut settings.option_as_meta,
            tr!("Option acts as Meta", ui.ctx()),
        );
        ui.label(tr!(
            "Send Option+key as an Escape-prefixed Meta key",
            ui.ctx()
        ));
    }
    ui.checkbox(
        &mut settings.osc52_clipboard,
        tr!("Allow remote OSC 52 clipboard access", ui.ctx()),
    );
    ui.label(tr!(
        "Allow writes to the default clipboard; ask before remote reads",
        ui.ctx()
    ));
    ui.separator();
    ui.checkbox(
        &mut appearance.terminal_mouse_local_selection_priority,
        tr!("Local selection priority", ui.ctx()),
    );
    ui.label(tr!(
        "On: Alt/Option sends mouse gestures; off: standard xterm with Shift selection",
        ui.ctx()
    ));
    ui.checkbox(
        &mut appearance.right_click_copy_or_paste,
        tr!("Right-click copy or paste", ui.ctx()),
    );
    ui.label(tr!("Copy a selection; otherwise paste", ui.ctx()));
    ui.checkbox(
        &mut appearance.copy_selection_on_select,
        tr!("Copy selection on select", ui.ctx()),
    );
    ui.label(tr!("Right-click always pastes", ui.ctx()));
    ui.label(tr!(
        "Behavior changes apply after Save. Default size and shell apply to new terminals.",
        ui.ctx()
    ));
}

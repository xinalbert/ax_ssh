//! Resolve the shared palette into egui styles without a second theme schema.
use ax_ssh::config::ThemeSettings;
use eframe::egui::{self, Color32, Stroke};

#[derive(Default)]
pub(super) struct Theme {
    applied: Option<ThemeSettings>,
}
impl Theme {
    pub(super) fn apply(&mut self, ctx: &egui::Context, settings: &ThemeSettings) {
        if self.applied.as_ref() == Some(settings) {
            return;
        }
        let preference = match settings.mode {
            ax_ssh::config::ThemeMode::System => egui::ThemePreference::System,
            ax_ssh::config::ThemeMode::Light => egui::ThemePreference::Light,
            ax_ssh::config::ThemeMode::Dark => egui::ThemePreference::Dark,
        };
        ctx.set_theme(preference);
        for (theme, palette, mut visuals) in [
            (
                egui::Theme::Light,
                settings.light_palette(),
                egui::Visuals::light(),
            ),
            (
                egui::Theme::Dark,
                settings.dark_palette(),
                egui::Visuals::dark(),
            ),
        ] {
            let color = |text: &str| Color32::from_hex(text).unwrap_or(visuals.text_color());
            let text = color(&palette.text);
            let panel = color(&palette.panel);
            let alternate = color(&palette.panel_alt);
            let border = color(&palette.border);
            let accent = color(&palette.accent);
            let background = color(&palette.background);
            let selection = color(&palette.terminal_selection);
            let danger = color(&palette.danger);
            visuals.override_text_color = Some(text);
            visuals.panel_fill = panel;
            visuals.window_fill = panel;
            visuals.extreme_bg_color = background;
            visuals.faint_bg_color = alternate;
            visuals.window_stroke = Stroke::new(1.0, border);
            visuals.hyperlink_color = accent;
            visuals.error_fg_color = danger;
            visuals.selection.bg_fill = selection;
            visuals.selection.stroke = Stroke::new(1.0, accent);
            for widget in [
                &mut visuals.widgets.noninteractive,
                &mut visuals.widgets.inactive,
                &mut visuals.widgets.hovered,
                &mut visuals.widgets.active,
                &mut visuals.widgets.open,
            ] {
                widget.bg_stroke = Stroke::new(1.0, border);
                widget.fg_stroke = Stroke::new(1.0, text);
            }
            visuals.widgets.inactive.bg_fill = alternate;
            visuals.widgets.hovered.bg_fill = accent.linear_multiply(0.25);
            visuals.widgets.active.bg_fill = accent.linear_multiply(0.4);
            ctx.set_visuals_of(theme, visuals);
        }
        self.applied = Some(settings.clone());
    }
}

use super::super::i18n::{self, mark, tr};
use super::*;

#[derive(Clone)]
struct TabDrag(Uuid);

impl WorkspaceUi {
    pub(super) fn tabs(
        &mut self,
        ui: &mut egui::Ui,
        view: &WorkspaceView,
        width: f32,
        appearance: &ax_ssh::config::AppearanceSettings,
    ) -> Option<Action> {
        let mut action = None;
        egui::ScrollArea::horizontal()
            .id_salt("tab-strip")
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (index, tab) in view.tabs.iter().enumerate() {
                        ui.push_id(tab.id, |ui| {
                            let active = view.active_tab_id == Some(tab.id);
                            let label = if tab.settings {
                                tr!("Settings", ui.ctx())
                            } else {
                                &tab.title
                            };
                            let title =
                                format!("{} {}", if tab.settings { "⚙" } else { "▣" }, label);
                            let icon = if tab.settings {
                                None
                            } else {
                                self.icons.image(
                                    ui,
                                    super::super::system_icons::tab_icon(tab, appearance),
                                    appearance.system_icons_follow_theme,
                                )
                            };
                            let button = icon
                                .map_or_else(
                                    || egui::Button::new(title),
                                    |image| egui::Button::image_and_text(image, label),
                                )
                                .selected(active)
                                .truncate()
                                .sense(Sense::click_and_drag());
                            let response = ui
                                .add_sized([width - 28.0, 28.0], button)
                                .on_hover_text(label);
                            if active && self.last_active != Some(tab.id) {
                                response.scroll_to_me(Some(egui::Align::Center));
                            }
                            if response.clicked() {
                                action = Some(Action::Layout(WorkspaceAction::Activate(tab.id)));
                            }
                            if response.clicked_by(egui::PointerButton::Middle) {
                                action = Some(Action::Layout(WorkspaceAction::CloseTab(tab.id)));
                            }
                            response.dnd_set_drag_payload(TabDrag(tab.id));
                            if let Some(drag) = response.dnd_release_payload::<TabDrag>()
                                && drag.0 != tab.id
                            {
                                action = Some(Action::Layout(WorkspaceAction::MoveTab {
                                    id: drag.0,
                                    index,
                                }));
                            }
                            response.context_menu(|ui| {
                                for (label, next) in [
                                    (
                                        mark!("Move Tab Left"),
                                        WorkspaceAction::MoveTab {
                                            id: tab.id,
                                            index: index.saturating_sub(1),
                                        },
                                    ),
                                    (
                                        mark!("Move Tab Right"),
                                        WorkspaceAction::MoveTab {
                                            id: tab.id,
                                            index: index + 1,
                                        },
                                    ),
                                    (
                                        mark!("Close Current Tab"),
                                        WorkspaceAction::CloseTab(tab.id),
                                    ),
                                ] {
                                    if ui.button(i18n::current(ui.ctx()).text("", label)).clicked()
                                    {
                                        action = Some(Action::Layout(next));
                                        ui.close();
                                    }
                                }
                            });
                            if ui
                                .add_sized([22.0, 28.0], egui::Button::new("×"))
                                .accessible_name(tr!("Close Current Tab", ui.ctx()))
                                .on_hover_text(tr!("Close Current Tab", ui.ctx()))
                                .clicked()
                            {
                                action = Some(Action::Layout(WorkspaceAction::CloseTab(tab.id)));
                            }
                        });
                    }
                    ui.menu_button("+", |ui| {
                        for (label, next) in [
                            (mark!("New Local Shell"), WorkspaceAction::NewLocal),
                            (mark!("New Connection"), WorkspaceAction::NewConnection),
                            (mark!("Settings…"), WorkspaceAction::OpenSettings),
                        ] {
                            if ui.button(i18n::current(ui.ctx()).text("", label)).clicked() {
                                action = Some(Action::Layout(next));
                                ui.close();
                            }
                        }
                    });
                });
            });
        action
    }
}

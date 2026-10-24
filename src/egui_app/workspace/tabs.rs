use super::*;

#[derive(Clone)]
struct TabDrag(Uuid);

impl WorkspaceUi {
    pub(super) fn tabs(
        &mut self,
        ui: &mut egui::Ui,
        view: &WorkspaceView,
        width: f32,
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
                            let title =
                                format!("{} {}", if tab.settings { "⚙" } else { "▣" }, tab.title);
                            let button = egui::Button::new(title)
                                .selected(active)
                                .truncate()
                                .sense(Sense::click_and_drag());
                            let response = ui
                                .add_sized([width - 28.0, 28.0], button)
                                .on_hover_text(&tab.title);
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
                                        "Move left",
                                        WorkspaceAction::MoveTab {
                                            id: tab.id,
                                            index: index.saturating_sub(1),
                                        },
                                    ),
                                    (
                                        "Move right",
                                        WorkspaceAction::MoveTab {
                                            id: tab.id,
                                            index: index + 1,
                                        },
                                    ),
                                    ("Close tab", WorkspaceAction::CloseTab(tab.id)),
                                ] {
                                    if ui.button(label).clicked() {
                                        action = Some(Action::Layout(next));
                                        ui.close();
                                    }
                                }
                            });
                            if ui
                                .add_sized([22.0, 28.0], egui::Button::new("×"))
                                .accessible_name("Close tab")
                                .on_hover_text("Close tab")
                                .clicked()
                            {
                                action = Some(Action::Layout(WorkspaceAction::CloseTab(tab.id)));
                            }
                        });
                    }
                    ui.menu_button("+", |ui| {
                        for (label, next) in [
                            ("New Local Shell", WorkspaceAction::NewLocal),
                            ("New Connection", WorkspaceAction::NewConnection),
                            ("Settings…", WorkspaceAction::OpenSettings),
                        ] {
                            if ui.button(label).clicked() {
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

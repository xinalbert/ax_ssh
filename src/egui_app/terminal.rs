//! Fixed-grid Painter terminal. UI retains interaction and bounded visible-row caches only.

use anyhow::Result;
use ax_ssh::controller::{SelectionKind, TerminalClient, TerminalView};
use ax_ssh::terminal::{TerminalCursorShape, TerminalSelectionRange};
use ax_ssh::terminal_dimensions::TerminalSize;
use eframe::egui::{self, Color32, Rect, Sense, Stroke, Vec2};

mod geometry;
mod input;
mod paint;

pub(super) const BACKGROUND: Color32 = Color32::from_rgb(24, 26, 32);
const FONT_SIZE: f32 = 15.0;

#[derive(Default)]
pub(super) struct TerminalPane {
    cache: Vec<paint::CachedRow>,
    appearance: Option<ax_ssh::config::AppearanceSettings>,
    input: input::Input,
    size: Option<TerminalSize>,
    generation: Option<u64>,
    last_grid: Option<geometry::Grid>,
    last_client: Option<TerminalClient>,
    last_ppp: f32,
    widget_id: Option<egui::Id>,
    selection_revision: i32,
    selection: Option<TerminalSelectionRange>,
    anchor: Option<(usize, usize)>,
    focused: bool,
    last_input_frame: Option<u64>,
    next_selection_request: u64,
    pending_selection: Option<u64>,
}

impl TerminalPane {
    pub(super) fn configure(&mut self, appearance: &ax_ssh::config::AppearanceSettings) {
        self.appearance = Some(appearance.clone());
    }

    pub(super) fn surrender_focus(&self, ctx: &egui::Context) {
        if let Some(id) = self.widget_id {
            ctx.memory_mut(|memory| memory.surrender_focus(id));
        }
    }
    pub(super) fn clear_cache(&mut self) {
        self.cache.clear();
    }

    pub(super) fn suspend(&mut self) -> Result<()> {
        if let Some(client) = &self.last_client {
            if let Some(grid) = &self.last_grid {
                self.input.cancel_remote_press(
                    client,
                    grid,
                    None,
                    egui::Modifiers::default(),
                    self.last_ppp,
                )?;
            }
            if self.focused {
                client.focus(false)?;
            }
        }
        self.input.clear();
        self.focused = false;
        self.clear_cache();
        Ok(())
    }

    pub(super) fn focused(&self) -> bool {
        self.focused
    }

    #[cfg(test)]
    pub(super) fn show(
        &mut self,
        ui: &mut egui::Ui,
        client: &TerminalClient,
        view: &TerminalView,
    ) -> Result<()> {
        self.show_with_focus(ui, client, view, self.last_input_frame.is_none())
    }

    pub(super) fn show_with_focus(
        &mut self,
        ui: &mut egui::Ui,
        client: &TerminalClient,
        view: &TerminalView,
        request_focus: bool,
    ) -> Result<()> {
        if self.generation != Some(view.generation) {
            self.generation = Some(view.generation);
            self.size = None;
            self.clear_cache();
            self.input.clear();
            self.selection = None;
            self.anchor = None;
            self.pending_selection = None;
            self.focused = false;
        }
        let client = &client.for_view(view);
        let font_size = self
            .appearance
            .as_ref()
            .map_or(FONT_SIZE, |settings| f32::from(settings.terminal_font_size));
        let line_height = self.appearance.as_ref().map_or(1.0, |settings| {
            f32::from(settings.terminal_line_height_percent) / 100.0
        });
        let font = super::fonts::font(font_size, false, false);
        let ppp = ui.ctx().pixels_per_point();
        let cell =
            ui.fonts_mut(|fonts| Vec2::new(fonts.glyph_width(&font, 'M'), fonts.row_height(&font)));
        let cell = Vec2::new(
            (cell.x * ppp).round().max(1.0) / ppp,
            (cell.y * line_height * ppp).ceil().max(1.0) / ppp,
        );
        let (_, rect) = ui.allocate_space(ui.available_size().max(Vec2::splat(1.0)));
        let response = ui
            .interact(
                rect,
                ui.scope_id().with("axssh-terminal"),
                Sense::click_and_drag(),
            )
            .accessible_name("Terminal");
        self.widget_id = Some(response.id);
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::accesskit::Role::TextInput, true, "Terminal")
        });
        let geometry = geometry::Grid::new(rect, cell, ppp);
        self.last_grid = Some(geometry);
        self.last_client = Some(client.clone());
        self.last_ppp = ppp;
        if !ui.is_sizing_pass() && self.size != Some(geometry.size) {
            client.resize(
                geometry.size,
                (cell.x * ppp).round() as u32,
                (cell.y * ppp).round() as u32,
            )?;
            self.size = Some(geometry.size);
        }
        if self.selection_revision != view.selection_revision {
            self.selection = None;
            self.anchor = None;
            self.selection_revision = view.selection_revision;
            self.pending_selection = None;
        }
        if response.clicked() || response.drag_started() {
            response.request_focus();
        }
        if request_focus && !ui.is_sizing_pass() {
            response.request_focus();
        }
        let frame = ui.ctx().cumulative_frame_nr();
        let focused = ui.is_enabled() && response.has_focus() && ui.input(|i| i.focused);
        if !ui.is_sizing_pass() && self.last_input_frame != Some(frame) {
            self.last_input_frame = Some(frame);
            if focused != self.focused {
                client.focus(focused)?;
                self.focused = focused;
                if !focused {
                    self.input.cancel_remote_press(
                        client,
                        &geometry,
                        ui.input(|input| input.pointer.interact_pos()),
                        ui.input(|input| input.modifiers),
                        ppp,
                    )?;
                    self.input.clear();
                    self.selection = None;
                }
            }
            self.input.handle(
                ui,
                &response,
                client,
                view,
                &geometry,
                input::Selection {
                    range: &mut self.selection,
                    anchor: &mut self.anchor,
                },
            )?;
            if (response.double_clicked() || response.triple_clicked())
                && let Some(pos) = response.interact_pointer_pos()
            {
                let (row, column) = geometry.hit(pos);
                self.next_selection_request = self.next_selection_request.wrapping_add(1).max(1);
                let request = self.next_selection_request;
                client.select(
                    if response.triple_clicked() {
                        SelectionKind::Line
                    } else {
                        SelectionKind::Semantic
                    },
                    row,
                    column,
                    request,
                    view.selection_revision,
                )?;
                self.pending_selection = Some(request);
            }
            if let Some(request) = self.pending_selection
                && let Some(result) =
                    client.take_selection_matching(request, view.selection_revision)?
            {
                self.selection = result.range;
                self.pending_selection = None;
            }
            if let Some(text) = client.take_clipboard()? {
                ui.ctx().copy_text(text);
            }
        }
        let palette = self
            .appearance
            .as_ref()
            .map(|settings| {
                let palette = if ui.visuals().dark_mode {
                    settings.theme.dark_palette()
                } else {
                    settings.theme.light_palette()
                };
                [
                    Color32::from_hex(&palette.terminal_foreground).unwrap_or(Color32::LIGHT_GRAY),
                    Color32::from_hex(&palette.terminal_background).unwrap_or(BACKGROUND),
                ]
            })
            .unwrap_or([Color32::from_rgb(220, 223, 230), BACKGROUND]);
        let painter = ui.painter().with_clip_rect(rect);
        painter.rect_filled(
            rect,
            0.0,
            paint::resolve(view.terminal.background_color, palette[1]),
        );
        paint::rows(
            &painter,
            view,
            &geometry,
            &mut self.cache,
            font_size,
            ppp,
            palette,
        );
        if let Some(range) = self.selection {
            for row in range.start_row..=range.end_row {
                let start = if row == range.start_row {
                    range.start_column
                } else {
                    0
                };
                let end = if row == range.end_row {
                    range.end_column + 1
                } else {
                    view.terminal.max_columns
                };
                painter.rect_filled(
                    geometry.span(row, start, end.saturating_sub(start)),
                    0.0,
                    Color32::from_rgba_unmultiplied(100, 140, 220, 80),
                );
            }
        }
        let now = ui.input(|i| i.time);
        let blink = self
            .appearance
            .as_ref()
            .is_none_or(|settings| settings.terminal_cursor_blink)
            && focused
            && view.terminal.cursor_blinking
            && self.selection.is_none()
            && self.input.preedit.is_empty();
        if blink {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f64(
                    0.5 - now.rem_euclid(0.5),
                ));
        }
        if view.terminal.cursor_visible && (!blink || now.rem_euclid(1.0) < 0.5) {
            let cursor = geometry.span(
                view.terminal.cursor_row,
                view.terminal.cursor_column,
                view.terminal.cursor_cells,
            );
            let color = paint::resolve(view.terminal.cursor_color, Color32::LIGHT_GRAY);
            match view.terminal.cursor_shape {
                TerminalCursorShape::Beam => {
                    painter.rect_filled(
                        Rect::from_min_size(cursor.min, Vec2::new(2.0 / ppp, cell.y)),
                        0.0,
                        color,
                    );
                }
                TerminalCursorShape::Underline => {
                    painter.line_segment(
                        [cursor.left_bottom(), cursor.right_bottom()],
                        Stroke::new(2.0 / ppp, color),
                    );
                }
                TerminalCursorShape::Block if focused => {
                    painter.rect_filled(cursor, 0.0, color);
                    painter.text(
                        cursor.min,
                        egui::Align2::LEFT_TOP,
                        &view.terminal.cursor_text,
                        font.clone(),
                        palette[1],
                    );
                }
                TerminalCursorShape::Block | TerminalCursorShape::HollowBlock => {
                    painter.rect_stroke(
                        cursor.shrink(0.5 / ppp),
                        0.0,
                        Stroke::new(1.0 / ppp, color),
                        egui::StrokeKind::Inside,
                    );
                }
            }
        }
        if focused {
            let cursor = geometry.span(view.terminal.cursor_row, view.terminal.cursor_column, 1);
            ui.output_mut(|output| {
                output.ime = Some(egui::output::IMEOutput {
                    purpose: egui::IMEPurpose::Terminal,
                    rect,
                    cursor_rect: cursor,
                    should_interrupt_composition: false,
                });
            });
            if !self.input.preedit.is_empty() {
                painter.rect_filled(
                    Rect::from_min_size(
                        cursor.min,
                        Vec2::new(
                            (self.input.preedit.chars().count() as f32 + 1.0) * cell.x,
                            cell.y,
                        ),
                    ),
                    0.0,
                    palette[1],
                );
                painter.text(
                    cursor.min,
                    egui::Align2::LEFT_TOP,
                    &self.input.preedit,
                    font,
                    Color32::WHITE,
                );
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[cfg(unix)]
    fn two_line_reader_script() -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt as _;

        let path = std::env::temp_dir().join(format!("axssh-egui-ime-{}.sh", uuid::Uuid::new_v4()));
        std::fs::write(
            &path,
            "#!/bin/sh\nstty -echo\nprintf 'AX_EGUI_READY\\n'\nIFS= read -r first\nprintf 'AX_EGUI_FIRST:%s\\n' \"$first\"\nIFS= read -r second\nprintf 'AX_EGUI_SECOND:%s\\n' \"$second\"\n",
        )
        .expect("test shell script should be written");
        let mut permissions = std::fs::metadata(&path)
            .expect("test shell script metadata should be readable")
            .permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&path, permissions)
            .expect("test shell script should be executable");
        path
    }

    #[cfg(unix)]
    fn terminal_text(view: &ax_ssh::controller::TerminalView) -> String {
        view.terminal
            .lines
            .iter()
            .flat_map(|line| &line.runs)
            .map(|run| run.text.as_str())
            .collect()
    }

    #[cfg(unix)]
    fn show_with_input(
        ctx: &egui::Context,
        pane: &mut TerminalPane,
        client: &ax_ssh::controller::TerminalClient,
        events: Vec<egui::Event>,
    ) {
        let view = client.view().expect("terminal view should be available");
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    Vec2::new(900.0, 600.0),
                )),
                focused: true,
                events,
                ..Default::default()
            },
            |ui| {
                pane.show(ui, client, &view)
                    .expect("terminal pane should render")
            },
        );
        output.drop_without_applying_deltas();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn pane_focus_and_disabled_editors_keep_text_in_the_correct_pty() {
        let script = two_line_reader_script();
        let runtime = tokio::runtime::Handle::current();
        let mut owners = Vec::new();
        for _ in 0..2 {
            owners.push(ax_ssh::controller::TerminalSession::spawn(
                &runtime,
                script.to_string_lossy().into_owned(),
                Arc::new(|| {}),
            ));
        }
        let clients = owners
            .iter()
            .map(|terminal| terminal.client())
            .collect::<Vec<_>>();
        for client in &clients {
            tokio::time::timeout(std::time::Duration::from_secs(10), async {
                while !terminal_text(&client.view().unwrap()).contains("AX_EGUI_READY") {
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
        }
        let ctx = egui::Context::default();
        ctx.set_fonts(super::super::fonts::load().unwrap());
        let mut panes = [TerminalPane::default(), TerminalPane::default()];
        let mut draw = |events: Vec<egui::Event>, focus: Option<usize>, enabled: bool| {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(1000.0, 600.0),
                    )),
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| {
                    if focus.is_some() {
                        for pane in &panes {
                            pane.surrender_focus(ui.ctx());
                        }
                    }
                    for index in 0..2 {
                        let rect = Rect::from_min_size(
                            egui::pos2(index as f32 * 490.0, 0.0),
                            Vec2::new(480.0, 580.0),
                        );
                        ui.scope_builder(
                            egui::UiBuilder::new().id_salt(index).max_rect(rect),
                            |ui| {
                                ui.add_enabled_ui(enabled, |ui| {
                                    panes[index]
                                        .show_with_focus(
                                            ui,
                                            &clients[index],
                                            &clients[index].view().unwrap(),
                                            focus == Some(index),
                                        )
                                        .unwrap()
                                });
                            },
                        );
                    }
                },
            );
            output.drop_without_applying_deltas();
        };
        draw(vec![], Some(0), true);
        draw(
            vec![egui::Event::Text("fixture-secret-must-not-send".into())],
            None,
            false,
        );
        let enter = egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        draw(
            vec![egui::Event::Text("first".into()), enter.clone()],
            Some(0),
            true,
        );
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            while !terminal_text(&clients[0].view().unwrap()).contains("AX_EGUI_FIRST:first") {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert!(!terminal_text(&clients[1].view().unwrap()).contains("AX_EGUI_FIRST:"));
        draw(
            vec![egui::Event::Text("second".into()), enter],
            Some(1),
            true,
        );
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            while !terminal_text(&clients[1].view().unwrap()).contains("AX_EGUI_FIRST:second") {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert!(!terminal_text(&clients[0].view().unwrap()).contains("AX_EGUI_SECOND:"));
        for owner in owners {
            owner.shutdown().await.unwrap();
        }
        std::fs::remove_file(script).unwrap();
    }

    #[tokio::test]
    async fn pane_draws_and_processes_input_without_a_native_window() {
        let ctx = egui::Context::default();
        ctx.set_fonts(super::super::fonts::load().unwrap());
        let terminal = ax_ssh::controller::TerminalSession::spawn(
            &tokio::runtime::Handle::current(),
            "axssh-no-such-test-shell".to_owned(),
            Arc::new(|| {}),
        );
        let client = terminal.client();
        let mut view = client.view().unwrap();
        let mut model = ax_ssh::terminal::TerminalModel::new(80, 24, 10);
        model.process("ASCII 中文 e\u{301}\r\n\x1b[31mred\x1b[0m".as_bytes());
        view.terminal = model.snapshot();
        let mut pane = TerminalPane::default();
        for events in [
            vec![],
            vec![egui::Event::Ime(egui::ImeEvent::Preedit {
                text: "拼音".to_owned(),
                active_range_chars: None,
            })],
            vec![egui::Event::Ime(egui::ImeEvent::Commit("输入".to_owned()))],
            vec![
                egui::Event::Text("duplicate".to_owned()),
                egui::Event::Ime(egui::ImeEvent::Commit("提交".to_owned())),
            ],
        ] {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(900.0, 600.0),
                    )),
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| {
                    pane.show(ui, &client, &view).unwrap();
                },
            );
            assert!(!output.shapes.is_empty());
            assert!(output.platform_output.ime.is_some());
            output.drop_without_applying_deltas();
        }
        assert!(pane.input.preedit.is_empty());
        assert_eq!(pane.cache.len(), 24);
        pane.clear_cache();
        assert!(pane.cache.is_empty());
        terminal.shutdown().await.unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn ime_commit_and_matching_text_reach_the_pty_once() {
        let script = two_line_reader_script();
        let terminal = ax_ssh::controller::TerminalSession::spawn(
            &tokio::runtime::Handle::current(),
            script.to_string_lossy().into_owned(),
            Arc::new(|| {}),
        );
        let client = terminal.client();
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let view = client.view().expect("terminal view should be available");
                if view.connected && terminal_text(&view).contains("AX_EGUI_READY") {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("test shell should become ready");

        let ctx = egui::Context::default();
        ctx.set_fonts(super::super::fonts::load().expect("test fonts should load"));
        let mut pane = TerminalPane::default();
        show_with_input(&ctx, &mut pane, &client, vec![]);
        show_with_input(
            &ctx,
            &mut pane,
            &client,
            vec![
                egui::Event::Text("深度发掘".to_owned()),
                egui::Event::Ime(egui::ImeEvent::Commit("深度发掘".to_owned())),
                egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let text =
                    terminal_text(&client.view().expect("terminal view should be available"));
                if text.contains("AX_EGUI_FIRST:深度发掘") {
                    assert_eq!(text.matches("深度发掘").count(), 1);
                    assert!(!text.contains("AX_EGUI_SECOND"));
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("only one IME submission should reach the shell");
        terminal
            .shutdown()
            .await
            .expect("terminal should shut down");
        std::fs::remove_file(script).expect("test shell script should be removed");
    }
}

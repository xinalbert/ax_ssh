use anyhow::Result;
use ax_ssh::controller::{TerminalClient, TerminalView};
use ax_ssh::terminal::{
    TerminalKey, TerminalModifiers, TerminalMouseButton, TerminalMouseEvent,
    TerminalMouseEventKind, TerminalMouseModifiers, TerminalSelectionRange,
};
use eframe::egui::{self, Event, ImeEvent, Key, Modifiers, PointerButton, Response};

use super::geometry::Grid;

#[derive(Default)]
pub(super) struct Input {
    pub(super) preedit: String,
    wheel: egui::Vec2,
    remote_press: Option<TerminalMouseButton>,
    pending_committed_text: Option<(String, u64)>,
}

pub(super) struct Selection<'a> {
    pub(super) range: &'a mut Option<TerminalSelectionRange>,
    pub(super) anchor: &'a mut Option<(usize, usize)>,
}

impl Input {
    pub(super) fn clear(&mut self) {
        self.preedit.clear();
        self.wheel = egui::Vec2::ZERO;
        self.remote_press = None;
        self.pending_committed_text = None;
    }

    pub(super) fn cancel_remote_press(
        &mut self,
        client: &TerminalClient,
        grid: &Grid,
        pos: Option<egui::Pos2>,
        modifiers: Modifiers,
        pixels_per_point: f32,
    ) -> Result<()> {
        let Some(button) = self.remote_press.take() else {
            return Ok(());
        };
        let pos = pos.unwrap_or(grid.origin);
        let (row, column) = grid.hit(pos);
        let (pixel_x, pixel_y) = grid.pixel_hit(pos, pixels_per_point);
        client.mouse(
            mouse(
                TerminalMouseEventKind::Release,
                button,
                row,
                column,
                modifiers,
            ),
            pixel_x,
            pixel_y,
            1,
        )
    }

    pub(super) fn handle(
        &mut self,
        ui: &mut egui::Ui,
        response: &Response,
        client: &TerminalClient,
        view: &TerminalView,
        grid: &Grid,
        selection: Selection<'_>,
    ) -> Result<()> {
        let Selection {
            range: selection,
            anchor,
        } = selection;
        let focused = ui.is_enabled() && response.has_focus() && ui.input(|i| i.focused);
        if focused {
            ui.memory_mut(|memory| {
                memory.set_focus_lock_filter(
                    response.id,
                    egui::EventFilter {
                        tab: true,
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        escape: true,
                    },
                )
            });
        }
        let frame = ui.ctx().cumulative_frame_nr();
        let events = ui.input(|i| i.events.clone());
        for event in events.iter().cloned() {
            match event {
                Event::Ime(event) if focused => {
                    if let Some(text) = self.composition(event, frame, &events) {
                        client.key(TerminalKey::Text(text), TerminalModifiers::default())?;
                    }
                }
                Event::Text(text) if focused && self.preedit.is_empty() => {
                    if !text.chars().any(char::is_control) {
                        let duplicate = self.committed_in_frame(&text, &events, frame);
                        if !duplicate {
                            let modifiers = ui.input(|i| i.modifiers);
                            client.key(
                                TerminalKey::Text(text),
                                TerminalModifiers {
                                    alt: modifiers.alt && !cfg!(target_os = "macos"),
                                    ..Default::default()
                                },
                            )?;
                        }
                    }
                }
                Event::Paste(text) if focused => client.paste(text.clone())?,
                Event::Copy if focused => {
                    if let Some(range) = selection {
                        client.copy(*range, view.selection_revision)?;
                    } else if !cfg!(target_os = "macos") {
                        client.key(
                            TerminalKey::Text("c".to_owned()),
                            TerminalModifiers {
                                control: true,
                                ..Default::default()
                            },
                        )?;
                    }
                }
                Event::Cut if focused && !cfg!(target_os = "macos") => client.key(
                    TerminalKey::Text("x".to_owned()),
                    TerminalModifiers {
                        control: true,
                        ..Default::default()
                    },
                )?,
                Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } if focused && self.preedit.is_empty() => {
                    if modifiers.command
                        && key == Key::A
                        && (cfg!(target_os = "macos") || modifiers.shift)
                    {
                        *selection = Some(TerminalSelectionRange {
                            start_row: 0,
                            start_column: 0,
                            end_row: view.terminal.lines.len().saturating_sub(1),
                            end_column: view.terminal.max_columns.saturating_sub(1),
                        });
                    } else if let Some(key) = terminal_key(key, modifiers) {
                        client.key(key, terminal_modifiers(modifiers))?;
                    }
                }
                Event::PointerButton {
                    pos,
                    button,
                    pressed,
                    modifiers,
                } if response.rect.contains(pos)
                    || self.remote_press.is_some()
                    || anchor.is_some() =>
                {
                    let (row, column) = grid.hit(pos);
                    let (pixel_x, pixel_y) = grid.pixel_hit(pos, ui.ctx().pixels_per_point());
                    let remote = view.terminal.mouse_button_reporting_active && !modifiers.shift;
                    if pressed && remote {
                        let button = mouse_button(button);
                        self.remote_press = Some(button);
                        *selection = None;
                        client.mouse(
                            mouse(
                                TerminalMouseEventKind::Press,
                                button,
                                row,
                                column,
                                modifiers,
                            ),
                            pixel_x,
                            pixel_y,
                            1,
                        )?;
                    } else if !pressed && self.remote_press.is_some() {
                        if let Some(button) = self.remote_press.take() {
                            client.mouse(
                                mouse(
                                    TerminalMouseEventKind::Release,
                                    button,
                                    row,
                                    column,
                                    modifiers,
                                ),
                                pixel_x,
                                pixel_y,
                                1,
                            )?;
                        }
                    } else if button == PointerButton::Primary {
                        if pressed {
                            *anchor = Some((row, column));
                            *selection = None;
                        } else if let Some(start) = anchor
                            && *start != (row, column)
                        {
                            *selection = Some(range(*start, (row, column)));
                        }
                    }
                }
                Event::PointerMoved(pos) => {
                    let (row, column) = grid.hit(pos);
                    let (pixel_x, pixel_y) = grid.pixel_hit(pos, ui.ctx().pixels_per_point());
                    if let Some(button) = self.remote_press {
                        client.mouse(
                            mouse(
                                TerminalMouseEventKind::Motion,
                                button,
                                row,
                                column,
                                ui.input(|i| i.modifiers),
                            ),
                            pixel_x,
                            pixel_y,
                            1,
                        )?;
                    } else if view.terminal.mouse_reporting.motion
                        && !ui.input(|i| i.modifiers.shift)
                        && response.hovered()
                    {
                        client.mouse(
                            mouse(
                                TerminalMouseEventKind::Motion,
                                TerminalMouseButton::None,
                                row,
                                column,
                                ui.input(|i| i.modifiers),
                            ),
                            pixel_x,
                            pixel_y,
                            1,
                        )?;
                    } else if response.dragged()
                        && let Some(start) = anchor
                    {
                        *selection = Some(range(*start, (row, column)));
                    }
                }
                Event::MouseWheel {
                    unit,
                    delta,
                    modifiers,
                    ..
                } if response.hovered() => {
                    let steps = self.wheel_steps(unit, delta, grid);
                    if view.terminal.mouse_wheel_reporting_active && !modifiers.shift {
                        let pos = ui.input(|i| i.pointer.hover_pos()).unwrap_or(grid.origin);
                        let (row, column) = grid.hit(pos);
                        let (pixel_x, pixel_y) = grid.pixel_hit(pos, ui.ctx().pixels_per_point());
                        for (steps, positive, negative) in [
                            (
                                steps.1,
                                TerminalMouseButton::WheelUp,
                                TerminalMouseButton::WheelDown,
                            ),
                            (
                                steps.0,
                                TerminalMouseButton::WheelLeft,
                                TerminalMouseButton::WheelRight,
                            ),
                        ] {
                            if steps == 0 {
                                continue;
                            }
                            client.mouse(
                                mouse(
                                    TerminalMouseEventKind::Press,
                                    if steps > 0 { positive } else { negative },
                                    row,
                                    column,
                                    modifiers,
                                ),
                                pixel_x,
                                pixel_y,
                                steps.unsigned_abs() as u16,
                            )?;
                        }
                    } else if steps.1 != 0 {
                        client.scroll(steps.1)?;
                    }
                }
                _ => {}
            }
        }
        if self.remote_press.is_some() && !ui.input(|input| input.pointer.any_down()) {
            self.cancel_remote_press(
                client,
                grid,
                ui.input(|input| input.pointer.interact_pos()),
                ui.input(|input| input.modifiers),
                ui.ctx().pixels_per_point(),
            )?;
        }
        Ok(())
    }

    fn wheel_steps(
        &mut self,
        unit: egui::MouseWheelUnit,
        delta: egui::Vec2,
        grid: &Grid,
    ) -> (i32, i32) {
        let delta = match unit {
            egui::MouseWheelUnit::Point => delta / grid.cell,
            egui::MouseWheelUnit::Line => delta * 3.0,
            egui::MouseWheelUnit::Page => {
                delta * egui::vec2(grid.size.columns() as f32, grid.size.rows() as f32)
            }
        };
        // Keep fractional touchpad motion, but never retain an unbounded backlog
        // after a large native event. A wheel transaction fits one command slot.
        self.wheel =
            (self.wheel + delta).clamp(egui::Vec2::splat(-256.0), egui::Vec2::splat(256.0));
        let steps = egui::vec2(self.wheel.x.trunc(), self.wheel.y.trunc());
        self.wheel -= steps;
        (steps.x as i32, steps.y as i32)
    }

    /// Returns text that should be written to the PTY for an IME event.
    ///
    /// `egui-winit` forwards IME commits and also forwards printable keyboard
    /// input as `Event::Text`. The two streams can describe the same commit,
    /// either in one frame or in adjacent frames, so a commit is authoritative
    /// and an equal text event is consumed exactly once.
    fn composition(&mut self, event: ImeEvent, frame: u64, events: &[Event]) -> Option<String> {
        match event {
            ImeEvent::Preedit { text, .. } => {
                self.preedit = text.chars().take(256).collect();
                None
            }
            ImeEvent::Commit(text) => {
                self.preedit.clear();
                self.pending_committed_text = (!text.is_empty()
                    && text.chars().count() <= 4_096
                    && !events.iter().any(
                        |event| matches!(event, Event::Text(candidate) if candidate == &text),
                    ))
                .then_some((text.clone(), frame));
                (!text.is_empty()).then_some(text)
            }
            _ => None,
        }
    }

    fn committed_in_frame(&mut self, text: &str, events: &[Event], frame: u64) -> bool {
        let committed_now = events.iter().any(
            |event| matches!(event, Event::Ime(ImeEvent::Commit(committed)) if committed == text),
        );
        if committed_now {
            self.pending_committed_text = None;
            return true;
        }
        let committed_previous =
            self.pending_committed_text
                .as_ref()
                .is_some_and(|(committed, committed_frame)| {
                    *committed_frame < frame
                        && frame.saturating_sub(*committed_frame) <= 1
                        && committed == text
                });
        if committed_previous
            || self
                .pending_committed_text
                .as_ref()
                .is_some_and(|(_, committed_frame)| frame.saturating_sub(*committed_frame) > 1)
        {
            self.pending_committed_text = None;
        }
        committed_previous
    }
}

fn terminal_modifiers(modifiers: Modifiers) -> TerminalModifiers {
    TerminalModifiers {
        alt: modifiers.alt && !cfg!(target_os = "macos"),
        control: modifiers.ctrl,
        meta: modifiers.mac_cmd,
        shift: modifiers.shift,
    }
}

fn terminal_key(key: Key, modifiers: Modifiers) -> Option<TerminalKey> {
    Some(match key {
        Key::Enter => TerminalKey::Return,
        Key::Backspace => TerminalKey::Backspace,
        Key::Tab => TerminalKey::Tab,
        Key::Escape => TerminalKey::Escape,
        Key::ArrowUp => TerminalKey::Up,
        Key::ArrowDown => TerminalKey::Down,
        Key::ArrowLeft => TerminalKey::Left,
        Key::ArrowRight => TerminalKey::Right,
        Key::Insert => TerminalKey::Insert,
        Key::Delete => TerminalKey::Delete,
        Key::Home => TerminalKey::Home,
        Key::End => TerminalKey::End,
        Key::PageUp => TerminalKey::PageUp,
        Key::PageDown => TerminalKey::PageDown,
        _ => {
            let name = key.name();
            if let Some(function) = name.strip_prefix('F').and_then(|n| n.parse::<u8>().ok()) {
                TerminalKey::Function(function)
            } else if modifiers.ctrl {
                let text = match key {
                    Key::Space => " ".to_owned(),
                    Key::OpenBracket => "[".to_owned(),
                    Key::CloseBracket => "]".to_owned(),
                    Key::Backslash => "\\".to_owned(),
                    _ if name.len() == 1 => name.to_lowercase(),
                    _ => return None,
                };
                TerminalKey::Text(text)
            } else {
                return None;
            }
        }
    })
}

fn mouse_button(button: PointerButton) -> TerminalMouseButton {
    match button {
        PointerButton::Primary => TerminalMouseButton::Left,
        PointerButton::Secondary => TerminalMouseButton::Right,
        PointerButton::Middle => TerminalMouseButton::Middle,
        _ => TerminalMouseButton::None,
    }
}

fn mouse(
    kind: TerminalMouseEventKind,
    button: TerminalMouseButton,
    row: usize,
    column: usize,
    modifiers: Modifiers,
) -> TerminalMouseEvent {
    TerminalMouseEvent {
        kind,
        button,
        row,
        column,
        modifiers: TerminalMouseModifiers {
            shift: modifiers.shift,
            alt: modifiers.alt,
            control: modifiers.ctrl,
        },
    }
}

fn range(a: (usize, usize), b: (usize, usize)) -> TerminalSelectionRange {
    let (start, end) = if a <= b { (a, b) } else { (b, a) };
    TerminalSelectionRange {
        start_row: start.0,
        start_column: start.1,
        end_row: end.0,
        end_column: end.1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composition_sends_only_commits_and_keeps_unicode_whole() {
        let mut input = Input::default();
        assert_eq!(
            input.composition(
                ImeEvent::Preedit {
                    text: "中文".to_owned(),
                    active_range_chars: None
                },
                10,
                &[],
            ),
            None
        );
        assert_eq!(input.preedit, "中文");
        assert_eq!(
            input.composition(ImeEvent::Commit("中文".to_owned()), 10, &[]),
            Some("中文".to_owned())
        );
        assert!(input.preedit.is_empty());
        assert!(input.committed_in_frame("中文", &[], 11));
        assert!(!input.committed_in_frame("中文", &[], 12));
        assert_eq!(
            input.composition(ImeEvent::Commit(String::new()), 13, &[]),
            None
        );
    }

    #[test]
    fn commit_without_preedit_suppresses_an_equal_text_in_the_next_frame() {
        let mut input = Input::default();
        assert_eq!(
            input.composition(ImeEvent::Commit("深度发掘".to_owned()), 40, &[]),
            Some("深度发掘".to_owned())
        );
        assert!(input.committed_in_frame("深度发掘", &[], 41));
        assert!(!input.committed_in_frame("other", &[], 42));
    }

    #[test]
    fn same_frame_ime_commit_suppresses_only_the_matching_text_event() {
        let mut input = Input::default();
        let events = vec![
            Event::Text("提交".to_owned()),
            Event::Ime(ImeEvent::Commit("提交".to_owned())),
            Event::Text("independent".to_owned()),
        ];
        assert!(input.committed_in_frame("提交", &events, 2));
        assert!(!input.committed_in_frame("independent", &events, 2));
    }

    #[test]
    fn same_frame_duplicate_does_not_hide_a_later_equal_text_input() {
        let mut input = Input::default();
        let events = vec![
            Event::Text("中文".to_owned()),
            Event::Ime(ImeEvent::Commit("中文".to_owned())),
        ];
        assert_eq!(
            input.composition(ImeEvent::Commit("中文".to_owned()), 50, &events),
            Some("中文".to_owned())
        );
        assert!(input.committed_in_frame("中文", &events, 50));
        assert!(!input.committed_in_frame("中文", &[], 51));
    }

    #[test]
    fn ordinary_text_is_not_also_emitted_from_key_events() {
        assert_eq!(terminal_key(Key::A, Modifiers::NONE), None);
        assert_eq!(
            terminal_key(Key::A, Modifiers::CTRL),
            Some(TerminalKey::Text("a".to_owned()))
        );
        assert_eq!(
            terminal_key(Key::ArrowUp, Modifiers::NONE),
            Some(TerminalKey::Up)
        );
    }

    #[test]
    fn command_modifiers_and_local_selection_are_distinct() {
        assert_eq!(
            terminal_modifiers(Modifiers::CTRL | Modifiers::ALT),
            TerminalModifiers {
                alt: !cfg!(target_os = "macos"),
                control: true,
                meta: false,
                shift: false,
            }
        );
        assert_eq!(
            range((2, 4), (1, 8)),
            TerminalSelectionRange {
                start_row: 1,
                start_column: 8,
                end_row: 2,
                end_column: 4,
            }
        );
    }

    #[test]
    fn terminal_mouse_reports_use_cell_and_pixel_coordinates() {
        let event = mouse(
            TerminalMouseEventKind::Press,
            TerminalMouseButton::Left,
            3,
            4,
            Modifiers::CTRL,
        );
        assert_eq!((event.row, event.column), (3, 4));
        assert!(event.modifiers.control);
    }

    #[test]
    fn wheel_keeps_fractional_axes_and_bounds_large_transactions() {
        let grid = Grid::new(
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 480.0)),
            egui::vec2(10.0, 20.0),
            1.0,
        );
        let mut input = Input::default();
        assert_eq!(
            input.wheel_steps(egui::MouseWheelUnit::Point, egui::vec2(4.0, 9.0), &grid),
            (0, 0)
        );
        assert_eq!(
            input.wheel_steps(egui::MouseWheelUnit::Point, egui::vec2(6.0, 11.0), &grid),
            (1, 1)
        );
        assert_eq!(
            input.wheel_steps(egui::MouseWheelUnit::Line, egui::vec2(-1.0, 2.0), &grid),
            (-3, 6)
        );
        assert_eq!(
            input.wheel_steps(
                egui::MouseWheelUnit::Page,
                egui::vec2(10_000.0, -10_000.0),
                &grid
            ),
            (256, -256)
        );
        assert_eq!(
            input.wheel_steps(egui::MouseWheelUnit::Point, egui::Vec2::ZERO, &grid),
            (0, 0)
        );
        input.wheel_steps(egui::MouseWheelUnit::Point, egui::vec2(4.0, 9.0), &grid);
        input.clear();
        assert_eq!(input.wheel, egui::Vec2::ZERO);
    }
}

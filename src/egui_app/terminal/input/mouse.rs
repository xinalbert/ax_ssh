use super::*;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Interaction {
    pub(super) local_priority: bool,
    pub(super) auto_copy: bool,
    pub(super) right_click: bool,
}

impl From<&ax_ssh::config::AppearanceSettings> for Interaction {
    fn from(settings: &ax_ssh::config::AppearanceSettings) -> Self {
        Self {
            local_priority: settings.terminal_mouse_local_selection_priority,
            auto_copy: settings.copy_selection_on_select,
            right_click: settings.right_click_copy_or_paste,
        }
    }
}

#[derive(Clone, Copy)]
struct Click {
    row: usize,
    column: usize,
    x: usize,
    y: usize,
    modifiers: Modifiers,
}

#[derive(Clone, Copy)]
enum Press {
    Remote(TerminalMouseButton),
    Local {
        start: (usize, usize),
        dragged: bool,
        click: Option<Click>,
    },
}

#[derive(Default)]
pub(super) struct MouseInput {
    press: Option<Press>,
}

pub(super) struct MouseContext<'a> {
    pub(super) ui: &'a egui::Ui,
    pub(super) response: &'a Response,
    pub(super) client: &'a TerminalClient,
    pub(super) view: &'a TerminalView,
    pub(super) grid: &'a Grid,
}

impl Interaction {
    pub(super) fn forward(self, modifiers: Modifiers) -> bool {
        !modifiers.shift && (!self.local_priority || modifiers.alt)
    }
}

impl MouseInput {
    pub(super) fn cancel(
        &mut self,
        client: &TerminalClient,
        grid: &Grid,
        pos: Option<egui::Pos2>,
        modifiers: Modifiers,
        ppp: f32,
    ) -> Result<()> {
        if let Some(Press::Remote(button)) = self.press {
            let pos = pos.unwrap_or(grid.origin);
            let (row, column) = grid.hit(pos);
            let (x, y) = grid.pixel_hit(pos, ppp);
            client.mouse(
                mouse(
                    TerminalMouseEventKind::Release,
                    button,
                    row,
                    column,
                    modifiers,
                ),
                x,
                y,
                1,
            )?;
        }
        self.press = None;
        Ok(())
    }

    pub(super) fn handle(
        &mut self,
        event: Event,
        context: &MouseContext<'_>,
        selection: &mut Option<TerminalSelectionRange>,
        settings: Interaction,
    ) -> Result<bool> {
        let MouseContext {
            ui,
            response,
            client,
            view,
            grid,
        } = *context;
        match event {
            Event::PointerButton {
                pos,
                button,
                pressed,
                modifiers,
            } if response.rect.contains(pos) || self.press.is_some() => {
                let hit = grid.hit(pos);
                let (x, y) = grid.pixel_hit(pos, ui.ctx().pixels_per_point());
                if pressed {
                    if self.press.is_some() {
                        return Ok(false);
                    }
                    let reporting = view.connected && view.terminal.mouse_button_reporting_active;
                    if reporting && settings.forward(modifiers) {
                        let remote = mouse_button(button);
                        client.mouse(
                            mouse(
                                TerminalMouseEventKind::Press,
                                remote,
                                hit.0,
                                hit.1,
                                modifiers,
                            ),
                            x,
                            y,
                            1,
                        )?;
                        self.press = Some(Press::Remote(remote));
                        *selection = None;
                    } else if button == PointerButton::Primary {
                        self.press = Some(Press::Local {
                            start: hit,
                            dragged: false,
                            click: (reporting
                                && !modifiers.shift
                                && settings.local_priority
                                && !modifiers.alt)
                                .then_some(Click {
                                    row: hit.0,
                                    column: hit.1,
                                    x,
                                    y,
                                    modifiers,
                                }),
                        });
                        *selection = None;
                    } else if button == PointerButton::Secondary
                        && (settings.auto_copy || settings.right_click)
                    {
                        if !settings.auto_copy
                            && let Some(range) = selection.take()
                        {
                            client.copy(range, view.selection_revision)?;
                        } else if view.connected {
                            crate::egui_app::clipboard::request(
                                ui.ctx(),
                                response.id,
                                client,
                                view,
                            )?;
                            *selection = None;
                        }
                    }
                } else {
                    match self.press {
                        Some(Press::Remote(remote)) if mouse_button(button) == remote => {
                            client.mouse(
                                mouse(
                                    TerminalMouseEventKind::Release,
                                    remote,
                                    hit.0,
                                    hit.1,
                                    modifiers,
                                ),
                                x,
                                y,
                                1,
                            )?;
                            self.press = None;
                        }
                        Some(Press::Local {
                            start,
                            dragged,
                            click,
                        }) if button == PointerButton::Primary => {
                            self.press = None;
                            if hit != start {
                                *selection = Some(range(start, hit));
                                return Ok(true);
                            }
                            if !dragged
                                && !response.double_clicked()
                                && !response.triple_clicked()
                                && let Some(click) = click
                            {
                                client.mouse(
                                    mouse(
                                        TerminalMouseEventKind::Press,
                                        TerminalMouseButton::Left,
                                        click.row,
                                        click.column,
                                        click.modifiers,
                                    ),
                                    click.x,
                                    click.y,
                                    1,
                                )?;
                                // Retain ownership until the reliable release is accepted.
                                self.press = Some(Press::Remote(TerminalMouseButton::Left));
                                self.cancel(
                                    client,
                                    grid,
                                    Some(pos),
                                    modifiers,
                                    ui.ctx().pixels_per_point(),
                                )?;
                            }
                        }
                        _ => {}
                    }
                }
            }
            Event::PointerMoved(pos) => {
                let hit = grid.hit(pos);
                let (x, y) = grid.pixel_hit(pos, ui.ctx().pixels_per_point());
                let modifiers = ui.input(|input| input.modifiers);
                match &mut self.press {
                    Some(Press::Remote(button)) => client.mouse(
                        mouse(
                            TerminalMouseEventKind::Motion,
                            *button,
                            hit.0,
                            hit.1,
                            modifiers,
                        ),
                        x,
                        y,
                        1,
                    )?,
                    Some(Press::Local { start, dragged, .. }) => {
                        *dragged |= hit != *start;
                        *selection = (hit != *start).then(|| range(*start, hit));
                    }
                    None if view.connected
                        && view.terminal.mouse_reporting.motion
                        && settings.forward(modifiers)
                        && response.hovered() =>
                    {
                        client.mouse(
                            mouse(
                                TerminalMouseEventKind::Motion,
                                TerminalMouseButton::None,
                                hit.0,
                                hit.1,
                                modifiers,
                            ),
                            x,
                            y,
                            1,
                        )?
                    }
                    _ => {}
                }
            }
            Event::PointerGone => self.cancel(
                client,
                grid,
                None,
                ui.input(|input| input.modifiers),
                ui.ctx().pixels_per_point(),
            )?,
            _ => {}
        }
        Ok(false)
    }

    pub(super) fn finish(&mut self, context: &MouseContext<'_>) -> Result<()> {
        if self.press.is_some() && !context.ui.input(|input| input.pointer.any_down()) {
            self.cancel(
                context.client,
                context.grid,
                context.ui.input(|input| input.pointer.interact_pos()),
                context.ui.input(|input| input.modifiers),
                context.ui.ctx().pixels_per_point(),
            )?;
        }
        Ok(())
    }
}

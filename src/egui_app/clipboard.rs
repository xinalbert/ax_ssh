//! Local paste requests stay bound to the terminal that requested them.
use anyhow::Result;
use ax_ssh::controller::{TerminalClient, TerminalView};
use eframe::egui;

#[derive(Clone)]
struct PendingPaste {
    owner: egui::Id,
    client: TerminalClient,
    generation: u64,
    revision: i32,
    frame: u64,
    delivered: Option<(u64, bool)>,
}

fn id() -> egui::Id {
    egui::Id::unique("terminal-paste-request")
}

pub(super) fn request(
    ctx: &egui::Context,
    owner: egui::Id,
    client: &TerminalClient,
    view: &TerminalView,
) -> Result<()> {
    anyhow::ensure!(view.connected, "terminal is not connected");
    ctx.memory_mut(|memory| memory.request_focus(owner));
    let frame = ctx.cumulative_frame_nr();
    ctx.data_mut(|data| {
        data.insert_temp(
            id(),
            PendingPaste {
                owner,
                client: client.for_view(view),
                generation: view.generation,
                revision: view.selection_revision,
                frame,
                delivered: None,
            },
        )
    });
    ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaste);
    Ok(())
}

/// eframe 0.37 reads RequestPaste synchronously after drawing and queues its
/// result for the next input frame. Keep no clipboard text in this ticket.
pub(super) fn guard(ctx: &egui::Context, visible: impl FnOnce(egui::Id) -> bool) {
    let Some(mut pending) = ctx.data(|data| data.get_temp::<PendingPaste>(id())) else {
        return;
    };
    let (has_paste, interrupted) = ctx.input(|input| {
        (
            input
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::Paste(_) | egui::Event::PasteImage(_))),
            !input.focused
                || input.events.iter().any(|event| {
                    matches!(
                        event,
                        egui::Event::PointerButton { pressed: true, .. }
                            | egui::Event::Key { pressed: true, .. }
                            | egui::Event::Text(_)
                            | egui::Event::Ime(_)
                    )
                }),
        )
    });
    let frame = ctx.cumulative_frame_nr();
    if pending
        .delivered
        .is_some_and(|(delivered, _)| delivered != frame)
        || frame.saturating_sub(pending.frame) > 2
    {
        ctx.data_mut(|data| data.remove::<PendingPaste>(id()));
        return;
    }
    if !has_paste {
        return;
    }
    let valid = pending
        .delivered
        .map(|(_, valid)| valid)
        .unwrap_or_else(|| {
            !interrupted
                && visible(pending.owner)
                && ctx.memory(|memory| memory.focused()) == Some(pending.owner)
                && pending.client.view().is_ok_and(|view| {
                    view.connected
                        && view.generation == pending.generation
                        && view.selection_revision == pending.revision
                })
        });
    if !valid {
        ctx.input_mut(|input| {
            input.events.retain(|event| {
                !matches!(event, egui::Event::Paste(_) | egui::Event::PasteImage(_))
            })
        });
    }
    pending.delivered = Some((frame, valid));
    ctx.data_mut(|data| data.insert_temp(id(), pending));
}

pub(super) fn allows(ctx: &egui::Context, owner: egui::Id, view: &TerminalView) -> bool {
    ctx.data(|data| data.get_temp::<PendingPaste>(id()))
        .is_none_or(|pending| {
            pending.owner == owner
                && pending.generation == view.generation
                && pending.revision == view.selection_revision
                && pending
                    .delivered
                    .is_some_and(|(frame, allowed)| allowed && frame == ctx.cumulative_frame_nr())
        })
}

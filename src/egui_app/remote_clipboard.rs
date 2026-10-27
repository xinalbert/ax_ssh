//! Authorized native reads run off the UI thread and retain a join.
use anyhow::{Context, Result};
use ax_ssh::controller::{TerminalClient, TerminalView};
use eframe::egui;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::{mpsc, watch};
use uuid::Uuid;

#[derive(Clone)]
struct Pending {
    owner: egui::Id,
    terminal: TerminalClient,
    token: Uuid,
    generation: u64,
    revision: i32,
    live: Arc<AtomicBool>,
    frame: u64,
    viewport: Option<egui::Rect>,
    pixels_per_point: f32,
    font_revision: u64,
}

impl Pending {
    fn current(&self) -> bool {
        self.live.load(Ordering::Acquire)
            && self
                .terminal
                .clipboard_read()
                .is_ok_and(|token| token == Some(self.token))
            && self.terminal.view().is_ok_and(|view| {
                view.connected
                    && view.generation == self.generation
                    && view.selection_revision == self.revision
            })
    }
}

struct Completion {
    token: Uuid,
    text: Result<String>,
}

#[derive(Clone)]
pub(super) struct Client {
    requests: mpsc::Sender<Pending>,
    completed: Arc<Mutex<Option<Completion>>>,
}

pub(super) struct Service {
    stop: watch::Sender<bool>,
    task: tokio::task::JoinHandle<Result<()>>,
}

fn key() -> egui::Id {
    egui::Id::unique("remote-clipboard-read")
}

impl Service {
    pub(super) fn spawn(runtime: &tokio::runtime::Handle, ctx: &egui::Context) -> (Self, Client) {
        Self::with_reader(
            runtime,
            ctx,
            Arc::new(|| {
                arboard::Clipboard::new()
                    .context("cannot open clipboard")?
                    .get_text()
                    .context("cannot read clipboard text")
            }),
        )
    }

    fn with_reader(
        runtime: &tokio::runtime::Handle,
        ctx: &egui::Context,
        read: Arc<dyn Fn() -> Result<String> + Send + Sync>,
    ) -> (Self, Client) {
        let (requests, mut receiver) = mpsc::channel::<Pending>(1);
        let completed = Arc::new(Mutex::new(None));
        let results = completed.clone();
        let (stop, mut cancelled) = watch::channel(false);
        let ctx = ctx.clone();
        let task = runtime.spawn(async move {
            loop {
                let request = tokio::select! {
                    biased;
                    _ = cancelled.changed() => break,
                    request = receiver.recv() => match request { Some(request) => request, None => break },
                };
                if !request.current() { continue; }
                let reader = read.clone();
                let admitted = request.clone();
                // Retain and join this job if cancellation arrives during native I/O.
                let text = tokio::task::spawn_blocking(move || -> Result<Option<String>> {
                    if !admitted.current() { return Ok(None); }
                    let text = reader()?;
                    anyhow::ensure!(text.len() <= ax_ssh::terminal::MAX_OSC52_CLIPBOARD_BYTES,
                        "clipboard exceeds the OSC 52 limit (64 KiB)");
                    Ok(Some(text))
                }).await.context("clipboard read task failed")?;
                if *cancelled.borrow() { break; }
                if request.current() {
                    let text = match text { Ok(Some(text)) => Ok(text), Ok(None) => continue, Err(error) => Err(error) };
                    *results.lock().map_err(|_| anyhow::anyhow!("clipboard result lock poisoned"))? = Some(Completion { token: request.token, text });
                    ctx.request_repaint();
                }
            }
            results.lock().map_err(|_| anyhow::anyhow!("clipboard result lock poisoned"))?.take();
            Ok(())
        });
        (
            Self { stop, task },
            Client {
                requests,
                completed,
            },
        )
    }

    pub(super) async fn shutdown(self) -> Result<()> {
        self.stop.send_replace(true);
        self.task.await.context("clipboard service failed")?
    }
}

impl Client {
    pub(super) fn install(&self, ctx: &egui::Context) {
        ctx.data_mut(|data| data.insert_temp(key(), self.clone()));
    }

    pub(super) fn guard(
        &self,
        ctx: &egui::Context,
        visible: impl FnOnce(egui::Id) -> bool,
    ) -> Result<()> {
        let Some(pending) = ctx.data(|data| data.get_temp::<Pending>(key())) else {
            self.completed
                .lock()
                .map_err(|_| anyhow::anyhow!("clipboard result lock poisoned"))?
                .take();
            return Ok(());
        };
        if pending.frame == ctx.cumulative_frame_nr() {
            // A repeated pass can restore the Allow button's old focus. Transfer
            // focus again before rebuilding its disabled, pending controls.
            ctx.memory_mut(|memory| memory.request_focus(pending.owner));
            return Ok(());
        }
        let valid = pending.current()
            && ctx.input(|input| input.viewport().inner_rect) == pending.viewport
            && ctx.pixels_per_point() == pending.pixels_per_point
            && super::fonts::view(ctx).revision == pending.font_revision
            && visible(pending.owner)
            && ctx.memory(|memory| memory.focused()) == Some(pending.owner)
            && ctx.input(|input| {
                input.focused
                    && !input.events.iter().any(|event| {
                        matches!(
                            event,
                            egui::Event::PointerButton { pressed: true, .. }
                                | egui::Event::Key { pressed: true, .. }
                                | egui::Event::Text(_)
                                | egui::Event::Ime(_)
                                | egui::Event::MouseWheel { .. }
                        )
                    })
            });
        let result = self
            .completed
            .lock()
            .map_err(|_| anyhow::anyhow!("clipboard result lock poisoned"))?
            .take();
        let result = result.filter(|result| result.token == pending.token);
        if !valid || result.is_some() {
            pending.live.store(false, Ordering::Release);
            ctx.data_mut(|data| data.remove::<Pending>(key()));
            if valid && let Some(result) = result {
                match result.text {
                    Ok(text) => pending.terminal.respond_clipboard(
                        pending.token,
                        Some(text),
                        pending.revision,
                    )?,
                    Err(error) => {
                        pending.terminal.respond_clipboard(
                            pending.token,
                            None,
                            pending.revision,
                        )?;
                        return Err(error);
                    }
                }
            } else {
                pending
                    .terminal
                    .respond_clipboard(pending.token, None, pending.revision)?;
            }
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        Ok(())
    }
}

pub(super) fn request(
    ctx: &egui::Context,
    owner: egui::Id,
    terminal: &TerminalClient,
    view: &TerminalView,
    token: Uuid,
) -> Result<()> {
    anyhow::ensure!(
        ctx.data(|data| data.get_temp::<Pending>(key())).is_none(),
        "another clipboard read is pending"
    );
    let service = ctx
        .data(|data| data.get_temp::<Client>(key()))
        .context("clipboard service unavailable")?;
    let pending = Pending {
        owner,
        terminal: terminal.for_view(view),
        token,
        generation: view.generation,
        revision: view.selection_revision,
        live: Arc::new(AtomicBool::new(true)),
        frame: ctx.cumulative_frame_nr(),
        viewport: ctx.input(|input| input.viewport().inner_rect),
        pixels_per_point: ctx.pixels_per_point(),
        font_revision: super::fonts::view(ctx).revision,
    };
    anyhow::ensure!(pending.current(), "clipboard request expired");
    service
        .requests
        .try_send(pending.clone())
        .map_err(|_| anyhow::anyhow!("clipboard reader is busy or closed"))?;
    ctx.memory_mut(|memory| memory.request_focus(owner));
    ctx.data_mut(|data| data.insert_temp(key(), pending));
    Ok(())
}

pub(super) fn pending(ctx: &egui::Context, token: Uuid) -> bool {
    ctx.data(|data| data.get_temp::<Pending>(key()))
        .is_some_and(|pending| pending.token == token)
}

#[cfg(all(test, unix))]
mod tests;

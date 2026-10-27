//! One background conversion, with a UI-owned destination ticket and retained shutdown join.
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
    client: TerminalClient,
    token: Uuid,
    generation: u64,
    revision: i32,
    live: Arc<AtomicBool>,
    completed: Arc<Mutex<Option<Result<()>>>>,
    converted: bool,
    confirmed: bool,
    frame: u64,
    deadline: std::time::Instant,
    pixels_per_point: f32,
    font_revision: u64,
    viewport: Option<egui::Rect>,
}

impl Pending {
    fn current(&self) -> bool {
        self.live.load(Ordering::Acquire)
            && self.client.view().is_ok_and(|view| {
                view.connected
                    && view.generation == self.generation
                    && view.selection_revision == self.revision
            })
    }
    fn cancel(&self) -> Result<()> {
        self.live.store(false, Ordering::Release);
        self.client.finish_image(self.token, false)
    }
}

struct Request {
    pending: Pending,
    image: Arc<egui::ColorImage>,
}

#[derive(Clone)]
pub(super) struct Client {
    requests: mpsc::Sender<Request>,
    busy: Arc<AtomicBool>,
}

pub(super) struct Service {
    stop: watch::Sender<bool>,
    task: tokio::task::JoinHandle<Result<()>>,
}

fn key() -> egui::Id {
    egui::Id::unique("terminal-image-paste")
}
fn client_key() -> egui::Id {
    egui::Id::unique("terminal-image-paste-client")
}

impl Service {
    pub(super) fn spawn(runtime: &tokio::runtime::Handle, ctx: &egui::Context) -> (Self, Client) {
        let (requests, mut receiver) = mpsc::channel::<Request>(1);
        let busy = Arc::new(AtomicBool::new(false));
        let active = busy.clone();
        let (stop, mut cancelled) = watch::channel(false);
        let ctx = ctx.clone();
        let task = runtime.spawn(async move {
            loop {
                let request = tokio::select! {
                    biased;
                    _ = cancelled.changed() => break,
                    request = receiver.recv() => match request { Some(request) => request, None => break },
                };
                let pending = request.pending;
                let admitted = pending.clone();
                // Never abort this bounded blocking job; shutdown retains and awaits it.
                let png = tokio::task::spawn_blocking(move || {
                    if !admitted.current() { return Ok(None); }
                    encode(&request.image).map(Some)
                }).await.context("clipboard image encoder task failed")?;
                let result = match png {
                    Ok(Some(png)) if pending.current() && !*cancelled.borrow() => pending.client.prepare_image(pending.token, png, pending.revision),
                    Ok(_) => Ok(()),
                    Err(error) => Err(error),
                };
                if (!pending.current() || *cancelled.borrow()) && let Err(error) = pending.cancel() {
                    tracing::debug!(%error, "image paste cancellation queue closed");
                }
                *pending.completed.lock().map_err(|_| anyhow::anyhow!("image conversion result lock poisoned"))? = Some(result);
                active.store(false, Ordering::Release);
                ctx.request_repaint();
                if *cancelled.borrow() { break; }
            }
            Ok(())
        });
        (Self { stop, task }, Client { requests, busy })
    }

    pub(super) async fn shutdown(self) -> Result<()> {
        self.stop.send_replace(true);
        self.task.await.context("image paste service failed")?
    }
}

impl Client {
    pub(super) fn install(&self, ctx: &egui::Context) {
        ctx.data_mut(|data| data.insert_temp(client_key(), self.clone()));
    }

    pub(super) fn guard(
        &self,
        ctx: &egui::Context,
        visible: impl FnOnce(egui::Id) -> bool,
    ) -> Result<()> {
        let Some(mut pending) = ctx.data(|data| data.get_temp::<Pending>(key())) else {
            return Ok(());
        };
        let frame = ctx.cumulative_frame_nr();
        let interrupted = ctx.input(|input| {
            !input.focused
                || (frame != pending.frame
                    && input.events.iter().any(|event| {
                        matches!(
                            event,
                            egui::Event::PointerButton { pressed: true, .. }
                                | egui::Event::Key { pressed: true, .. }
                                | egui::Event::Text(_)
                                | egui::Event::Ime(_)
                                | egui::Event::Paste(_)
                                | egui::Event::PasteImage(_)
                                | egui::Event::MouseWheel { .. }
                        )
                    }))
        });
        let current = !interrupted
            && visible(pending.owner)
            && ctx.memory(|memory| memory.focused()) == Some(pending.owner)
            && ctx.pixels_per_point() == pending.pixels_per_point
            && super::fonts::view(ctx).revision == pending.font_revision
            && ctx.input(|input| input.viewport().inner_rect) == pending.viewport
            && std::time::Instant::now() < pending.deadline
            && pending.current();
        if !current {
            // Keep retrying the cleanup intent under backpressure; revocation is immediate.
            pending.cancel()?;
            ctx.data_mut(|data| data.remove::<Pending>(key()));
            return Ok(());
        }
        let result = pending
            .completed
            .lock()
            .map_err(|_| anyhow::anyhow!("image conversion result lock poisoned"))?
            .take();
        if let Some(result) = result {
            pending.converted = true;
            if let Err(error) = result {
                pending.cancel()?;
                ctx.data_mut(|data| data.remove::<Pending>(key()));
                return Err(error);
            }
        }
        if pending.converted {
            match pending.client.image_ready(pending.token)? {
                Some(true) if !pending.confirmed => {
                    pending.client.finish_image(pending.token, true)?;
                    pending.confirmed = true;
                }
                None => {
                    ctx.data_mut(|data| data.remove::<Pending>(key()));
                    return Ok(());
                }
                Some(_) => {}
            }
        }
        ctx.data_mut(|data| data.insert_temp(key(), pending));
        ctx.request_repaint_after(std::time::Duration::from_millis(50));
        Ok(())
    }
}

pub(super) fn request(
    ctx: &egui::Context,
    owner: egui::Id,
    terminal: &TerminalClient,
    view: &TerminalView,
    image: Arc<egui::ColorImage>,
) -> Result<()> {
    validate(&image)?;
    anyhow::ensure!(
        ctx.input(|input| !input.events.iter().any(|event| matches!(
            event,
            egui::Event::Text(_)
                | egui::Event::Ime(_)
                | egui::Event::PointerButton { pressed: true, .. }
                | egui::Event::MouseWheel { .. }
        ))),
        "image paste interrupted by new input"
    );
    anyhow::ensure!(
        view.connected
            && matches!(
                view.identity,
                ax_ssh::controller::ConnectionIdentity::Local
                    | ax_ssh::controller::ConnectionIdentity::Ssh { .. }
            ),
        "image attachments require Local Shell or SSH"
    );
    anyhow::ensure!(
        ctx.data(|data| data.get_temp::<Pending>(key())).is_none(),
        "another image attachment is busy"
    );
    let client = ctx
        .data(|data| data.get_temp::<Client>(client_key()))
        .context("image paste service is unavailable")?;
    anyhow::ensure!(
        client
            .busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok(),
        "image encoder is busy"
    );
    let pending = Pending {
        owner,
        client: terminal.for_view(view),
        token: Uuid::new_v4(),
        generation: view.generation,
        revision: view.selection_revision,
        live: Arc::new(AtomicBool::new(true)),
        completed: Arc::new(Mutex::new(None)),
        converted: false,
        confirmed: false,
        frame: ctx.cumulative_frame_nr(),
        deadline: std::time::Instant::now() + std::time::Duration::from_secs(40),
        pixels_per_point: ctx.pixels_per_point(),
        font_revision: super::fonts::view(ctx).revision,
        viewport: ctx.input(|input| input.viewport().inner_rect),
    };
    if client
        .requests
        .try_send(Request {
            pending: pending.clone(),
            image,
        })
        .is_err()
    {
        client.busy.store(false, Ordering::Release);
        anyhow::bail!("image encoder is busy or stopped");
    }
    ctx.data_mut(|data| data.insert_temp(key(), pending));
    ctx.request_repaint();
    Ok(())
}

pub(super) fn show(ctx: &egui::Context) -> Result<()> {
    let Some(pending) = ctx.data(|data| data.get_temp::<Pending>(key())) else {
        return Ok(());
    };
    let mut cancel = false;
    egui::Area::new(key().with("progress"))
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-12.0, 32.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(crate::egui_app::i18n::tr!("Paste", ui.ctx()));
                    let button = ui.button(crate::egui_app::i18n::tr!("Cancel", ui.ctx()));
                    cancel = button.clicked() && !ui.is_sizing_pass();
                });
            });
        });
    if cancel {
        pending.cancel()?;
        ctx.data_mut(|data| data.remove::<Pending>(key()));
    }
    Ok(())
}

fn validate(image: &egui::ColorImage) -> Result<()> {
    let [width, height] = image.size;
    anyhow::ensure!(
        width > 0
            && height > 0
            && width <= ax_ssh::media::MAX_IMAGE_SIDE
            && height <= ax_ssh::media::MAX_IMAGE_SIDE,
        "clipboard image must be at most 4096 × 4096 pixels"
    );
    anyhow::ensure!(
        image.pixels.len() == width * height,
        "invalid clipboard image pixels"
    );
    Ok(())
}

fn encode(image: &egui::ColorImage) -> Result<Vec<u8>> {
    validate(image)?;
    let rgba: Vec<u8> = image
        .pixels
        .iter()
        .flat_map(egui::Color32::to_srgba_unmultiplied)
        .collect();
    ax_ssh::media::encode_clipboard_png(image.size[0], image.size[1], &rgba)
}

#[cfg(test)]
mod tests;

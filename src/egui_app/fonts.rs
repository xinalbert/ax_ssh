//! Font intents and GUI application; one retained worker owns all font reads.

use anyhow::{Context, Result, ensure};
use ax_ssh::{
    config::AppearanceSettings,
    font_resources::{self, BUNDLED_UI_FONT_FAMILY},
};
use eframe::egui::{self, FontDefinitions, FontFamily, FontId};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::{runtime::Handle, sync::watch, task::JoinHandle};

mod loader;
pub(super) use loader::Loader;
pub(super) const STYLES: [&str; 4] = ["regular", "bold", "italic", "bold-italic"];

#[derive(Clone, Debug, PartialEq, Eq)]
struct Selection {
    application: String,
    terminal: String,
}

impl Default for Selection {
    fn default() -> Self {
        Self {
            application: BUNDLED_UI_FONT_FAMILY.into(),
            terminal: BUNDLED_UI_FONT_FAMILY.into(),
        }
    }
}

impl Selection {
    fn from_settings(settings: &AppearanceSettings) -> Result<Self> {
        Ok(Self {
            application: font_resources::valid_font_family(&settings.application_font_family)
                .context("invalid application font family")?,
            terminal: font_resources::valid_font_family(&settings.terminal_font_family)
                .context("invalid terminal font family")?,
        })
    }
}

#[derive(Clone, Default)]
struct Request {
    revision: u64,
    selection: Selection,
}

struct Loaded {
    definitions: FontDefinitions,
    application: String,
    terminal: String,
    synthetic_italic: [bool; 2],
    warnings: Vec<String>,
}

#[derive(Clone)]
struct Snapshot {
    request: Request,
    loaded: Arc<Loaded>,
}

#[derive(Clone)]
pub(super) struct FontView {
    pub(super) options: Arc<Vec<String>>,
    pub(super) pending: bool,
    pub(super) warnings: Arc<Vec<String>>,
    pub(super) revision: u64,
    pub(super) application: String,
    pub(super) terminal: String,
    synthetic_italic: [bool; 2],
}

impl Default for FontView {
    fn default() -> Self {
        Self {
            options: Arc::new(Vec::new()),
            pending: false,
            warnings: Arc::new(Vec::new()),
            revision: 0,
            application: BUNDLED_UI_FONT_FAMILY.into(),
            terminal: BUNDLED_UI_FONT_FAMILY.into(),
            synthetic_italic: [false; 2],
        }
    }
}

fn state_id() -> egui::Id {
    egui::Id::unique("axssh-font-state")
}

pub(super) fn view(ctx: &egui::Context) -> FontView {
    ctx.data(|data| data.get_temp::<FontView>(state_id()))
        .unwrap_or_default()
}

pub(super) fn synthetic_italic(ctx: &egui::Context) -> [bool; 2] {
    view(ctx).synthetic_italic
}

pub(super) fn font(size: f32, bold: bool, italic: bool) -> FontId {
    FontId::new(
        size,
        FontFamily::Name(
            format!(
                "terminal-{}",
                STYLES[usize::from(bold) + 2 * usize::from(italic)]
            )
            .into(),
        ),
    )
}

/// One UI producer and one latest-value request/result slot.
pub(super) struct FontClient {
    requests: watch::Sender<Request>,
    results: watch::Receiver<Snapshot>,
    options: Arc<Vec<String>>,
    requested: Request,
    applied: Option<u64>,
}

impl FontClient {
    pub(super) fn update(
        &mut self,
        ctx: &egui::Context,
        settings: &AppearanceSettings,
    ) -> Result<()> {
        let selection = Selection::from_settings(settings)?;
        if selection != self.requested.selection {
            self.requested = Request {
                revision: self
                    .requested
                    .revision
                    .checked_add(1)
                    .context("font request identity exhausted")?,
                selection,
            };
            self.requests.send_replace(self.requested.clone());
            ctx.request_repaint();
        }
        let snapshot = self.results.borrow_and_update().clone();
        let mut state = view(ctx);
        let current = snapshot.request.revision == self.requested.revision
            && snapshot.request.selection == self.requested.selection;
        if current && self.applied != Some(snapshot.request.revision) {
            ctx.set_fonts(snapshot.loaded.definitions.clone());
            self.applied = Some(snapshot.request.revision);
            state.revision = state
                .revision
                .checked_add(1)
                .context("font presentation identity exhausted")?;
            state.synthetic_italic = snapshot.loaded.synthetic_italic;
            state.application = snapshot.loaded.application.clone();
            state.terminal = snapshot.loaded.terminal.clone();
            state.warnings = Arc::new(snapshot.loaded.warnings.clone());
            ctx.request_repaint();
        }
        state.pending = !current;
        state.options = self.options.clone();
        ctx.data_mut(|data| data.insert_temp(state_id(), state));
        Ok(())
    }
}

/// The owner remains outside the native event loop and joins every accepted job.
pub(super) struct FontService {
    stop: watch::Sender<bool>,
    cancelled: Arc<AtomicBool>,
    task: JoinHandle<Result<()>>,
}

impl FontService {
    pub(super) fn spawn(
        runtime: &Handle,
        loader: Loader,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> (Self, FontClient) {
        let initial = Request::default();
        let options = loader.options.clone();
        let baseline = Arc::new(Loaded {
            definitions: loader.baseline.clone(),
            application: BUNDLED_UI_FONT_FAMILY.into(),
            terminal: BUNDLED_UI_FONT_FAMILY.into(),
            synthetic_italic: [false; 2],
            warnings: Vec::new(),
        });
        let (requests, updates) = watch::channel(initial.clone());
        let (results, snapshots) = watch::channel(Snapshot {
            request: initial.clone(),
            loaded: baseline,
        });
        let (stop, stopped) = watch::channel(false);
        let cancelled = Arc::new(AtomicBool::new(false));
        let task = runtime.spawn(run(
            loader,
            updates,
            results,
            stopped,
            cancelled.clone(),
            wake,
        ));
        (
            Self {
                stop,
                cancelled,
                task,
            },
            FontClient {
                requests,
                results: snapshots,
                options,
                requested: initial,
                applied: None,
            },
        )
    }

    pub(super) async fn shutdown(self) -> Result<()> {
        self.cancelled.store(true, Ordering::Release);
        self.stop.send_replace(true);
        self.task.await.context("font worker task failed")?
    }
}

async fn run(
    mut loader: Loader,
    mut requests: watch::Receiver<Request>,
    results: watch::Sender<Snapshot>,
    mut stop: watch::Receiver<bool>,
    cancelled: Arc<AtomicBool>,
    wake: Arc<dyn Fn() + Send + Sync>,
) -> Result<()> {
    loop {
        tokio::select! {
            biased;
            result = stop.changed() => { if result.is_err() || *stop.borrow() { break; } }
            result = requests.changed() => { if result.is_err() { break; } }
        }
        if cancelled.load(Ordering::Acquire) {
            break;
        }
        loop {
            let request = requests.borrow_and_update().clone();
            let selection = request.selection.clone();
            let cancellation = cancelled.clone();
            let (returned, loaded) = tokio::task::spawn_blocking(move || {
                let loaded = loader.load(&selection, &cancellation);
                (loader, loaded)
            })
            .await
            .context("font read job failed")?;
            loader = returned;
            if cancelled.load(Ordering::Acquire) {
                return Ok(());
            }
            if requests.borrow().revision != request.revision {
                continue;
            }
            results.send_replace(Snapshot {
                request,
                loaded: Arc::new(loaded?),
            });
            wake();
            break;
        }
    }
    Ok(())
}

pub(super) fn check_cancelled(cancelled: &AtomicBool) -> Result<()> {
    ensure!(!cancelled.load(Ordering::Acquire), "font load cancelled");
    Ok(())
}

#[cfg(test)]
pub(super) fn load() -> Result<FontDefinitions> {
    Ok(Loader::bundled()?.baseline)
}

#[cfg(test)]
mod tests;

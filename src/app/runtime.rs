use super::*;
use tokio::runtime::Builder;

pub(super) const MIN_TOKIO_WORKER_THREADS: usize = 2;
pub(super) const MAX_TOKIO_WORKER_THREADS: usize = 4;
pub(super) const MAX_TOKIO_BLOCKING_THREADS: usize = 8;
pub(super) const TOKIO_BLOCKING_THREAD_KEEP_ALIVE: Duration = Duration::from_secs(2);

pub(super) fn select_slint_renderer(preference: RendererPreference) -> Result<()> {
    let selector = if std::env::var_os("SLINT_BACKEND").is_some() {
        // Keep the standard Slint environment override available for diagnostics
        // and explicit software-renderer fallback runs.
        slint::BackendSelector::new()
    } else {
        slint::BackendSelector::new().backend_name(renderer_backend_name(preference).into())
    };

    selector.select().map_err(Into::into)
}

/// Reports the renderer that this process selected, including Slint's explicit
/// environment override. Renderer preference changes are restart-only, so the
/// terminal presentation policy must use this startup decision rather than a
/// Settings preview value.
pub(super) fn software_renderer_selected(preference: RendererPreference) -> bool {
    std::env::var("SLINT_BACKEND").map_or_else(
        |_| renderer_backend_name(preference) == "winit-software",
        |backend| backend == "winit-software",
    )
}

pub(super) fn configure_software_presentation(mode: SoftwarePresentationMode) {
    #[cfg(target_os = "macos")]
    softbuffer::set_macos_ca_backing_store_enabled(software_presentation_uses_backing_store(mode));

    #[cfg(not(target_os = "macos"))]
    let _ = mode;
}

pub(super) const fn software_presentation_uses_backing_store(
    mode: SoftwarePresentationMode,
) -> bool {
    matches!(mode, SoftwarePresentationMode::DamageBackingStore)
}

pub(super) fn tokio_worker_thread_count() -> usize {
    std::thread::available_parallelism()
        .map(|parallelism| parallelism.get())
        .map_or(
            MIN_TOKIO_WORKER_THREADS,
            tokio_worker_thread_count_for_parallelism,
        )
}

pub(super) fn tokio_worker_thread_count_for_parallelism(parallelism: usize) -> usize {
    parallelism.clamp(MIN_TOKIO_WORKER_THREADS, MAX_TOKIO_WORKER_THREADS)
}

pub(super) fn build_tokio_runtime(worker_threads: usize) -> Result<Runtime> {
    Builder::new_multi_thread()
        .worker_threads(worker_threads)
        .max_blocking_threads(MAX_TOKIO_BLOCKING_THREADS)
        .thread_keep_alive(TOKIO_BLOCKING_THREAD_KEEP_ALIVE)
        .thread_name("axssh-tokio")
        .enable_all()
        .build()
        .context("failed to build Tokio runtime")
}

pub(super) fn renderer_backend_name(preference: RendererPreference) -> &'static str {
    match preference {
        RendererPreference::Gpu => "winit-skia",
        RendererPreference::Software => "winit-software",
        RendererPreference::Automatic if cfg!(target_os = "macos") => "winit-skia",
        RendererPreference::Automatic => "winit-software",
    }
}

pub(super) fn load_startup_bundled_fonts(
    runtime: &Handle,
    font_registry: &Arc<Mutex<FontRegistry>>,
    selected_families: Vec<String>,
) -> Vec<LoadedBundledFont> {
    let resources = match font_registry.lock() {
        Ok(registry) => registry.resources(),
        Err(_) => {
            warn!("cannot access font resources during startup");
            return Vec::new();
        }
    };
    match runtime.block_on(async move {
        tokio::task::spawn_blocking(move || resources.load_bundled_fonts(&selected_families)).await
    }) {
        Ok(Ok(fonts)) => fonts,
        Ok(Err(error)) => {
            warn!(%error, "failed to read bundled font resources during startup");
            Vec::new()
        }
        Err(error) => {
            warn!(%error, "bundled font task failed during startup");
            Vec::new()
        }
    }
}

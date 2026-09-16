use super::*;
use std::fmt::Display;
use std::io::Write;
use std::path::Path;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use tokio::runtime::Builder;

pub(super) const MIN_TOKIO_WORKER_THREADS: usize = 2;
pub(super) const MAX_TOKIO_WORKER_THREADS: usize = 4;
pub(super) const MAX_TOKIO_BLOCKING_THREADS: usize = 8;
pub(super) const TOKIO_BLOCKING_THREAD_KEEP_ALIVE: Duration = Duration::from_secs(2);

const RENDERER_SKIA: &str = "winit-skia";
const RENDERER_SOFTWARE: &str = "winit-software";
const RENDERER_ENVIRONMENT: &str = "environment";
const RENDERER_FALLBACK_MARKER: &str = "renderer-software-fallback";

static RENDERER_SELECTION: OnceLock<RendererSelection> = OnceLock::new();
static RENDERER_WINDOW_COUNT: AtomicUsize = AtomicUsize::new(0);
static RENDERER_FAULT_COUNT: AtomicU64 = AtomicU64::new(0);
static RENDERER_SHADER_FAULT_COUNT: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone)]
pub(super) struct RendererSelection {
    pub(super) requested_backend: &'static str,
    pub(super) selected_backend: &'static str,
    pub(super) source: &'static str,
    pub(super) fallback_reason: Option<&'static str>,
    pub(super) metal_device: Option<String>,
}

impl RendererSelection {
    pub(super) fn uses_software(&self) -> bool {
        self.selected_backend == RENDERER_SOFTWARE
    }
}

#[derive(Debug, Clone)]
pub(super) struct RendererDiagnostics {
    pub(super) requested_backend: String,
    pub(super) selected_backend: String,
    pub(super) source: String,
    pub(super) fallback_reason: String,
    pub(super) metal_device: String,
    pub(super) window_count: usize,
    pub(super) fault_count: u64,
    pub(super) shader_fault_count: u64,
}

/// Makes every retired Slint winit window release its native surface on hide.
///
/// Slint's winit backend otherwise only makes a macOS window invisible. Its
/// `SLINT_DESTROY_WINDOW_ON_HIDE` opt-in suspends the renderer and drops the
/// native Winit window, which releases the associated Metal layer/drawables.
/// This must run before logging, Slint, or Tokio can create another thread.
pub(super) fn enable_slint_destroy_window_on_hide() {
    // SAFETY: `main` invokes this before logging initializes its writer thread
    // and before Slint or Tokio are initialized, so no concurrent environment
    // reads or writes can race with this process-wide update.
    unsafe {
        std::env::set_var("SLINT_DESTROY_WINDOW_ON_HIDE", "1");
    }
}

pub(super) fn select_slint_renderer(
    preference: RendererPreference,
    log_directory: &Path,
) -> Result<RendererSelection> {
    let requested_backend = renderer_backend_name(preference);
    if std::env::var_os("SLINT_BACKEND").is_some() {
        // Keep the standard Slint environment override available for diagnostics
        // and explicit software-renderer fallback runs.
        slint::BackendSelector::new()
            .select()
            .context("failed to select Slint renderer from SLINT_BACKEND")?;
        let selection = RendererSelection {
            requested_backend,
            selected_backend: environment_renderer_backend(),
            source: RENDERER_ENVIRONMENT,
            fallback_reason: None,
            metal_device: selected_metal_device(environment_renderer_backend()),
        };
        remember_renderer_selection(selection.clone());
        return Ok(selection);
    }

    if preference == RendererPreference::Automatic
        && requested_backend == RENDERER_SKIA
        && renderer_fallback_marker_exists(log_directory)
    {
        warn!(
            target: "ax_ssh::diagnostics",
            event = "renderer-fallback",
            requested_renderer = requested_backend,
            selected_renderer = RENDERER_SOFTWARE,
            reason = "previous-renderer-fault",
            "using the software renderer after a previous GPU renderer fault"
        );
        slint::BackendSelector::new()
            .backend_name(RENDERER_SOFTWARE.into())
            .select()
            .context("failed to select software renderer after a previous GPU fault")?;
        let selection = RendererSelection {
            requested_backend,
            selected_backend: RENDERER_SOFTWARE,
            source: "previous-fault-fallback",
            fallback_reason: Some("previous-renderer-fault"),
            metal_device: None,
        };
        remember_renderer_selection(selection.clone());
        return Ok(selection);
    }

    match slint::BackendSelector::new()
        .backend_name(requested_backend.into())
        .select()
    {
        Ok(()) => {
            let selection = RendererSelection {
                requested_backend,
                selected_backend: requested_backend,
                source: "configuration",
                fallback_reason: None,
                metal_device: selected_metal_device(requested_backend),
            };
            remember_renderer_selection(selection.clone());
            Ok(selection)
        }
        Err(error) if requested_backend == RENDERER_SKIA => {
            warn!(
                target: "ax_ssh::diagnostics",
                event = "renderer-fallback",
                requested_renderer = requested_backend,
                selected_renderer = RENDERER_SOFTWARE,
                reason = "renderer-selection-error",
                error = %bounded_renderer_error(&error),
                "GPU renderer selection failed; retrying with the software renderer"
            );
            slint::BackendSelector::new()
                .backend_name(RENDERER_SOFTWARE.into())
                .select()
                .context("failed to select software renderer after GPU renderer failure")?;
            let selection = RendererSelection {
                requested_backend,
                selected_backend: RENDERER_SOFTWARE,
                source: "selection-fallback",
                fallback_reason: Some("renderer-selection-error"),
                metal_device: None,
            };
            remember_renderer_selection(selection.clone());
            Ok(selection)
        }
        Err(error) => Err(error).context("failed to select configured Slint renderer"),
    }
}

fn remember_renderer_selection(selection: RendererSelection) {
    let _ = RENDERER_SELECTION.set(selection);
}

fn selected_metal_device(selected_backend: &str) -> Option<String> {
    (selected_backend == RENDERER_SKIA)
        .then(metal_device_name)
        .flatten()
}

fn environment_renderer_backend() -> &'static str {
    match std::env::var("SLINT_BACKEND").ok().as_deref() {
        Some("winit-skia") | Some("skia") => RENDERER_SKIA,
        Some("winit-software") | Some("software") => RENDERER_SOFTWARE,
        _ => RENDERER_ENVIRONMENT,
    }
}

#[cfg(target_os = "macos")]
fn metal_device_name() -> Option<String> {
    use objc2_metal::MTLDevice;

    objc2_metal::MTLCreateSystemDefaultDevice().map(|device| device.name().to_string())
}

#[cfg(not(target_os = "macos"))]
fn metal_device_name() -> Option<String> {
    None
}

/// Returns renderer state suitable for the user-copyable diagnostics report.
pub(super) fn renderer_diagnostics() -> RendererDiagnostics {
    let selection = RENDERER_SELECTION.get();
    RendererDiagnostics {
        requested_backend: selection
            .map(|selection| selection.requested_backend.to_owned())
            .unwrap_or_else(|| "uninitialized".to_owned()),
        selected_backend: selection
            .map(|selection| selection.selected_backend.to_owned())
            .unwrap_or_else(|| "uninitialized".to_owned()),
        source: selection
            .map(|selection| selection.source.to_owned())
            .unwrap_or_else(|| "uninitialized".to_owned()),
        fallback_reason: selection
            .and_then(|selection| selection.fallback_reason)
            .unwrap_or("none")
            .to_owned(),
        metal_device: selection
            .and_then(|selection| selection.metal_device.clone())
            .unwrap_or_else(|| "unavailable".to_owned()),
        window_count: RENDERER_WINDOW_COUNT.load(Ordering::Acquire),
        fault_count: RENDERER_FAULT_COUNT.load(Ordering::Acquire),
        shader_fault_count: RENDERER_SHADER_FAULT_COUNT.load(Ordering::Acquire),
    }
}

pub(super) fn renderer_window_created(kind: &'static str) {
    let window_count = RENDERER_WINDOW_COUNT.fetch_add(1, Ordering::AcqRel) + 1;
    tracing::info!(
        target: "ax_ssh::diagnostics",
        event = "renderer-window",
        action = "created",
        kind,
        window_count,
        "renderer window created"
    );
}

pub(super) fn renderer_window_destroyed(kind: &'static str) {
    let previous = RENDERER_WINDOW_COUNT
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
            count.checked_sub(1)
        })
        .unwrap_or(0);
    let window_count = previous.saturating_sub(1);
    tracing::info!(
        target: "ax_ssh::diagnostics",
        event = "renderer-window",
        action = "destroyed",
        kind,
        window_count,
        "renderer window released"
    );
}

pub(super) fn log_renderer_fault(stage: &'static str, error: &dyn Display) -> bool {
    let message = bounded_renderer_error(error);
    let kind = renderer_fault_kind(&message);
    if kind == "other" {
        return false;
    }
    let fault_count = RENDERER_FAULT_COUNT.fetch_add(1, Ordering::AcqRel) + 1;
    let shader_fault_count = if kind == "metal-shader" {
        RENDERER_SHADER_FAULT_COUNT.fetch_add(1, Ordering::AcqRel) + 1
    } else {
        RENDERER_SHADER_FAULT_COUNT.load(Ordering::Acquire)
    };
    tracing::error!(
        target: "ax_ssh::diagnostics",
        event = "renderer-fault",
        stage,
        kind,
        message = %message,
        window_count = RENDERER_WINDOW_COUNT.load(Ordering::Acquire),
        fault_count,
        shader_fault_count,
        "renderer fault observed"
    );
    true
}

fn bounded_renderer_error(error: &dyn Display) -> String {
    error
        .to_string()
        .chars()
        .filter(|character| !character.is_control())
        .take(512)
        .collect()
}

fn renderer_fault_kind(error: &str) -> &'static str {
    let lower = error.to_ascii_lowercase();
    if lower.contains("shader")
        || lower.contains("org.skia.ganesh")
        || (lower.contains("metal") && lower.contains("compil"))
    {
        "metal-shader"
    } else if lower.contains("skia") || lower.contains("metal") || lower.contains("renderer") {
        "gpu-renderer"
    } else {
        "other"
    }
}

pub(super) fn renderer_fallback_marker_exists(log_directory: &Path) -> bool {
    log_directory.join(RENDERER_FALLBACK_MARKER).is_file()
}

pub(super) fn persist_renderer_fallback(
    log_directory: &Path,
    preference: RendererPreference,
    error: &dyn Display,
) {
    if preference != RendererPreference::Automatic || std::env::var_os("SLINT_BACKEND").is_some() {
        return;
    }
    let marker = log_directory.join(RENDERER_FALLBACK_MARKER);
    let contents = format!(
        "kind={}\n",
        renderer_fault_kind(&bounded_renderer_error(error))
    );
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;

        options.mode(0o600);
    }
    match options.open(&marker) {
        Ok(mut file) => {
            if let Err(write_error) = file.write_all(contents.as_bytes()) {
                let _ = std::fs::remove_file(&marker);
                warn!(
                    target: "ax_ssh::diagnostics",
                    path = %marker.display(),
                    error = %write_error,
                    "failed to write renderer fallback marker"
                );
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => {
            warn!(
                target: "ax_ssh::diagnostics",
                path = %marker.display(),
                error = %error,
                "failed to create renderer fallback marker"
            );
        }
    }
}

pub(super) fn clear_renderer_fallback(log_directory: &Path) {
    let marker = log_directory.join(RENDERER_FALLBACK_MARKER);
    if let Err(error) = std::fs::remove_file(&marker)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        debug!(
            target: "ax_ssh::diagnostics",
            path = %marker.display(),
            %error,
            "failed to clear renderer fallback marker"
        );
    }
}

pub(super) fn configure_software_presentation(mode: SoftwarePresentationMode) {
    #[cfg(target_os = "macos")]
    softbuffer::set_macos_ca_backing_store_enabled(software_presentation_uses_backing_store(mode));

    #[cfg(not(target_os = "macos"))]
    let _ = mode;
}

#[cfg(any(target_os = "macos", test))]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_metal_shader_timeouts_without_storing_shader_source() {
        assert_eq!(
            renderer_fault_kind("Error Domain=org.skia.ganesh Code=1 Compilation took longer"),
            "metal-shader"
        );
        assert_eq!(
            renderer_fault_kind("Metal pipeline compilation failed"),
            "metal-shader"
        );
        assert_eq!(renderer_fault_kind("connection closed"), "other");
    }

    #[test]
    fn renderer_fault_message_is_bounded() {
        let error = "shader".repeat(200);
        let bounded = bounded_renderer_error(&error);

        assert_eq!(bounded.chars().count(), 512);
        assert!(bounded.starts_with("shader"));
    }

    #[test]
    fn selection_uses_actual_software_backend_not_saved_preference() {
        let selection = RendererSelection {
            requested_backend: RENDERER_SKIA,
            selected_backend: RENDERER_SOFTWARE,
            source: "selection-fallback",
            fallback_reason: Some("renderer-selection-error"),
            metal_device: None,
        };

        assert!(selection.uses_software());
    }
}

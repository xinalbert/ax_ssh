//! Process-wide tracing initialization and file-writer lifetime.

use std::any::Any;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::panic::{self, PanicHookInfo};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use chrono::{SecondsFormat, Utc};
use directories::ProjectDirs;
use tracing::Level;
use tracing_appender::non_blocking::{NonBlocking, NonBlockingBuilder, WorkerGuard};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::writer::MakeWriterExt as _;

const LOG_FILE_LIMIT: usize = 15;
const LOG_BUFFERED_LINES: usize = 1_024;
const CRASH_LOG_FILE_NAME: &str = "ax_ssh-crash.log";
const CRASH_FIELD_LIMIT: usize = 4_096;

/// Keeps the non-blocking logging worker alive until process shutdown.
#[derive(Debug)]
pub struct LoggingGuard {
    directory: PathBuf,
    crash_log_path: PathBuf,
    _crash_file: Arc<Mutex<File>>,
    _file_guard: WorkerGuard,
}

impl LoggingGuard {
    /// Installs the process-wide tracing subscriber and opens the rolling log.
    pub fn init() -> Result<Self> {
        let directory = default_log_directory()?;
        prepare_log_directory(&directory)?;

        let (crash_log_path, crash_file) = build_crash_writer(&directory)?;
        let (file_writer, file_guard) = build_file_writer(&directory)?;
        let console_writer = std::io::stderr.with_max_level(Level::INFO);
        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("ax_ssh=info,russh=warn"));

        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(true)
            .with_thread_ids(true)
            .with_ansi(false)
            .with_writer(file_writer.and(console_writer))
            .try_init()
            .map_err(|error| {
                anyhow::anyhow!("failed to install the global tracing subscriber: {error}")
            })?;
        install_panic_hook(Arc::clone(&crash_file));

        Ok(Self {
            directory,
            crash_log_path,
            _crash_file: crash_file,
            _file_guard: file_guard,
        })
    }

    /// Directory containing the retained daily log files.
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// Dedicated synchronous panic report file, separate from rolling tracing logs.
    pub fn crash_log_path(&self) -> &Path {
        &self.crash_log_path
    }
}

impl Drop for LoggingGuard {
    fn drop(&mut self) {
        tracing::info!("AxSSH logging shutdown; flushing buffered events");
    }
}

fn default_log_directory() -> Result<PathBuf> {
    let dirs = ProjectDirs::from("com", "axsoft", "ax_ssh")
        .context("cannot determine the platform data directory for logs")?;
    Ok(dirs.data_local_dir().join("logs"))
}

fn prepare_log_directory(path: &Path) -> Result<()> {
    fs::create_dir_all(path)
        .with_context(|| format!("failed to create log directory {}", path.display()))?;
    set_private_directory_permissions(path)
}

fn build_file_writer(directory: &Path) -> Result<(NonBlocking, WorkerGuard)> {
    let file_appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("ax_ssh")
        .filename_suffix("log")
        .max_log_files(LOG_FILE_LIMIT)
        .build(directory)
        .with_context(|| {
            format!(
                "failed to initialize rolling logs in {}",
                directory.display()
            )
        })?;
    Ok(NonBlockingBuilder::default()
        .buffered_lines_limit(LOG_BUFFERED_LINES)
        .lossy(false)
        .thread_name("ax-ssh-log-writer")
        .finish(file_appender))
}

fn build_crash_writer(directory: &Path) -> Result<(PathBuf, Arc<Mutex<File>>)> {
    let path = directory.join(CRASH_LOG_FILE_NAME);
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("failed to initialize crash log {}", path.display()))?;
    set_private_file_permissions(&path)?;
    Ok((path, Arc::new(Mutex::new(file))))
}

fn install_panic_hook(crash_file: Arc<Mutex<File>>) {
    let previous_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        let report = format_crash_report(panic_info);
        let write_result = match crash_file.lock() {
            Ok(mut file) => write_crash_report(&mut file, &report),
            Err(poisoned) => write_crash_report(&mut poisoned.into_inner(), &report),
        };
        if let Err(error) = write_result {
            eprintln!("AxSSH crash report could not be written: {error}");
        }

        // Keep Rust's normal stderr/backtrace behavior in addition to the durable report.
        previous_hook(panic_info);
    }));
}

fn write_crash_report(file: &mut File, report: &str) -> std::io::Result<()> {
    file.write_all(report.as_bytes())?;
    file.flush()?;
    file.sync_data()
}

fn format_crash_report(panic_info: &PanicHookInfo<'_>) -> String {
    let thread = std::thread::current();
    let thread_name = thread.name().unwrap_or("<unnamed>");
    let location = panic_info
        .location()
        .map(|location| {
            format!(
                "{}:{}:{}",
                location.file(),
                location.line(),
                location.column()
            )
        })
        .unwrap_or_else(|| "<unknown>".to_owned());
    let renderer = std::env::var("SLINT_BACKEND")
        .ok()
        .filter(|value| matches!(value.as_str(), "winit-skia" | "winit-software"))
        .unwrap_or_else(|| "automatic".to_owned());
    format!(
        "=== AxSSH crash report ===\n\
timestamp_utc={}\n\
version={}\n\
pid={}\n\
os={}\n\
arch={}\n\
thread={}\n\
thread_id={:?}\n\
renderer={}\n\
panic_location={}\n\
panic_message={}\n\
backtrace:\n{}\
=== end AxSSH crash report ===\n\n",
        Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        env!("CARGO_PKG_VERSION"),
        std::process::id(),
        std::env::consts::OS,
        std::env::consts::ARCH,
        thread_name,
        thread.id(),
        renderer,
        location,
        panic_payload(panic_info.payload()),
        std::backtrace::Backtrace::force_capture(),
    )
}

fn panic_payload(payload: &(dyn Any + Send)) -> String {
    let message = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&'static str>().copied())
        .unwrap_or("<non-string panic payload>");
    let mut bounded = String::with_capacity(message.len().min(CRASH_FIELD_LIMIT));
    for character in message.chars().take(CRASH_FIELD_LIMIT) {
        match character {
            '\n' => bounded.push_str("\\n"),
            '\r' => bounded.push_str("\\r"),
            '\t' => bounded.push_str("\\t"),
            character if character.is_control() => bounded.push('?'),
            character => bounded.push(character),
        }
    }
    if message.chars().nth(CRASH_FIELD_LIMIT).is_some() {
        bounded.push_str("...");
    }
    bounded
}

#[cfg(unix)]
fn set_private_directory_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt as _;

    let mut permissions = fs::metadata(path)
        .with_context(|| format!("failed to inspect log directory {}", path.display()))?
        .permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions)
        .with_context(|| format!("failed to secure log directory {}", path.display()))
}

#[cfg(unix)]
fn set_private_file_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt as _;

    let mut permissions = fs::metadata(path)
        .with_context(|| format!("failed to inspect crash log {}", path.display()))?
        .permissions();
    permissions.set_mode(0o600);
    fs::set_permissions(path, permissions)
        .with_context(|| format!("failed to secure crash log {}", path.display()))
}

#[cfg(not(unix))]
fn set_private_file_permissions(_: &Path) -> Result<()> {
    Ok(())
}

#[cfg(not(unix))]
fn set_private_directory_permissions(_: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepares_private_log_directory() {
        let path = std::env::temp_dir().join(format!("ax-ssh-logs-{}", uuid::Uuid::new_v4()));
        prepare_log_directory(&path).expect("log directory should be created");
        assert!(path.is_dir());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;

            let mode = fs::metadata(&path)
                .expect("log directory metadata should be readable")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o700);
        }
        fs::remove_dir(path).expect("temporary log directory should be removable");
    }

    #[test]
    fn worker_guard_flushes_rolling_log_on_drop() {
        let path = std::env::temp_dir().join(format!("ax-ssh-log-flush-{}", uuid::Uuid::new_v4()));
        prepare_log_directory(&path).expect("log directory should be created");
        let (writer, guard) = build_file_writer(&path).expect("log writer should be created");
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_writer(writer)
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!("flush-lifecycle-test");
        });
        drop(guard);

        let log_files = fs::read_dir(&path)
            .expect("log directory should be readable")
            .map(|entry| entry.expect("log entry should be readable").path())
            .collect::<Vec<_>>();
        assert_eq!(log_files.len(), 1);
        let contents = fs::read_to_string(&log_files[0]).expect("log file should be readable");
        assert!(contents.contains("flush-lifecycle-test"));
        fs::remove_dir_all(path).expect("temporary log directory should be removable");
    }

    #[test]
    fn crash_writer_creates_a_private_durable_file() {
        let path = std::env::temp_dir().join(format!("ax-ssh-crash-{}", uuid::Uuid::new_v4()));
        prepare_log_directory(&path).expect("temporary log directory should be created");
        let (crash_path, crash_file) =
            build_crash_writer(&path).expect("crash writer should be created");
        let report = "panic_message=end byte index 1 is out of bounds\n";
        match crash_file.lock() {
            Ok(mut file) => {
                write_crash_report(&mut file, report).expect("crash report should be durable")
            }
            Err(poisoned) => write_crash_report(&mut poisoned.into_inner(), report)
                .expect("crash report should be durable"),
        }
        assert_eq!(crash_path, path.join(CRASH_LOG_FILE_NAME));
        assert_eq!(
            fs::read_to_string(&crash_path).expect("crash report should be readable"),
            report
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;

            let mode = fs::metadata(&crash_path)
                .expect("crash report metadata should be readable")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }
        fs::remove_dir_all(path).expect("temporary crash directory should be removable");
    }

    #[test]
    fn panic_payload_keeps_string_messages() {
        let owned = String::from("panic details");
        let owned_payload: &(dyn Any + Send) = &owned;
        assert_eq!(panic_payload(owned_payload), "panic details");

        let static_payload: &(dyn Any + Send) = &"static panic details";
        assert_eq!(panic_payload(static_payload), "static panic details");
    }
}

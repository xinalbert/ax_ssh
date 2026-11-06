#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;

fn main() -> anyhow::Result<()> {
    app::configure_window_lifecycle();
    let logging = ax_ssh::logging::LoggingGuard::init()?;
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        revision = env!("AXSSH_BUILD_REVISION"),
        process_id = std::process::id(),
        debug_build = cfg!(debug_assertions),
        executable = ?std::env::current_exe(),
        log_directory = %logging.directory().display(),
        crash_log = %logging.crash_log_path().display(),
        "AxSSH process started"
    );

    let result = app::run(logging.directory().to_path_buf());
    match &result {
        Ok(()) => tracing::info!("AxSSH process exiting normally"),
        Err(error) => tracing::error!(%error, "AxSSH process exiting with an error"),
    }
    drop(logging);
    result
}

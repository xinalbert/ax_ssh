#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;

fn main() -> anyhow::Result<()> {
    let logging = ax_ssh::logging::LoggingGuard::init()?;
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        log_directory = %logging.directory().display(),
        "AxSSH process started"
    );

    let result = app::run();
    match &result {
        Ok(()) => tracing::info!("AxSSH process exiting normally"),
        Err(error) => tracing::error!(%error, "AxSSH process exiting with an error"),
    }
    drop(logging);
    result
}

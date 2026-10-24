#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod egui_app;

fn main() -> anyhow::Result<()> {
    let logging = ax_ssh::logging::LoggingGuard::init_experimental()?;
    let result = egui_app::run();
    if let Err(error) = &result {
        tracing::error!(%error, "egui application failed");
    }
    drop(logging);
    result
}

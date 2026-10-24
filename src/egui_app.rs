//! Experimental egui frontend; transport and terminal state stay in the controller.

use std::sync::Arc;

use anyhow::{Context, Result};
use ax_ssh::controller::{WorkerShutdowns, Workspace};
use eframe::egui;

mod connection;
mod file_icons;
mod fonts;
mod menus;
mod native_drop;
mod settings;
mod sftp_drop;
mod sftp_edit;
mod terminal;
mod theme;
mod workspace;

struct App {
    client: ax_ssh::controller::WorkspaceClient,
    workspace: workspace::WorkspaceUi,
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    menu: Option<menus::native::NativeMenu>,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        if let Some(menu) = &mut self.menu {
            self.workspace.native_menu = true;
            if let Ok(view) = self.client.view() {
                let blocked = self.workspace.blocked(&view);
                let terminal = view
                    .tabs
                    .iter()
                    .any(|tab| Some(tab.id) == view.active_tab_id && !tab.settings);
                if let Err(error) = menu.sync(&view.settings, |cmd| {
                    cmd.enabled(view.ready, blocked, terminal, view.tabs.len())
                }) {
                    tracing::warn!(%error, "cannot update native menu");
                }
                if self.workspace.menu_command.is_none() {
                    self.workspace.menu_command = menu.take();
                }
            }
        }
        self.workspace.native_drop_position = native_drop::position(frame, ui.ctx());
        if let Err(error) = self.workspace.show(ui, &self.client) {
            ui.label(format!("{error:#}"));
        }
    }
    fn on_exit(&mut self) {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            self.menu.take();
        }
    }
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|input| input.viewport().minimized == Some(true)) {
            self.workspace.clear_caches();
        }
    }
}

pub(crate) fn run() -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .max_blocking_threads(4)
        .enable_all()
        .build()
        .context("cannot start experimental terminal runtime")?;
    // Startup font I/O completes on a blocking worker before any native window exists.
    let fonts = runtime
        .block_on(async { tokio::task::spawn_blocking(fonts::load).await })
        .context("terminal font loader failed")??;
    let owner = Arc::new(std::sync::Mutex::new(None));
    let owner_for_app = owner.clone();
    let handle = runtime.handle().clone();
    let icon = image::load_from_memory(include_bytes!(
        "../assets/ion/terminal_icon_all_formats/terminal_icon_256.png"
    ))
    .context("cannot decode application icon")?
    .into_rgba8();
    let viewport = egui::ViewportBuilder::default()
        .with_title("AxSSH · egui terminal")
        .with_app_id("com.axsoft.ax_ssh_egui")
        .with_inner_size([1000.0, 700.0])
        .with_min_inner_size([400.0, 240.0])
        .with_icon(egui::IconData {
            width: icon.width(),
            height: icon.height(),
            rgba: icon.into_raw(),
        });
    let result = eframe::run_native(
        "AxSSH egui",
        eframe::NativeOptions {
            viewport,
            renderer: eframe::Renderer::Wgpu,
            persist_window: false,
            ..Default::default()
        },
        Box::new(move |cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            cc.egui_ctx.set_fonts(fonts);
            let ctx = cc.egui_ctx.clone();
            let terminal = Workspace::spawn(
                &handle,
                Workspace::experimental_store()?,
                Arc::new(move || ctx.request_repaint()),
            );
            let client = terminal.client();
            *owner_for_app
                .lock()
                .map_err(|_| "terminal owner lock poisoned")? = Some(terminal);
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            let menu = Some(menus::native::NativeMenu::new(cc)?);
            Ok(Box::new(App {
                client,
                #[cfg(any(target_os = "macos", target_os = "windows"))]
                menu,
                workspace: workspace::WorkspaceUi::default(),
            }))
        }),
    )
    .map_err(|error| anyhow::anyhow!("cannot run egui window: {error}"));
    let terminal = owner
        .lock()
        .map_err(|_| anyhow::anyhow!("terminal owner lock poisoned"))?
        .take();
    let mut shutdowns = WorkerShutdowns::default();
    if let Some(terminal) = terminal {
        drop(shutdowns.start(runtime.handle(), terminal.shutdown()));
    }
    let cleanup = if runtime.block_on(shutdowns.finish(std::time::Duration::from_secs(25))) {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "terminal cleanup exceeded the application deadline"
        ))
    };
    runtime.shutdown_timeout(std::time::Duration::from_secs(2));
    result.and(cleanup)
}

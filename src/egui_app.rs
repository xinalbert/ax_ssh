//! Experimental egui frontend; transport and terminal state stay in the controller.

use std::sync::Arc;

use anyhow::{Context, Result};
use ax_ssh::controller::{WorkerShutdowns, Workspace};
use eframe::egui;

mod clipboard;
mod connection;
mod file_icons;
mod fonts;
mod i18n;
mod image_paste;
mod menus;
mod native_drop;
#[cfg(target_os = "macos")]
mod native_input;
mod remote_clipboard;
mod settings;
mod sftp_drop;
mod sftp_edit;
mod system_icons;
mod terminal;
mod theme;
mod workspace;

struct App {
    client: ax_ssh::controller::WorkspaceClient,
    workspace: workspace::WorkspaceUi,
    system_locale: Option<String>,
    fonts: fonts::FontClient,
    clipboard: remote_clipboard::Client,
    images: image_paste::Client,
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    menu: Option<menus::native::NativeMenu>,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        if let Ok(view) = self.client.view() {
            i18n::Translator::new(view.settings.ui_language, self.system_locale.as_deref())
                .apply(ui.ctx());
        }
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        if let Some(menu) = &mut self.menu {
            self.workspace.native_menu = true;
            if let Ok(view) = self.client.view() {
                let blocked = self.workspace.blocked(&view);
                if let Err(error) = menu.sync(&view.settings, i18n::current(ui.ctx()), |cmd| {
                    cmd.enabled_for_view(&view, blocked)
                }) {
                    tracing::warn!(%error, "cannot update native menu");
                }
                if self.workspace.menu_command.is_none() {
                    self.workspace.menu_command = menu.take();
                }
            }
        }
        clipboard::guard(ui.ctx(), |owner| {
            self.client
                .view()
                .is_ok_and(|view| self.workspace.permits_paste(&view, owner))
        });
        if let Err(error) = self.clipboard.guard(ui.ctx(), |owner| {
            self.client.view().is_ok_and(|view| {
                view.settings.terminal.osc52_clipboard && self.workspace.permits_paste(&view, owner)
            })
        }) {
            ui.colored_label(egui::Color32::LIGHT_RED, format!("{error:#}"));
        }
        if let Err(error) = self.images.guard(ui.ctx(), |owner| {
            self.client
                .view()
                .is_ok_and(|view| self.workspace.permits_paste(&view, owner))
        }) {
            ui.colored_label(egui::Color32::LIGHT_RED, format!("{error:#}"));
        }
        self.workspace.native_drop_position = native_drop::position(frame, ui.ctx());
        if let Err(error) = self.workspace.show(ui, &self.client) {
            ui.label(format!("{error:#}"));
        }
        if let Err(error) = image_paste::show(ui.ctx()) {
            ui.colored_label(egui::Color32::LIGHT_RED, format!("{error:#}"));
        }
        #[cfg(target_os = "macos")]
        native_input::update(
            ui.ctx(),
            frame,
            self.client.view().is_ok_and(|view| {
                view.settings.terminal.option_as_meta
                    && self.workspace.terminal_keyboard_active(ui.ctx(), &view)
            }),
        );
        if !ui.is_sizing_pass()
            && let Ok(view) = self.client.view()
            && let Err(error) = self
                .fonts
                .update(ui.ctx(), self.workspace.font_appearance(&view.settings))
        {
            ui.colored_label(egui::Color32::LIGHT_RED, format!("{error:#}"));
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
    ax_ssh::controller::semantic::prepare();
    // Resolve the platform locale once before entering any GUI callback.
    let system_locale = sys_locale::get_locale();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .max_blocking_threads(4)
        .enable_all()
        .build()
        .context("cannot start experimental terminal runtime")?;
    // Startup font I/O completes on a blocking worker before any native window exists.
    let fonts = runtime
        .block_on(async { tokio::task::spawn_blocking(fonts::Loader::discover).await })
        .context("terminal font loader failed")??;
    #[cfg(target_os = "linux")]
    runtime
        .block_on(async {
            tokio::task::spawn_blocking(ax_ssh::system_icons::discover_local_platform).await
        })
        .context("local system icon discovery task failed")?;
    let x11_locations = runtime
        .block_on(async {
            tokio::task::spawn_blocking(ax_ssh::x_server::discovered_provider_locations).await
        })
        .context("local X server discovery task failed")?;
    let local_shells = match runtime
        .block_on(async { tokio::task::spawn_blocking(ax_ssh::local_shell::discover_shells).await })
    {
        Ok(choices) => choices,
        Err(error) => {
            tracing::warn!(%error, "local shell discovery task failed");
            Vec::new()
        }
    };
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
            cc.egui_ctx.set_fonts(fonts.bootstrap());
            settings::set_x11_locations(&cc.egui_ctx, x11_locations);
            settings::set_local_shells(&cc.egui_ctx, local_shells);
            let mut slot = owner_for_app
                .lock()
                .map_err(|_| "terminal owner lock poisoned")?;
            let ctx = cc.egui_ctx.clone();
            let terminal = Workspace::spawn(
                &handle,
                Workspace::experimental_store()?,
                Arc::new(move || ctx.request_repaint()),
            );
            let client = terminal.client();
            let ctx = cc.egui_ctx.clone();
            let (font_owner, fonts) =
                fonts::FontService::spawn(&handle, fonts, Arc::new(move || ctx.request_repaint()));
            let (clipboard_owner, clipboard) =
                remote_clipboard::Service::spawn(&handle, &cc.egui_ctx);
            clipboard.install(&cc.egui_ctx);
            let (image_owner, images) = image_paste::Service::spawn(&handle, &cc.egui_ctx);
            images.install(&cc.egui_ctx);
            *slot = Some((terminal, font_owner, clipboard_owner, image_owner));
            drop(slot);
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            let menu = Some(menus::native::NativeMenu::new(cc)?);
            Ok(Box::new(App {
                client,
                system_locale,
                fonts,
                clipboard,
                images,
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
    if let Some((terminal, fonts, clipboard, images)) = terminal {
        drop(shutdowns.start(runtime.handle(), terminal.shutdown()));
        drop(shutdowns.start(runtime.handle(), fonts.shutdown()));
        drop(shutdowns.start(runtime.handle(), clipboard.shutdown()));
        drop(shutdowns.start(runtime.handle(), images.shutdown()));
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

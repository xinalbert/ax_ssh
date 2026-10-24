//! AppKit/Win32 adapter; no GTK loop is introduced into the winit application.
use super::{Command, GROUPS};
use anyhow::Result;
use ax_ssh::config::AppSettings;
use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use std::sync::mpsc::{Receiver, sync_channel};

pub(crate) struct NativeMenu {
    menu: Menu,
    items: Vec<(Command, MenuItem)>,
    events: Receiver<Command>,
    shortcuts: Option<ax_ssh::config::ShortcutSettings>,
    #[cfg(target_os = "windows")]
    hwnd: isize,
}

impl NativeMenu {
    pub(crate) fn new(cc: &eframe::CreationContext<'_>) -> Result<Self> {
        let menu = Menu::new();
        let mut items = Vec::new();
        #[cfg(target_os = "macos")]
        {
            let app = Submenu::new("AxSSH", true);
            append(&app, &mut items, Command::About, "About AxSSH")?;
            app.append(&PredefinedMenuItem::separator())?;
            append(&app, &mut items, Command::Settings, "Settings…")?;
            app.append(&PredefinedMenuItem::separator())?;
            app.append(&PredefinedMenuItem::hide(None))?;
            app.append(&PredefinedMenuItem::hide_others(None))?;
            app.append(&PredefinedMenuItem::show_all(None))?;
            app.append(&PredefinedMenuItem::separator())?;
            append(&app, &mut items, Command::Quit, "Quit AxSSH")?;
            menu.append(&app)?;
        }
        for (title, entries) in GROUPS {
            let submenu = Submenu::new(*title, true);
            for (cmd, label) in *entries {
                if cfg!(target_os = "macos") && matches!(cmd, Command::Settings | Command::About) {
                    continue;
                }
                append(&submenu, &mut items, *cmd, label)?;
            }
            if *title == "File" && cfg!(target_os = "windows") {
                submenu.append(&PredefinedMenuItem::separator())?;
                append(&submenu, &mut items, Command::Quit, "Exit")?;
            }
            if *title == "Window" {
                submenu.append(&PredefinedMenuItem::minimize(None))?;
            }
            if !submenu.items().is_empty() {
                menu.append(&submenu)?;
            }
        }
        #[cfg(target_os = "macos")]
        {
            menu.init_for_nsapp();
            for item in menu.items() {
                if let muda::MenuItemKind::Submenu(submenu) = item {
                    if submenu.text() == "Window" {
                        submenu.set_as_windows_menu_for_nsapp();
                    }
                    if submenu.text() == "Help" {
                        submenu.set_as_help_menu_for_nsapp();
                    }
                }
            }
        }
        #[cfg(target_os = "windows")]
        let hwnd = {
            use raw_window_handle::{HasWindowHandle as _, RawWindowHandle};
            let RawWindowHandle::Win32(handle) = cc.window_handle()?.as_raw() else {
                anyhow::bail!("native menu needs a Win32 window");
            };
            let hwnd = handle.hwnd.get();
            // SAFETY: eframe owns this live HWND on the calling UI thread.
            unsafe {
                menu.init_for_hwnd(hwnd)?;
            }
            hwnd
        };
        let (send, events) = sync_channel(32);
        let ids: Vec<_> = items
            .iter()
            .map(|(cmd, item)| (item.id().clone(), *cmd))
            .collect();
        let ctx = cc.egui_ctx.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            if let Some((_, cmd)) = ids.iter().find(|(id, _)| *id == event.id) {
                if send.try_send(*cmd).is_err() {
                    tracing::warn!("native menu queue full or closed");
                }
                ctx.request_repaint();
            }
        }));
        Ok(Self {
            menu,
            items,
            events,
            shortcuts: None,
            #[cfg(target_os = "windows")]
            hwnd,
        })
    }
    pub(crate) fn sync(
        &mut self,
        settings: &AppSettings,
        enabled: impl Fn(Command) -> bool,
    ) -> Result<()> {
        let changed = self.shortcuts.as_ref() != Some(&settings.shortcuts);
        for (cmd, item) in &self.items {
            let active = enabled(*cmd);
            if item.is_enabled() != active {
                item.set_enabled(active);
            }
            if changed {
                let shortcut = cmd.shortcut(settings);
                #[cfg(target_os = "macos")]
                item.set_key_accelerator(
                    shortcut
                        .map(|text| {
                            text.replace("Cmd", "Super")
                                .replace("Meta", "Super")
                                .parse()
                        })
                        .transpose()?,
                )?;
                #[cfg(target_os = "windows")]
                {
                    let label = GROUPS
                        .iter()
                        .flat_map(|(_, items)| items.iter())
                        .find(|(action, _)| action == cmd)
                        .map(|(_, label)| *label)
                        .unwrap_or("Exit");
                    item.set_text(format!(
                        "{}{}",
                        label,
                        shortcut.map(|key| format!("\t{key}")).unwrap_or_default()
                    ));
                }
            }
        }
        self.shortcuts = Some(settings.shortcuts.clone());
        Ok(())
    }
    pub(crate) fn take(&self) -> Option<Command> {
        self.events.try_recv().ok()
    }
}

fn append(
    menu: &Submenu,
    items: &mut Vec<(Command, MenuItem)>,
    cmd: Command,
    title: &str,
) -> Result<()> {
    let item = MenuItem::new(title, false, None);
    menu.append(&item)?;
    items.push((cmd, item));
    Ok(())
}
impl Drop for NativeMenu {
    fn drop(&mut self) {
        MenuEvent::set_event_handler(None::<fn(MenuEvent)>);
        #[cfg(target_os = "macos")]
        self.menu.remove_for_nsapp();
        #[cfg(target_os = "windows")]
        {
            // SAFETY: invoked in App::on_exit before eframe destroys its HWND.
            if let Err(error) = unsafe { self.menu.remove_for_hwnd(self.hwnd) } {
                tracing::warn!(%error, "cannot detach native menu");
            }
        }
    }
}

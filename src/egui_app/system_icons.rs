//! Bounded textures from build-time pixels; no SVG parsing or filesystem work.

use std::collections::HashMap;

use ax_ssh::config::{AppearanceSettings, SessionProfile, system_icon_choices, system_icon_id};
use ax_ssh::controller::WorkspaceTabView;
use eframe::egui;

const SIDE: usize = 48;
const ASSETS: &[(&str, &[u8])] = include!(concat!(env!("OUT_DIR"), "/system_icon_pixels.rs"));

#[derive(Default)]
pub(super) struct Icons {
    // IDs only come from ASSETS (at most 128), with two color variants each.
    textures: HashMap<(usize, bool), egui::TextureHandle>,
}

impl Icons {
    pub(super) fn clear(&mut self) {
        self.textures.clear();
    }

    pub(super) fn image(
        &mut self,
        ui: &egui::Ui,
        id: &str,
        follow_theme: bool,
    ) -> Option<egui::Image<'static>> {
        let ctx = ui.ctx();
        let canonical = system_icon_id(id)?;
        let index = ASSETS
            .binary_search_by_key(&canonical, |(id, _)| *id)
            .ok()?;
        let texture = self
            .textures
            .entry((index, follow_theme))
            .or_insert_with(|| {
                let mut image =
                    egui::ColorImage::from_rgba_premultiplied([SIDE, SIDE], ASSETS[index].1);
                if follow_theme {
                    // Slint colorize replaces RGB and keeps alpha. Image tint alone
                    // multiplies source RGB, so use an opaque-white alpha silhouette.
                    for pixel in &mut image.pixels {
                        let alpha = pixel.a();
                        *pixel = egui::Color32::from_rgba_premultiplied(alpha, alpha, alpha, alpha);
                    }
                }
                ctx.load_texture(
                    format!("system-icon-{canonical}-{follow_theme}"),
                    image,
                    egui::TextureOptions::LINEAR,
                )
            });
        let name = system_icon_choices()
            .iter()
            .find(|choice| choice.id == canonical)?
            .name;
        Some(
            egui::Image::new((texture.id(), texture.size_vec2()))
                .fit_to_exact_size(egui::vec2(16.0, 16.0))
                .alt_text(name)
                .tint(if follow_theme {
                    ui.visuals().text_color()
                } else {
                    egui::Color32::WHITE
                }),
        )
    }
}

pub(super) fn local_icon(appearance: &AppearanceSettings) -> &'static str {
    appearance
        .local_shell_icon_override
        .as_deref()
        .and_then(system_icon_id)
        .unwrap_or_else(ax_ssh::system_icons::local_platform)
}

pub(super) fn profile_icon(profile: &SessionProfile) -> &'static str {
    profile
        .system_icon_override
        .as_deref()
        .and_then(system_icon_id)
        .or_else(|| {
            profile.cached_system().and_then(|system| {
                system.icon_id().and_then(system_icon_id).or_else(|| {
                    system_icon_id(ax_ssh::system_icons::platform_for_release(system.release()))
                })
            })
        })
        .unwrap_or("")
}

pub(super) fn tab_icon(tab: &WorkspaceTabView, appearance: &AppearanceSettings) -> &'static str {
    let focused = tab
        .layout
        .panes
        .iter()
        .find(|pane| pane.focused)
        .map(|pane| pane.tab_id);
    let pane = tab
        .panes
        .iter()
        .find(|pane| Some(pane.id) == focused)
        .or_else(|| tab.panes.first());
    match pane {
        Some(pane) if pane.local => local_icon(appearance),
        Some(pane) => pane.profile.as_ref().map_or("", profile_icon),
        None => "",
    }
}

#[cfg(test)]
mod tests;

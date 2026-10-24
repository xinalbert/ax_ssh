//! Frontend-owned textures for the shared, already decoded icon snapshots.

use std::collections::HashMap;

use ax_ssh::file_icons::{FileIcon, FileIconKey};
use eframe::egui;

const MAX_TEXTURES: usize = 64;

struct CachedIcon {
    pixels: FileIcon,
    texture: egui::TextureHandle,
}

#[derive(Default)]
pub(super) struct FileIconTextures {
    textures: HashMap<FileIconKey, CachedIcon>,
}

impl FileIconTextures {
    pub(super) fn clear(&mut self) {
        self.textures.clear();
    }

    pub(super) fn sync(&mut self, ctx: &egui::Context, icons: &HashMap<FileIconKey, FileIcon>) {
        self.textures.retain(|key, _| icons.contains_key(key));
        for (key, pixels) in icons.iter().take(MAX_TEXTURES) {
            if self
                .textures
                .get(key)
                .is_some_and(|cached| cached.pixels.shares_pixels(pixels))
            {
                continue;
            }
            let image = egui::ColorImage::from_rgba_unmultiplied(
                [pixels.width() as usize, pixels.height() as usize],
                pixels.rgba(),
            );
            if let Some(cached) = self.textures.get_mut(key) {
                cached.texture.set(image, egui::TextureOptions::LINEAR);
                cached.pixels = pixels.clone();
            } else if self.textures.len() < MAX_TEXTURES {
                self.textures.insert(
                    key.clone(),
                    CachedIcon {
                        pixels: pixels.clone(),
                        texture: ctx.load_texture(
                            "sftp-file-type",
                            image,
                            egui::TextureOptions::LINEAR,
                        ),
                    },
                );
            }
        }
    }

    pub(super) fn show(&self, ui: &mut egui::Ui, key: &FileIconKey) {
        if let Some(cached) = self
            .textures
            .get(key)
            .or_else(|| self.textures.get(&FileIconKey::GenericFile))
        {
            ui.add(egui::Image::new(&cached.texture).fit_to_exact_size(egui::vec2(20.0, 20.0)));
        } else {
            // Missing keys keep row dimensions stable; names retain all semantics.
            ui.allocate_space(egui::vec2(20.0, 20.0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ax_ssh::file_icons::global_provider;

    #[test]
    fn repeated_snapshots_reuse_textures_and_release_removed_keys() {
        let ctx = egui::Context::default();
        let mut cache = FileIconTextures::default();
        let icons = HashMap::from([
            (
                FileIconKey::Folder,
                global_provider().cached_icon(&FileIconKey::Folder),
            ),
            (
                FileIconKey::GenericFile,
                global_provider().cached_icon(&FileIconKey::GenericFile),
            ),
        ]);
        cache.sync(&ctx, &icons);
        let id = cache.textures[&FileIconKey::Folder].texture.id();
        cache.sync(&ctx, &icons);
        assert_eq!(cache.textures[&FileIconKey::Folder].texture.id(), id);
        let remaining = HashMap::from([(FileIconKey::Folder, icons[&FileIconKey::Folder].clone())]);
        cache.sync(&ctx, &remaining);
        assert_eq!(cache.textures.len(), 1);
        cache.clear();
        assert!(cache.textures.is_empty());
    }

    #[test]
    fn texture_admission_remains_bounded() {
        let ctx = egui::Context::default();
        let mut cache = FileIconTextures::default();
        let fallback = global_provider().cached_icon(&FileIconKey::GenericFile);
        let icons = (0..200)
            .map(|index| {
                (
                    FileIconKey::Extension(format!("x{index}")),
                    fallback.clone(),
                )
            })
            .collect();
        cache.sync(&ctx, &icons);
        assert_eq!(cache.textures.len(), MAX_TEXTURES);
        cache.sync(&ctx, &HashMap::new());
        assert!(cache.textures.is_empty());
    }
}

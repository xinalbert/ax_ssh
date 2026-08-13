use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use slint::fontique_010::fontique;

const MAX_FONT_FAMILY_CHARS: usize = 128;
const MAX_FONT_OPTIONS: usize = 256;
const MAX_BUNDLED_FONT_FILE_BYTES: u64 = 24 * 1024 * 1024;

const BUNDLED_UI_FONT_FAMILY: &str = "JetBrains Mono";
const EMBEDDED_UI_FONT_FILES: &[&[u8]] = &[
    include_bytes!("../../assets/fonts/JetBrainsMono-Regular.ttf"),
    include_bytes!("../../assets/fonts/JetBrainsMono-Bold.ttf"),
    include_bytes!("../../assets/fonts/JetBrainsMono-Italic.ttf"),
    include_bytes!("../../assets/fonts/JetBrainsMono-BoldItalic.ttf"),
];

struct BundledFont {
    family: &'static str,
    files: &'static [&'static str],
}

const BUNDLED_FONTS: &[BundledFont] = &[
    BundledFont {
        family: "Maple Mono NF CN",
        files: &["MapleMono-NF-CN-Regular.ttf", "MapleMono-NF-CN-Bold.ttf"],
    },
    BundledFont {
        family: "Iosevka Term",
        files: &[
            "IosevkaTerm-Regular.ttf",
            "IosevkaTerm-Bold.ttf",
            "IosevkaTerm-Italic.ttf",
            "IosevkaTerm-BoldItalic.ttf",
        ],
    },
    BundledFont {
        family: BUNDLED_UI_FONT_FAMILY,
        files: &[
            "JetBrainsMono-Regular.ttf",
            "JetBrainsMono-Bold.ttf",
            "JetBrainsMono-Italic.ttf",
            "JetBrainsMono-BoldItalic.ttf",
        ],
    },
    BundledFont {
        family: "Monaspace Neon Var",
        files: &["MonaspaceNeon-Variable.ttf"],
    },
];

#[derive(Clone, Debug)]
pub(super) struct FontResources {
    directories: Vec<PathBuf>,
}

pub(super) struct FontRegistry {
    resources: FontResources,
    registered_families: BTreeSet<&'static str>,
}

pub(super) fn load_terminal_font_on_demand(
    runtime: &tokio::runtime::Handle,
    ui: slint::Weak<super::AppWindow>,
    font_registry: Arc<Mutex<FontRegistry>>,
    started: Arc<AtomicBool>,
) {
    if started.load(Ordering::Acquire) {
        return;
    }
    let family = ui
        .upgrade()
        .map(|ui| ui.get_terminal_font_family().to_string());
    let Some(family) = family else {
        return;
    };
    if bundled_font(&family).is_none() || started.swap(true, Ordering::AcqRel) {
        return;
    }
    let resources = match font_registry.lock() {
        Ok(registry) if registry.is_registered(&family) => return,
        Ok(registry) => registry.resources(),
        Err(_) => {
            started.store(false, Ordering::Release);
            tracing::warn!("cannot access font resources for terminal startup");
            return;
        }
    };
    runtime.spawn(async move {
        let family_for_load = family.clone();
        let loaded =
            tokio::task::spawn_blocking(move || resources.load_bundled_font(&family_for_load))
                .await;
        let font = match loaded {
            Ok(Ok(Some(font))) => font,
            Ok(Ok(None)) => {
                tracing::warn!(family = %family, "configured terminal bundled font is unavailable");
                started.store(false, Ordering::Release);
                return;
            }
            Ok(Err(error)) => {
                tracing::warn!(%error, "failed to read configured terminal font");
                started.store(false, Ordering::Release);
                return;
            }
            Err(error) => {
                tracing::warn!(%error, "configured terminal font task failed");
                started.store(false, Ordering::Release);
                return;
            }
        };
        super::view::dispatch_ui(&ui, move |ui| {
            let registration = font_registry
                .lock()
                .map_err(|_| anyhow::anyhow!("font registry lock poisoned"))
                .and_then(|mut registry| registry.register_loaded_font(font));
            if let Err(error) = registration {
                started.store(false, Ordering::Release);
                tracing::warn!(%error, "failed to register configured terminal font");
                ui.set_status(format!("Cannot register terminal font: {error}").into());
            }
        });
    });
}

#[derive(Debug)]
pub(super) struct LoadedBundledFont {
    family: &'static str,
    files: Vec<Vec<u8>>,
}

impl FontResources {
    fn new() -> Self {
        Self {
            directories: bundled_font_directories(),
        }
    }

    pub(super) fn load_bundled_font(&self, family: &str) -> Result<Option<LoadedBundledFont>> {
        let Some(font) = bundled_font(family) else {
            return Ok(None);
        };
        let files = if font.family == BUNDLED_UI_FONT_FAMILY {
            EMBEDDED_UI_FONT_FILES
                .iter()
                .map(|bytes| bytes.to_vec())
                .collect()
        } else {
            let directory = self.find_font_directory(font).with_context(|| {
                format!("bundled font resources are unavailable for {}", font.family)
            })?;
            font.files
                .iter()
                .map(|file_name| read_bundled_font_file(&directory.join(file_name)))
                .collect::<Result<Vec<_>>>()?
        };
        Ok(Some(LoadedBundledFont {
            family: font.family,
            files,
        }))
    }

    pub(super) fn load_bundled_fonts(&self, families: &[String]) -> Result<Vec<LoadedBundledFont>> {
        let mut loaded = Vec::new();
        let mut seen = BTreeSet::new();
        for family in families {
            if !seen.insert(family.trim().to_lowercase()) {
                continue;
            }
            if let Some(font) = self.load_bundled_font(family)? {
                loaded.push(font);
            }
        }
        Ok(loaded)
    }

    fn find_font_directory(&self, font: &BundledFont) -> Option<PathBuf> {
        self.directories
            .iter()
            .find(|directory| {
                directory.is_dir()
                    && font
                        .files
                        .iter()
                        .all(|file_name| directory.join(file_name).is_file())
            })
            .cloned()
    }
}

impl FontRegistry {
    pub(super) fn new() -> Self {
        Self {
            resources: FontResources::new(),
            registered_families: BTreeSet::new(),
        }
    }

    pub(super) fn resources(&self) -> FontResources {
        self.resources.clone()
    }

    pub(super) fn is_registered(&self, family: &str) -> bool {
        bundled_font(family).is_some_and(|font| self.registered_families.contains(font.family))
    }

    pub(super) fn register_loaded_font(&mut self, font: LoadedBundledFont) -> Result<()> {
        if self.registered_families.contains(font.family) {
            return Ok(());
        }
        let mut collection = slint::fontique_010::shared_collection();
        for bytes in font.files {
            let registered = collection.register_fonts(fontique::Blob::new(Arc::new(bytes)), None);
            if registered.is_empty() {
                anyhow::bail!("bundled font data could not be registered");
            }
        }
        self.registered_families.insert(font.family);
        tracing::info!(
            family = font.family,
            "bundled font registered from resources"
        );
        Ok(())
    }
}

pub(super) fn discover_system_monospace_families() -> Vec<String> {
    let mut database = fontdb::Database::new();
    database.load_system_fonts();

    let mut families = BTreeMap::new();
    for face in database.faces().filter(|face| face.monospaced) {
        for (family, _) in &face.families {
            if let Some(family) = valid_font_family(family) {
                families.entry(family.to_lowercase()).or_insert(family);
            }
        }
    }
    families.into_values().collect()
}

pub(super) fn font_options(selected: &str, system_families: &[String]) -> Vec<String> {
    let bundled = BUNDLED_FONTS
        .iter()
        .map(|font| font.family.to_owned())
        .collect::<Vec<_>>();
    let bundled_keys = BUNDLED_FONTS
        .iter()
        .map(|font| font.family.to_lowercase())
        .collect::<BTreeSet<_>>();
    let mut system = BTreeMap::new();
    insert_system_font_family(&mut system, selected, &bundled_keys);
    for family in system_families {
        insert_system_font_family(&mut system, family, &bundled_keys);
    }

    let system_limit = MAX_FONT_OPTIONS.saturating_sub(bundled.len());
    let selected_key = valid_font_family(selected).map(|family| family.to_lowercase());
    let mut system = system.into_values().collect::<Vec<_>>();
    if system.len() > system_limit {
        let selected_index = selected_key.as_ref().and_then(|key| {
            system
                .iter()
                .position(|family| family.to_lowercase() == *key)
        });
        if let Some(index) = selected_index
            && index >= system_limit
        {
            let selected = system.remove(index);
            system.truncate(system_limit.saturating_sub(1));
            system.push(selected);
            system.sort_unstable_by_key(|family| family.to_lowercase());
        } else {
            system.truncate(system_limit);
        }
    }

    bundled.into_iter().chain(system).collect()
}

fn insert_system_font_family(
    families: &mut BTreeMap<String, String>,
    value: &str,
    bundled_keys: &BTreeSet<String>,
) {
    let Some(family) = valid_font_family(value) else {
        return;
    };
    let key = family.to_lowercase();
    if !bundled_keys.contains(&key) {
        families.entry(key).or_insert(family);
    }
}

fn bundled_font(family: &str) -> Option<&'static BundledFont> {
    BUNDLED_FONTS
        .iter()
        .find(|font| font.family.eq_ignore_ascii_case(family.trim()))
}

fn bundled_font_directories() -> Vec<PathBuf> {
    let mut directories = Vec::new();
    if let Ok(executable) = std::env::current_exe()
        && let Some(executable_directory) = executable.parent()
    {
        push_unique_path(
            &mut directories,
            executable_directory.join("assets").join("fonts"),
        );
        if let Some(parent) = executable_directory.parent() {
            push_unique_path(
                &mut directories,
                parent.join("Resources").join("assets").join("fonts"),
            );
            push_unique_path(
                &mut directories,
                parent
                    .join("share")
                    .join("ax_ssh")
                    .join("assets")
                    .join("fonts"),
            );
        }
    }
    if let Ok(working_directory) = std::env::current_dir() {
        push_unique_path(
            &mut directories,
            working_directory.join("assets").join("fonts"),
        );
    }
    directories
}

fn push_unique_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.iter().any(|existing| existing == &path) {
        paths.push(path);
    }
}

fn read_bundled_font_file(path: &Path) -> Result<Vec<u8>> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("cannot inspect bundled terminal font {}", path.display()))?;
    if metadata.len() == 0 || metadata.len() > MAX_BUNDLED_FONT_FILE_BYTES {
        anyhow::bail!("bundled terminal font file size is invalid");
    }
    fs::read(path).with_context(|| format!("cannot read bundled terminal font {}", path.display()))
}

fn valid_font_family(value: &str) -> Option<String> {
    let family = value.trim();
    (!family.is_empty()
        && family.chars().count() <= MAX_FONT_FAMILY_CHARS
        && !family.chars().any(char::is_control))
    .then(|| family.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_fonts_have_unique_valid_family_names() {
        let options = font_options("", &[]);
        assert_eq!(options.len(), BUNDLED_FONTS.len());
        assert!(
            options
                .iter()
                .all(|family| valid_font_family(family).is_some())
        );
    }

    #[test]
    fn bundled_fonts_always_precede_system_fonts() {
        let options = font_options(
            "Saved Monospace",
            &[
                "Zed Mono".to_owned(),
                "jetbrains mono".to_owned(),
                "Another Mono".to_owned(),
            ],
        );
        let bundled = BUNDLED_FONTS
            .iter()
            .map(|font| font.family)
            .collect::<Vec<_>>();

        assert_eq!(&options[..bundled.len()], bundled);
        assert_eq!(
            options[bundled.len()..],
            ["Another Mono", "Saved Monospace", "Zed Mono"]
        );
    }

    #[test]
    fn options_keep_the_selected_font_within_the_cap() {
        let selected = "Saved Monospace";
        let system_families = (0..MAX_FONT_OPTIONS + 20)
            .map(|index| format!("System Mono {index:03}"))
            .collect::<Vec<_>>();
        let options = font_options(selected, &system_families);

        assert_eq!(options.len(), MAX_FONT_OPTIONS);
        assert!(options.iter().any(|family| family == selected));
    }

    #[test]
    fn options_are_case_insensitively_deduplicated() {
        let options = font_options(
            "JetBrains Mono",
            &["jetbrains mono".to_owned(), "Another Mono".to_owned()],
        );

        assert_eq!(
            options
                .iter()
                .filter(|family| family.eq_ignore_ascii_case("JetBrains Mono"))
                .count(),
            1
        );
    }

    #[test]
    fn registry_matches_registered_bundled_families_case_insensitively() {
        let mut registry = FontRegistry::new();
        assert!(!registry.is_registered("jetbrains mono"));

        registry.registered_families.insert(BUNDLED_UI_FONT_FAMILY);

        assert!(registry.is_registered("jetbrains mono"));
        assert!(!registry.is_registered("System Monospace"));
    }

    #[test]
    fn default_ui_font_is_available_without_external_resources() {
        let resources = FontResources {
            directories: Vec::new(),
        };
        let loaded = resources
            .load_bundled_font(BUNDLED_UI_FONT_FAMILY)
            .expect("embedded UI font should load")
            .expect("default UI font should be bundled");

        assert_eq!(loaded.family, BUNDLED_UI_FONT_FAMILY);
        assert_eq!(loaded.files.len(), EMBEDDED_UI_FONT_FILES.len());
        assert!(loaded.files.iter().all(|bytes| !bytes.is_empty()));
    }
}

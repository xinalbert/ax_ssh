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
const TERMINAL_CJK_FALLBACK_FONT_FAMILY: &str = "Maple Mono NF CN";
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
        family: TERMINAL_CJK_FALLBACK_FONT_FAMILY,
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
    if started.swap(true, Ordering::AcqRel) {
        return;
    }
    let terminal_families = terminal_bundled_font_families(&family);
    let (resources, pending_families) = match font_registry.lock() {
        Ok(registry) => {
            let pending = terminal_families
                .into_iter()
                .filter(|family| !registry.is_registered(family))
                .collect::<Vec<_>>();
            if pending.is_empty() {
                return;
            }
            (registry.resources(), pending)
        }
        Err(_) => {
            started.store(false, Ordering::Release);
            tracing::warn!("cannot access font resources for terminal startup");
            return;
        }
    };
    runtime.spawn(async move {
        let loaded =
            tokio::task::spawn_blocking(move || resources.load_bundled_fonts(&pending_families))
                .await;
        let fonts = match loaded {
            Ok(Ok(fonts)) => fonts,
            Ok(Err(error)) => {
                tracing::warn!(%error, "failed to read terminal font resources");
                started.store(false, Ordering::Release);
                return;
            }
            Err(error) => {
                tracing::warn!(%error, "terminal font task failed");
                started.store(false, Ordering::Release);
                return;
            }
        };
        super::view::dispatch_ui(&ui, move |ui| {
            for font in fonts {
                let registration = font_registry
                    .lock()
                    .map_err(|_| anyhow::anyhow!("font registry lock poisoned"))
                    .and_then(|mut registry| registry.register_loaded_font(font));
                if let Err(error) = registration {
                    started.store(false, Ordering::Release);
                    tracing::warn!(%error, "failed to register terminal font resources");
                    ui.set_status(format!("Cannot register terminal font: {error}").into());
                    return;
                }
            }
        });
    });
}

#[derive(Debug)]
pub(super) struct LoadedBundledFont {
    family: &'static str,
    source: BundledFontSource,
}

#[derive(Debug)]
enum BundledFontSource {
    Embedded,
    Paths(Vec<PathBuf>),
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
        let source = if font.family == BUNDLED_UI_FONT_FAMILY {
            BundledFontSource::Embedded
        } else {
            let directory = self.find_font_directory(font).with_context(|| {
                format!("bundled font resources are unavailable for {}", font.family)
            })?;
            let paths = font
                .files
                .iter()
                .map(|file_name| directory.join(file_name))
                .collect::<Vec<_>>();
            for path in &paths {
                validate_bundled_font_file(path)?;
            }
            BundledFontSource::Paths(paths)
        };
        Ok(Some(LoadedBundledFont {
            family: font.family,
            source,
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
        let family = font.family;
        register_loaded_font_in_collection(&mut collection, font)?;
        self.registered_families.insert(family);
        tracing::info!(family, "bundled font registered from resources");
        Ok(())
    }
}

fn terminal_bundled_font_families(primary_family: &str) -> Vec<String> {
    let mut families = bundled_font(primary_family)
        .map(|font| vec![font.family.to_owned()])
        .unwrap_or_default();
    if !families
        .iter()
        .any(|family| family == TERMINAL_CJK_FALLBACK_FONT_FAMILY)
    {
        families.push(TERMINAL_CJK_FALLBACK_FONT_FAMILY.to_owned());
    }
    families
}

fn register_loaded_font_in_collection(
    collection: &mut fontique::Collection,
    font: LoadedBundledFont,
) -> Result<()> {
    match font.source {
        BundledFontSource::Embedded => {
            for bytes in EMBEDDED_UI_FONT_FILES {
                let registered =
                    collection.register_fonts(fontique::Blob::new(Arc::new(bytes.to_vec())), None);
                if registered.is_empty() {
                    anyhow::bail!("bundled font data could not be registered");
                }
            }
        }
        BundledFontSource::Paths(paths) => {
            collection.load_fonts_from_paths(paths);
        }
    }

    let family_id = collection
        .family_id(font.family)
        .ok_or_else(|| anyhow::anyhow!("bundled font family was not registered"))?;
    if font.family == TERMINAL_CJK_FALLBACK_FONT_FAMILY
        && !collection.set_fallbacks(
            fontique::FallbackKey::new(fontique::Script::from_bytes(*b"Hani"), None),
            std::iter::once(family_id),
        )
    {
        anyhow::bail!("bundled CJK fallback could not be configured");
    }
    Ok(())
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

fn validate_bundled_font_file(path: &Path) -> Result<()> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("cannot inspect bundled terminal font {}", path.display()))?;
    if metadata.len() == 0 || metadata.len() > MAX_BUNDLED_FONT_FILE_BYTES {
        anyhow::bail!("bundled terminal font file size is invalid");
    }
    Ok(())
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
        assert!(matches!(loaded.source, BundledFontSource::Embedded));
    }

    #[test]
    fn terminal_font_loading_uses_one_cjk_fallback_path() {
        assert_eq!(
            terminal_bundled_font_families("JetBrains Mono"),
            ["JetBrains Mono", TERMINAL_CJK_FALLBACK_FONT_FAMILY]
        );
        assert_eq!(
            terminal_bundled_font_families(TERMINAL_CJK_FALLBACK_FONT_FAMILY),
            [TERMINAL_CJK_FALLBACK_FONT_FAMILY]
        );
        assert_eq!(
            terminal_bundled_font_families("System Monospace"),
            [TERMINAL_CJK_FALLBACK_FONT_FAMILY]
        );
    }

    #[test]
    fn maple_is_the_only_registered_han_fallback() {
        let resources = FontResources {
            directories: vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/fonts")],
        };
        let maple = resources
            .load_bundled_font(TERMINAL_CJK_FALLBACK_FONT_FAMILY)
            .expect("Maple font resources should load")
            .expect("Maple should be a bundled font");
        assert!(matches!(&maple.source, BundledFontSource::Paths(_)));
        let mut collection = fontique::Collection::new(fontique::CollectionOptions {
            shared: false,
            system_fonts: false,
        });

        register_loaded_font_in_collection(&mut collection, maple)
            .expect("Maple should register as the Han fallback");

        let fallback_ids = collection
            .fallback_families(fontique::FallbackKey::new(
                fontique::Script::from_bytes(*b"Hani"),
                None,
            ))
            .collect::<Vec<_>>();
        assert_eq!(fallback_ids.len(), 1);
        assert_eq!(
            collection.family_name(fallback_ids[0]),
            Some(TERMINAL_CJK_FALLBACK_FONT_FAMILY)
        );
    }
}

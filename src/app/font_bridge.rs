use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use slint::ComponentHandle;
use slint::fontique_011::fontique;

const MAX_FONT_FAMILY_CHARS: usize = 128;
const MAX_FONT_OPTIONS: usize = 256;
const MAX_BUNDLED_FONT_FILE_BYTES: u64 = 24 * 1024 * 1024;

const BUNDLED_UI_FONT_FAMILY: &str = "JetBrains Mono";
const TERMINAL_CJK_FALLBACK_FONT_FAMILY: &str = "Maple Mono NF CN";
const TERMINAL_SYMBOL_FALLBACK_FONT_FAMILY: &str = "Iosevka Term";

pub(super) fn terminal_emoji_font_family() -> String {
    // Generic-family resolution is process-local and independent of terminal
    // content. Keep one name per UI thread, rather than querying per glyph.
    thread_local! {
        static FAMILY: std::cell::OnceCell<String> = const { std::cell::OnceCell::new() };
    }
    FAMILY.with(|family| {
        family
            .get_or_init(|| {
                let mut collection = slint::fontique_011::shared_collection();
                let id = collection
                    .generic_families(fontique::GenericFamily::Emoji)
                    .next();
                id.and_then(|id| collection.family_name(id).map(str::to_owned))
                    .unwrap_or_default()
            })
            .clone()
    })
}
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
        family: TERMINAL_SYMBOL_FALLBACK_FONT_FAMILY,
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
    generation: u64,
}

pub(super) fn load_terminal_font_on_demand(
    runtime: &tokio::runtime::Handle,
    ui: slint::Weak<super::AppWindow>,
    state: Arc<Mutex<super::AppState>>,
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
            (registry.resources(), pending)
        }
        Err(_) => {
            started.store(false, Ordering::Release);
            tracing::warn!("cannot access font resources for terminal startup");
            return;
        }
    };
    runtime.spawn(async move {
        let loaded = tokio::task::spawn_blocking(move || {
            let fonts = resources.load_bundled_fonts(&pending_families)?;
            let system_families = discover_system_monospace_families()
                .into_iter()
                .take(MAX_FONT_OPTIONS)
                .collect::<Vec<_>>();
            Ok::<_, anyhow::Error>((fonts, system_families))
        })
        .await;
        let (fonts, system_families) = match loaded {
            Ok(Ok(loaded)) => loaded,
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
            let generation = font_registry
                .lock()
                .map_err(|_| anyhow::anyhow!("font registry lock poisoned"))
                .and_then(|mut registry| {
                    registry.register_system_terminal_fallbacks(&system_families)?;
                    Ok(registry.generation_as_slint_int())
                });
            let generation = match generation {
                Ok(generation) => generation,
                Err(error) => {
                    started.store(false, Ordering::Release);
                    tracing::warn!(%error, "failed to configure terminal font fallbacks");
                    ui.set_status("Cannot update terminal font state".into());
                    return;
                }
            };
            super::view::set_font_registry_generation(ui, generation);
            super::view::refresh_workspace(&ui.as_weak(), &state);
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
            generation: 0,
        }
    }

    pub(super) fn resources(&self) -> FontResources {
        self.resources.clone()
    }

    pub(super) fn is_registered(&self, family: &str) -> bool {
        bundled_font(family).is_some_and(|font| self.registered_families.contains(font.family))
    }

    pub(super) fn generation_as_slint_int(&self) -> i32 {
        self.generation.min(i32::MAX as u64) as i32
    }

    pub(super) fn register_loaded_font(&mut self, font: LoadedBundledFont) -> Result<()> {
        if self.registered_families.contains(font.family) {
            return Ok(());
        }
        let mut collection = slint::fontique_011::shared_collection();
        let family = font.family;
        register_loaded_font_in_collection(&mut collection, font)?;
        self.registered_families.insert(family);
        self.generation = self.generation.wrapping_add(1);
        tracing::info!(family, "bundled font registered from resources");
        Ok(())
    }

    fn register_system_terminal_fallbacks(&mut self, families: &[String]) -> Result<()> {
        configure_terminal_fallbacks(&mut slint::fontique_011::shared_collection(), families)?;
        self.generation = self.generation.wrapping_add(1);
        tracing::info!(
            candidate_families = families.len(),
            "terminal font fallbacks configured"
        );
        Ok(())
    }
}

fn terminal_bundled_font_families(primary_family: &str) -> Vec<String> {
    let mut families = bundled_font(primary_family)
        .map(|font| vec![font.family.to_owned()])
        .unwrap_or_default();
    for fallback in [
        TERMINAL_CJK_FALLBACK_FONT_FAMILY,
        TERMINAL_SYMBOL_FALLBACK_FONT_FAMILY,
    ] {
        if !families.iter().any(|family| family == fallback) {
            families.push(fallback.to_owned());
        }
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

    collection
        .family_id(font.family)
        .ok_or_else(|| anyhow::anyhow!("bundled font family was not registered"))?;
    if matches!(
        font.family,
        TERMINAL_CJK_FALLBACK_FONT_FAMILY | TERMINAL_SYMBOL_FALLBACK_FONT_FAMILY
    ) {
        configure_terminal_fallbacks(collection, &[])?;
    }
    Ok(())
}

fn configure_terminal_fallbacks(
    collection: &mut fontique::Collection,
    system_families: &[String],
) -> Result<()> {
    // Keep bundled coverage first, then installed monospace faces, then the
    // original platform fallbacks. Fontique also tries Han fallbacks for
    // neutral characters, so both chains must retain system coverage.
    let system_ids = system_families
        .iter()
        .take(MAX_FONT_OPTIONS)
        .filter_map(|family| collection.family_id(family))
        .collect::<Vec<_>>();
    for (script, bundled) in [
        (
            *b"Hani",
            [
                TERMINAL_CJK_FALLBACK_FONT_FAMILY,
                TERMINAL_SYMBOL_FALLBACK_FONT_FAMILY,
            ],
        ),
        (
            *b"Zyyy",
            [
                TERMINAL_SYMBOL_FALLBACK_FONT_FAMILY,
                TERMINAL_CJK_FALLBACK_FONT_FAMILY,
            ],
        ),
    ] {
        let key = fontique::FallbackKey::new(fontique::Script::from_bytes(script), None);
        // Read before replacing: this materializes the platform default too.
        let existing = collection.fallback_families(key).collect::<Vec<_>>();
        let mut families = bundled
            .into_iter()
            .filter_map(|family| collection.family_id(family))
            .collect::<Vec<_>>();
        for id in system_ids.iter().copied().chain(existing) {
            if !families.contains(&id) {
                families.push(id);
            }
        }
        if !families.is_empty() && !collection.set_fallbacks(key, families.into_iter()) {
            anyhow::bail!("terminal font fallbacks could not be configured for {script:?}");
        }
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
mod tests;

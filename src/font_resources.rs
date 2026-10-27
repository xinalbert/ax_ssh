//! Shared bundled-font inventory and bounded family options, without UI types.
//!
//! Font file reads and native registration remain with each frontend's loader.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

pub const MAX_FONT_FAMILY_CHARS: usize = 128;
pub const MAX_FONT_OPTIONS: usize = 256;
pub const MAX_BUNDLED_FONT_FILE_BYTES: u64 = 24 * 1024 * 1024;

pub const BUNDLED_UI_FONT_FAMILY: &str = "JetBrains Mono";
pub const TERMINAL_CJK_FALLBACK_FONT_FAMILY: &str = "Maple Mono NF CN";
pub const TERMINAL_SYMBOL_FALLBACK_FONT_FAMILY: &str = "Iosevka Term";

pub const EMBEDDED_UI_FONT_FILES: &[&[u8]] = &[
    include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono-Italic.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono-BoldItalic.ttf"),
];

/// One project-owned family and its allowlisted runtime resource filenames.
pub struct BundledFont {
    pub family: &'static str,
    pub files: &'static [&'static str],
}

pub const BUNDLED_FONTS: &[BundledFont] = &[
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

/// Discover bounded, sorted family names. Call only outside the GUI thread.
pub fn discover_system_monospace_families() -> Vec<String> {
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
    families.into_values().take(MAX_FONT_OPTIONS).collect()
}

/// Bundle-first choices, deduplicated with a retained valid selection under the cap.
pub fn font_options(selected: &str, system_families: &[String]) -> Vec<String> {
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

/// Look up a bundled family without consulting the filesystem.
pub fn bundled_font(family: &str) -> Option<&'static BundledFont> {
    BUNDLED_FONTS
        .iter()
        .find(|font| font.family.eq_ignore_ascii_case(family.trim()))
}

/// Resolve packaging resource locations; call outside GUI callbacks.
pub fn bundled_font_directories() -> Vec<PathBuf> {
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

/// Normalize a bounded family name, rejecting control characters and empty names.
pub fn valid_font_family(value: &str) -> Option<String> {
    let family = value.trim();
    (!family.is_empty()
        && family.chars().count() <= MAX_FONT_FAMILY_CHARS
        && !family.chars().any(char::is_control))
    .then(|| family.to_owned())
}

#[cfg(test)]
mod tests;

//! Fixed bundled terminal families plus egui's native system fallback provider.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use eframe::egui::{FontData, FontDefinitions, FontFamily, FontId};

const MAX_FONT_BYTES: u64 = 24 * 1024 * 1024;
const FACES: &[(&str, &[u8])] = &[
    (
        "regular",
        include_bytes!("../../assets/fonts/JetBrainsMono-Regular.ttf"),
    ),
    (
        "bold",
        include_bytes!("../../assets/fonts/JetBrainsMono-Bold.ttf"),
    ),
    (
        "italic",
        include_bytes!("../../assets/fonts/JetBrainsMono-Italic.ttf"),
    ),
    (
        "bold-italic",
        include_bytes!("../../assets/fonts/JetBrainsMono-BoldItalic.ttf"),
    ),
];

pub(super) fn font(size: f32, bold: bool, italic: bool) -> FontId {
    let face = match (bold, italic) {
        (false, false) => "regular",
        (true, false) => "bold",
        (false, true) => "italic",
        (true, true) => "bold-italic",
    };
    FontId::new(size, FontFamily::Name(format!("terminal-{face}").into()))
}

pub(super) fn load() -> Result<FontDefinitions> {
    let mut definitions = FontDefinitions::default();
    let mut fallbacks = Vec::new();
    let mut candidates = Vec::new();
    if let Some(parent) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(PathBuf::from))
    {
        candidates.push(parent.join("assets/fonts"));
        candidates.push(parent.join("../Resources/assets/fonts"));
    }
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/fonts"));
    for filename in ["MapleMono-NF-CN-Regular.ttf", "IosevkaTerm-Regular.ttf"] {
        if let Some(path) = candidates
            .iter()
            .map(|root| root.join(filename))
            .find(|p| p.is_file())
        {
            let metadata = path
                .metadata()
                .with_context(|| format!("cannot inspect font {filename}"))?;
            if metadata.len() > MAX_FONT_BYTES {
                anyhow::bail!("bundled font exceeds size limit: {filename}");
            }
            let mut bytes = Vec::with_capacity(metadata.len() as usize);
            std::fs::File::open(path)
                .with_context(|| format!("cannot open font {filename}"))?
                .take(MAX_FONT_BYTES + 1)
                .read_to_end(&mut bytes)
                .with_context(|| format!("cannot read font {filename}"))?;
            if bytes.len() as u64 > MAX_FONT_BYTES {
                anyhow::bail!("bundled font grew beyond the size limit: {filename}");
            }
            definitions
                .font_data
                .insert(filename.to_owned(), Arc::new(FontData::from_owned(bytes)));
            fallbacks.push(filename.to_owned());
        }
    }
    let mut families = BTreeMap::new();
    for (name, bytes) in FACES {
        definitions
            .font_data
            .insert((*name).to_owned(), Arc::new(FontData::from_static(bytes)));
        let mut chain = vec![(*name).to_owned()];
        chain.extend(fallbacks.clone());
        families.insert(FontFamily::Name(format!("terminal-{name}").into()), chain);
    }
    definitions.families.extend(families);
    Ok(definitions)
}

//! Font discovery and bounded reads; this module runs only on blocking workers.

use super::{Loaded, STYLES, Selection, check_cancelled};
use anyhow::{Context, Result, ensure};
use ax_ssh::font_resources::{
    self, BUNDLED_UI_FONT_FAMILY, EMBEDDED_UI_FONT_FILES, MAX_BUNDLED_FONT_FILE_BYTES,
    MAX_FONT_OPTIONS, TERMINAL_CJK_FALLBACK_FONT_FAMILY, TERMINAL_SYMBOL_FALLBACK_FONT_FAMILY,
};
use eframe::egui::{FontData, FontDefinitions, FontFamily};
use std::{
    collections::{BTreeMap, HashMap},
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
};

type Blob = Arc<dyn AsRef<[u8]> + Send + Sync>;
const MAX_REQUEST_BYTES: usize = 96 * 1024 * 1024;

pub(crate) struct Loader {
    system: fontdb::Database,
    directories: Vec<PathBuf>,
    pub(super) options: Arc<Vec<String>>,
    pub(super) baseline: FontDefinitions,
    #[cfg(test)]
    pub(super) before_load: Option<Arc<dyn Fn() + Send + Sync>>,
}

struct Faces {
    data: [Arc<FontData>; 4],
    synthetic: [bool; 2],
    family: String,
}

#[derive(Default)]
struct Reads {
    files: HashMap<PathBuf, Blob>,
    binaries: Vec<Blob>,
    total: usize,
}

impl Reads {
    fn file(&mut self, path: &Path, cancelled: &AtomicBool) -> Result<Blob> {
        check_cancelled(cancelled)?;
        if let Some(bytes) = self.files.get(path) {
            return Ok(bytes.clone());
        }
        let mut file =
            File::open(path).with_context(|| format!("cannot open font {}", path.display()))?;
        let metadata = file.metadata().context("cannot inspect font file")?;
        ensure!(
            metadata.is_file()
                && metadata.len() > 0
                && metadata.len() <= MAX_BUNDLED_FONT_FILE_BYTES,
            "font file exceeds the 24 MiB limit or is not a regular file"
        );
        ensure!(
            self.total.saturating_add(metadata.len() as usize) <= MAX_REQUEST_BYTES,
            "font request exceeds the 96 MiB budget"
        );
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        let mut block = [0u8; 64 * 1024];
        loop {
            check_cancelled(cancelled)?;
            let count = file.read(&mut block).context("cannot read font file")?;
            if count == 0 {
                break;
            }
            ensure!(
                bytes.len().saturating_add(count) <= MAX_BUNDLED_FONT_FILE_BYTES as usize,
                "font file grew beyond the size limit"
            );
            ensure!(
                self.total.saturating_add(bytes.len()).saturating_add(count) <= MAX_REQUEST_BYTES,
                "font request exceeds the 96 MiB budget"
            );
            bytes.extend_from_slice(&block[..count]);
        }
        self.total += bytes.len();
        let blob: Blob = Arc::new(bytes);
        self.files.insert(path.to_owned(), blob.clone());
        Ok(blob)
    }

    fn source(&mut self, source: &fontdb::Source, cancelled: &AtomicBool) -> Result<Blob> {
        match source {
            fontdb::Source::Binary(bytes) => {
                ensure!(
                    bytes.as_ref().as_ref().len() <= MAX_BUNDLED_FONT_FILE_BYTES as usize,
                    "font data exceeds the size limit"
                );
                check_cancelled(cancelled)?;
                if !self
                    .files
                    .values()
                    .chain(&self.binaries)
                    .any(|loaded| Arc::ptr_eq(loaded, bytes))
                {
                    let count = bytes.as_ref().as_ref().len();
                    ensure!(
                        self.total.saturating_add(count) <= MAX_REQUEST_BYTES,
                        "font request exceeds the 96 MiB budget"
                    );
                    self.total += count;
                    self.binaries.push(bytes.clone());
                }
                Ok(bytes.clone())
            }
            fontdb::Source::File(path) | fontdb::Source::SharedFile(path, _) => {
                self.file(path, cancelled)
            }
        }
    }
}

impl Loader {
    pub(crate) fn discover() -> Result<Self> {
        let mut system = fontdb::Database::new();
        system.load_system_fonts();
        Self::new(system, resources())
    }

    pub(crate) fn bootstrap(&self) -> FontDefinitions {
        self.baseline.clone()
    }

    #[cfg(test)]
    pub(crate) fn bundled() -> Result<Self> {
        Self::new(fontdb::Database::new(), resources())
    }

    fn new(system: fontdb::Database, directories: Vec<PathBuf>) -> Result<Self> {
        let mut families = BTreeMap::new();
        for face in system.faces().filter(|face| face.monospaced) {
            for (name, _) in &face.families {
                if let Some(name) = font_resources::valid_font_family(name) {
                    families.entry(name.to_lowercase()).or_insert(name);
                }
            }
        }
        let mut baseline = FontDefinitions::default();
        for (style, bytes) in STYLES.iter().zip(EMBEDDED_UI_FONT_FILES) {
            baseline
                .font_data
                .insert((*style).into(), Arc::new(FontData::from_static(bytes)));
        }
        let cancelled = AtomicBool::new(false);
        let mut reads = Reads::default();
        for family in [
            TERMINAL_CJK_FALLBACK_FONT_FAMILY,
            TERMINAL_SYMBOL_FALLBACK_FONT_FAMILY,
        ] {
            let asset =
                font_resources::bundled_font(family).context("missing bundled font inventory")?;
            let path = find_file(&directories, asset.files[0])
                .with_context(|| format!("required font unavailable: {family}"))?;
            let blob = reads.file(&path, &cancelled)?;
            validate_face(&blob, 0)?;
            baseline
                .font_data
                .insert(family.into(), Arc::new(FontData::from_blob(blob, 0)));
        }
        for style in STYLES {
            baseline.families.insert(
                FontFamily::Name(format!("terminal-{style}").into()),
                chain(style),
            );
        }
        prepend_application(&mut baseline, "regular");
        Ok(Self {
            system,
            directories,
            options: Arc::new(families.into_values().take(MAX_FONT_OPTIONS).collect()),
            baseline,
            #[cfg(test)]
            before_load: None,
        })
    }

    pub(super) fn load(&mut self, selection: &Selection, cancelled: &AtomicBool) -> Result<Loaded> {
        #[cfg(test)]
        if let Some(before_load) = &self.before_load {
            before_load();
        }
        check_cancelled(cancelled)?;
        let mut reads = Reads::default();
        let mut warnings = Vec::new();
        let mut definitions = self.baseline.clone();
        let mut application = BUNDLED_UI_FONT_FAMILY.to_owned();
        let mut terminal = application.clone();
        let mut synthetic_italic = [false; 2];
        for (role, family, fixed) in [
            ("application", selection.application.as_str(), false),
            ("terminal", selection.terminal.as_str(), true),
        ] {
            let faces = match self.family(family, fixed, &mut reads, cancelled) {
                Ok(faces) => faces,
                Err(error) => {
                    check_cancelled(cancelled)?;
                    warnings.push(
                        format!("{family}: {error:#}; using {BUNDLED_UI_FONT_FAMILY}")
                            .chars()
                            .take(1024)
                            .collect(),
                    );
                    self.default_faces()?
                }
            };
            for (index, data) in faces.data.into_iter().enumerate() {
                let name = format!("loaded-{role}-{}", STYLES[index]);
                definitions.font_data.insert(name.clone(), data);
                if fixed {
                    let mut fallback = vec![name];
                    fallback.extend(chain(STYLES[index]));
                    definitions.families.insert(
                        FontFamily::Name(format!("terminal-{}", STYLES[index]).into()),
                        fallback,
                    );
                } else if index == 0 {
                    prepend_application(&mut definitions, &name);
                }
            }
            if fixed {
                synthetic_italic = faces.synthetic;
                terminal = faces.family;
            } else {
                application = faces.family;
            }
        }
        check_cancelled(cancelled)?;
        Ok(Loaded {
            definitions,
            application,
            terminal,
            synthetic_italic,
            warnings,
        })
    }

    fn default_faces(&self) -> Result<Faces> {
        let data = STYLES
            .iter()
            .map(|style| {
                self.baseline
                    .font_data
                    .get(*style)
                    .cloned()
                    .context("embedded font missing")
            })
            .collect::<Result<Vec<_>>>()?
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid embedded face count"))?;
        Ok(Faces {
            data,
            synthetic: [false; 2],
            family: BUNDLED_UI_FONT_FAMILY.into(),
        })
    }

    fn family(
        &self,
        family: &str,
        fixed: bool,
        reads: &mut Reads,
        cancelled: &AtomicBool,
    ) -> Result<Faces> {
        if family.eq_ignore_ascii_case(BUNDLED_UI_FONT_FAMILY) {
            return self.default_faces();
        }
        let mut bundled = fontdb::Database::new();
        let database = if let Some(asset) = font_resources::bundled_font(family) {
            for file in asset.files {
                check_cancelled(cancelled)?;
                let path = find_file(&self.directories, file)
                    .with_context(|| format!("bundled font resource unavailable: {file}"))?;
                bundled.load_font_source(fontdb::Source::Binary(reads.file(&path, cancelled)?));
            }
            &bundled
        } else {
            &self.system
        };
        let canonical = database
            .faces()
            .flat_map(|face| &face.families)
            .find(|(name, _)| name.eq_ignore_ascii_case(family))
            .map(|(name, _)| name.as_str())
            .context("font family is not installed")?;
        let mut data = Vec::new();
        let mut synthetic = [false; 2];
        for (bold, italic) in [(false, false), (true, false), (false, true), (true, true)] {
            check_cancelled(cancelled)?;
            let id = database
                .query(&fontdb::Query {
                    families: &[fontdb::Family::Name(canonical)],
                    weight: if bold {
                        fontdb::Weight::BOLD
                    } else {
                        fontdb::Weight::NORMAL
                    },
                    style: if italic {
                        fontdb::Style::Italic
                    } else {
                        fontdb::Style::Normal
                    },
                    ..Default::default()
                })
                .context("font face is unavailable")?;
            let face = database.face(id).context("font face disappeared")?;
            let blob = reads.source(&face.source, cancelled)?;
            validate_face(&blob, face.index)?;
            let mut font = FontData::from_blob(blob, face.index);
            let mut coordinates = Vec::new();
            let mut variable_italic = false;
            for axis in font.variation_axes() {
                let tag = axis.tag.to_string();
                let value: Option<f32> = match tag.as_str() {
                    "wght" => Some(if bold { 700.0 } else { 400.0 }),
                    "ital" if italic => {
                        variable_italic = true;
                        Some(1.0)
                    }
                    "slnt" if italic => {
                        variable_italic = true;
                        Some(-12.0)
                    }
                    _ => None,
                };
                if let Some(value) = value {
                    coordinates.push((axis.tag, value.clamp(axis.range.min, axis.range.max)));
                }
            }
            font.tweak.coords = eframe::egui::epaint::text::VariationCoords::new(coordinates);
            if italic {
                synthetic[usize::from(bold)] =
                    face.style == fontdb::Style::Normal && !variable_italic;
            }
            data.push(Arc::new(font));
        }
        let faces = Faces {
            data: data
                .try_into()
                .map_err(|_| anyhow::anyhow!("invalid font face count"))?,
            synthetic,
            family: canonical.into(),
        };
        if fixed {
            validate_grid(&faces)?;
        }
        Ok(faces)
    }
}

// Metadata alone can claim monospace incorrectly. Measure bounded ASCII samples
// with the same font engine as the terminal, on this blocking worker.
fn validate_grid(faces: &Faces) -> Result<()> {
    let mut definitions = FontDefinitions::default();
    for (style, data) in STYLES.iter().zip(&faces.data) {
        definitions.font_data.insert((*style).into(), data.clone());
        definitions
            .families
            .insert(FontFamily::Name((*style).into()), vec![(*style).into()]);
    }
    let ctx = eframe::egui::Context::default();
    ctx.set_fonts(definitions);
    let mut equal = true;
    let output = ctx.run_ui(eframe::egui::RawInput::default(), |ui| {
        ui.fonts_mut(|fonts| {
            for style in STYLES {
                let font = eframe::egui::FontId::new(20.0, FontFamily::Name(style.into()));
                let width = fonts.glyph_width(&font, 'M');
                equal &= width.is_finite() && width > 0.0;
                for byte in b' '..=b'~' {
                    equal &= (fonts.glyph_width(&font, char::from(byte)) - width).abs() <= 0.1;
                }
            }
        });
    });
    output.drop_without_applying_deltas();
    ensure!(
        equal,
        "terminal font is proportional (unequal Latin advances)"
    );
    Ok(())
}

fn validate_face(blob: &Blob, index: u32) -> Result<()> {
    let mut database = fontdb::Database::new();
    database.load_font_source(fontdb::Source::Binary(blob.clone()));
    ensure!(
        database.faces().any(|face| face.index == index),
        "font bytes are invalid or have no requested collection face"
    );
    Ok(())
}

fn chain(primary: &str) -> Vec<String> {
    [
        primary,
        TERMINAL_CJK_FALLBACK_FONT_FAMILY,
        TERMINAL_SYMBOL_FALLBACK_FONT_FAMILY,
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

fn prepend_application(definitions: &mut FontDefinitions, primary: &str) {
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        let mut names = chain(primary);
        names.extend(
            definitions
                .families
                .get(&family)
                .into_iter()
                .flatten()
                .cloned(),
        );
        let mut seen = std::collections::HashSet::new();
        names.retain(|name| seen.insert(name.clone()));
        definitions.families.insert(family, names);
    }
}

fn resources() -> Vec<PathBuf> {
    let mut directories = font_resources::bundled_font_directories();
    directories.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/fonts"));
    directories
}

fn find_file(directories: &[PathBuf], name: &str) -> Option<PathBuf> {
    directories
        .iter()
        .map(|directory| directory.join(name))
        .find(|path| path.is_file())
}

#[cfg(test)]
mod tests;

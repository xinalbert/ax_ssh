use std::fs::File;
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::*;

const THEME_QUERY_TIMEOUT: Duration = Duration::from_millis(300);
const THEME_OUTPUT_LIMIT: u64 = 256;
const THEME_NAME_LIMIT: usize = 64;

pub(in crate::app::file_icons) struct Resolver {
    theme: Mutex<String>,
}

impl Default for Resolver {
    fn default() -> Self {
        Self {
            theme: Mutex::new("hicolor".to_owned()),
        }
    }
}

impl IconResolver for Resolver {
    fn cache_identity(&self) -> CacheIdentity {
        let theme = detect_icon_theme().unwrap_or_else(|| "hicolor".to_owned());
        if let Ok(mut current) = self.theme.lock() {
            *current = theme.clone();
        }
        CacheIdentity::new(&format!("linux-freedesktop-{theme}-v1"))
    }

    fn resolve(&self, key: &FileIconKey) -> Option<FileIcon> {
        let theme = self
            .theme
            .lock()
            .map_or_else(|_| "hicolor".to_owned(), |theme| theme.clone());
        for name in icon_names(key) {
            let Some(path) = freedesktop_icons::lookup(&name)
                .with_size(FILE_ICON_EDGE as u16)
                .with_theme(&theme)
                .find()
            else {
                continue;
            };
            if let Some(icon) = read_icon_file(&path) {
                return Some(icon);
            }
        }
        None
    }
}

fn icon_names(key: &FileIconKey) -> Vec<String> {
    match key {
        FileIconKey::Folder => vec!["folder".to_owned()],
        FileIconKey::Symlink => vec![
            "inode-symlink".to_owned(),
            "emblem-symbolic-link".to_owned(),
        ],
        FileIconKey::GenericFile => vec![
            "application-octet-stream".to_owned(),
            "text-x-generic".to_owned(),
        ],
        FileIconKey::Extension(extension) => {
            let Some(mime) = mime_guess::from_ext(extension).first_raw() else {
                return vec!["application-octet-stream".to_owned()];
            };
            let major = mime
                .split_once('/')
                .map(|(major, _)| major)
                .unwrap_or("application");
            vec![
                mime.replace('/', "-"),
                format!("{major}-x-generic"),
                "text-x-generic".to_owned(),
                "application-octet-stream".to_owned(),
            ]
        }
    }
}

fn read_icon_file(path: &std::path::Path) -> Option<FileIcon> {
    let file = File::open(path).ok()?;
    let mut encoded = Vec::new();
    file.take(MAX_ENCODED_ICON_BYTES as u64 + 1)
        .read_to_end(&mut encoded)
        .ok()?;
    if encoded.len() > MAX_ENCODED_ICON_BYTES {
        return None;
    }
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"))
    {
        return decode_svg_icon(&encoded);
    }
    decode_encoded_icon(&encoded)
}

fn decode_svg_icon(encoded: &[u8]) -> Option<FileIcon> {
    let image = slint::Image::load_from_svg_data(encoded).ok()?;
    let size = image.size();
    if size.width == 0
        || size.height == 0
        || size.width > MAX_SOURCE_ICON_EDGE
        || size.height > MAX_SOURCE_ICON_EDGE
    {
        return None;
    }
    let source = image.to_rgba8()?;
    let source =
        image::RgbaImage::from_raw(source.width(), source.height(), source.as_bytes().to_vec())?;
    let rgba = image::imageops::resize(
        &source,
        FILE_ICON_EDGE,
        FILE_ICON_EDGE,
        image::imageops::FilterType::Triangle,
    )
    .into_raw();
    FileIcon::from_rgba(FILE_ICON_EDGE, FILE_ICON_EDGE, rgba)
}

fn detect_icon_theme() -> Option<String> {
    let mut child = Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "icon-theme"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + THEME_QUERY_TIMEOUT;
    let status = loop {
        if let Some(status) = child.try_wait().ok()? {
            break status;
        }
        if Instant::now() >= deadline {
            if let Err(error) = child.kill()
                && error.kind() != std::io::ErrorKind::InvalidInput
            {
                return None;
            }
            child.wait().ok()?;
            return None;
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    if !status.success() {
        return None;
    }
    let mut output = Vec::new();
    child
        .stdout
        .take()?
        .take(THEME_OUTPUT_LIMIT + 1)
        .read_to_end(&mut output)
        .ok()?;
    if output.len() as u64 > THEME_OUTPUT_LIMIT {
        return None;
    }
    let theme = std::str::from_utf8(&output).ok()?.trim().trim_matches('\'');
    if theme.is_empty()
        || theme.chars().count() > THEME_NAME_LIMIT
        || theme
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\'))
    {
        return None;
    }
    Some(theme.to_owned())
}

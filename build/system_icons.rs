//! Bounded, offline rasterization of the checked-in catalog for egui only.

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::io::Read;
use std::path::{Component, Path};

use resvg::{tiny_skia, usvg};
use serde::Deserialize;

const SIDE: u32 = 48;
const MAX_ICONS: usize = 128;
const MAX_BYTES: usize = 2 * 1024 * 1024;

#[derive(Deserialize)]
struct Catalog {
    schema_version: u32,
    icons: Vec<Icon>,
}

#[derive(Deserialize)]
struct Icon {
    id: String,
    icon: String,
}

pub(super) fn generate(root: &Path, output: &Path) -> Result<(), Box<dyn Error>> {
    let source = root.join("catalog.json");
    println!("cargo:rerun-if-changed={}", source.display());
    let catalog: Catalog = serde_json::from_slice(&read_bounded(&source)?)?;
    if catalog.schema_version != 1 || catalog.icons.is_empty() || catalog.icons.len() > MAX_ICONS {
        return Err("unsupported or unbounded system icon catalog".into());
    }
    let root = root.canonicalize()?;
    let mut assets = BTreeMap::new();
    for icon in catalog.icons {
        if icon.id.is_empty()
            || icon.id.len() > 64
            || !icon
                .id
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            || !Path::new(&icon.icon)
                .components()
                .all(|component| matches!(component, Component::Normal(_)))
        {
            return Err(format!("invalid system icon record: {}", icon.id).into());
        }
        let path = root.join(&icon.icon).canonicalize()?;
        if !path.starts_with(&root) || fs::metadata(&path)?.len() > MAX_BYTES as u64 {
            return Err(format!("system icon escapes resource boundary: {}", icon.id).into());
        }
        println!("cargo:rerun-if-changed={}", path.display());
        let data = read_bounded(&path)?;
        let pixels = raster(
            &data,
            path.extension()
                .and_then(|value| value.to_str())
                .unwrap_or(""),
        )
        .map_err(|error| format!("cannot rasterize {}: {error}", icon.id))?;
        let file = format!("system-icon-{}.rgba", icon.id);
        fs::write(output.join(&file), pixels.data())?;
        if assets.insert(icon.id, file).is_some() {
            return Err("duplicate system icon ID".into());
        }
    }
    let mut generated = String::from("&[\n");
    for (id, file) in assets {
        generated.push_str(&format!(
            "({id:?}, include_bytes!(concat!(env!(\"OUT_DIR\"), \"/{file}\")) as &[u8]),\n"
        ));
    }
    generated.push_str("]\n");
    fs::write(output.join("system_icon_pixels.rs"), generated)?;
    Ok(())
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut data = Vec::new();
    fs::File::open(path)?
        .take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut data)?;
    if data.len() > MAX_BYTES {
        return Err(format!("system icon input exceeds 2 MiB: {}", path.display()).into());
    }
    Ok(data)
}

fn transform(width: f32, height: f32) -> tiny_skia::Transform {
    let scale = (SIDE as f32 / width).min(SIDE as f32 / height);
    tiny_skia::Transform::from_scale(scale, scale).post_translate(
        (SIDE as f32 - width * scale) / 2.0,
        (SIDE as f32 - height * scale) / 2.0,
    )
}

fn raster(data: &[u8], extension: &str) -> Result<tiny_skia::Pixmap, Box<dyn Error>> {
    if data.len() > MAX_BYTES {
        return Err("system icon exceeds 2 MiB".into());
    }
    let mut output = tiny_skia::Pixmap::new(SIDE, SIDE).ok_or("cannot allocate icon pixmap")?;
    match extension {
        "svg" => {
            let options = usvg::Options {
                image_href_resolver: usvg::ImageHrefResolver {
                    resolve_data: Box::new(|_, _, _| None),
                    resolve_string: Box::new(|_, _| None),
                },
                ..Default::default()
            };
            let tree = usvg::Tree::from_data(data, &options)?;
            resvg::render(
                &tree,
                transform(tree.size().width(), tree.size().height()),
                &mut output.as_mut(),
            );
        }
        "png" => {
            // Validate the PNG header before its decoder allocates source pixels.
            if data.len() < 24 || &data[..8] != b"\x89PNG\r\n\x1a\n" || &data[12..16] != b"IHDR" {
                return Err("invalid PNG header".into());
            }
            let width = u32::from_be_bytes(data[16..20].try_into()?);
            let height = u32::from_be_bytes(data[20..24].try_into()?);
            if width == 0
                || height == 0
                || width > 8192
                || height > 8192
                || u64::from(width) * u64::from(height) > 32 * 1024 * 1024
            {
                return Err("PNG dimensions exceed bundled resource budget".into());
            }
            let source = tiny_skia::Pixmap::decode_png(data)?;
            let paint = tiny_skia::PixmapPaint {
                quality: tiny_skia::FilterQuality::Bicubic,
                ..Default::default()
            };
            output.draw_pixmap(
                0,
                0,
                source.as_ref(),
                &paint,
                transform(width as f32, height as f32),
                None,
            );
        }
        _ => return Err("unsupported system icon format".into()),
    }
    if !output.pixels().iter().any(|pixel| pixel.alpha() != 0) {
        return Err("system icon produced no visible pixels".into());
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svg_resources_and_oversized_png_headers_are_rejected_before_allocation() {
        for href in [
            "file:///private/never-read.png",
            "https://invalid.example/icon.png",
            "data:image/png;base64,aW52YWxpZA==",
        ] {
            let svg = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="48" height="48"><image href="{href}" width="48" height="48"/></svg>"#
            );
            assert!(raster(svg.as_bytes(), "svg").is_err());
        }
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend_from_slice(&99999_u32.to_be_bytes());
        png.extend_from_slice(&48_u32.to_be_bytes());
        assert!(
            raster(&png, "png")
                .unwrap_err()
                .to_string()
                .contains("dimensions")
        );
        assert!(raster(&vec![0; MAX_BYTES + 1], "svg").is_err());
    }

    #[test]
    fn catalog_generation_rejects_escaped_and_duplicate_resource_records() {
        let directory =
            std::env::temp_dir().join(format!("axssh-build-icons-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let output = directory.join("output");
        fs::create_dir(&output).unwrap();
        fs::write(directory.join("icon.svg"), r#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"><path fill="red" d="M0 0H1V1H0Z"/></svg>"#).unwrap();
        for records in [
            serde_json::json!([{"id":"x", "icon":"../escape.svg"}]),
            serde_json::json!([{"id":"x", "icon":"icon.svg"}, {"id":"x", "icon":"icon.svg"}]),
            serde_json::json!([{"id":"bad/id", "icon":"icon.svg"}]),
            serde_json::json!([]),
        ] {
            fs::write(
                directory.join("catalog.json"),
                serde_json::to_vec(&serde_json::json!({"schema_version":1,"icons":records}))
                    .unwrap(),
            )
            .unwrap();
            assert!(generate(&directory, &output).is_err());
        }
        fs::remove_dir_all(directory).unwrap();
    }
}

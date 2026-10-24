//! Bounded vector icon decoding independent of either frontend.

use super::{FILE_ICON_EDGE, FileIcon};

const MAX_SVG_BYTES: usize = 2 * 1024 * 1024;
const MAX_SVG_EDGE: f32 = 512.0;

pub(super) fn decode(encoded: &[u8]) -> Option<FileIcon> {
    if encoded.is_empty() || encoded.len() > MAX_SVG_BYTES {
        return None;
    }
    // Disable both external resources and embedded images: file-type icons need
    // vector shapes only, never path access, nested SVGs or raster allocations.
    let options = resvg::usvg::Options {
        image_href_resolver: resvg::usvg::ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        ..Default::default()
    };
    let tree = resvg::usvg::Tree::from_str(std::str::from_utf8(encoded).ok()?, &options).ok()?;
    let size = tree.size();
    if size.width() > MAX_SVG_EDGE || size.height() > MAX_SVG_EDGE {
        return None;
    }
    let mut pixmap = resvg::tiny_skia::Pixmap::new(FILE_ICON_EDGE, FILE_ICON_EDGE)?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(
            FILE_ICON_EDGE as f32 / size.width(),
            FILE_ICON_EDGE as f32 / size.height(),
        ),
        &mut pixmap.as_mut(),
    );
    // tiny-skia stores premultiplied pixels; both UI adapters consume straight RGBA.
    let mut rgba = Vec::with_capacity((FILE_ICON_EDGE * FILE_ICON_EDGE * 4) as usize);
    for pixel in pixmap.pixels() {
        let color = pixel.demultiply();
        rgba.extend_from_slice(&[color.red(), color.green(), color.blue(), color.alpha()]);
    }
    FileIcon::from_rgba(FILE_ICON_EDGE, FILE_ICON_EDGE, rgba)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svg_pixels_keep_straight_alpha_and_fixed_dimensions() {
        let icon = decode(br##"<svg xmlns="http://www.w3.org/2000/svg" width="48" height="48"><rect width="48" height="48" fill="#ff0000" opacity="0.5"/></svg>"##).unwrap();
        assert_eq!((icon.width(), icon.height()), (24, 24));
        let pixel = &icon.rgba()[(12 * 24 + 12) * 4..][..4];
        assert_eq!(&pixel[..3], &[255, 0, 0]);
        assert!((127..=128).contains(&pixel[3]));
    }

    #[test]
    fn svg_rejects_bad_input_and_ignores_image_resources() {
        assert!(decode(b"invalid").is_none());
        assert!(decode(&vec![b' '; MAX_SVG_BYTES + 1]).is_none());
        assert!(
            decode(br#"<svg xmlns="http://www.w3.org/2000/svg" width="513" height="24"/>"#)
                .is_none()
        );
        let path =
            std::env::temp_dir().join(format!("axssh-icon-resource-{}.svg", uuid::Uuid::new_v4()));
        let red = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24"><rect width="24" height="24" fill="red"/></svg>"#;
        std::fs::write(&path, red).unwrap();
        let external = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24"><image href="{}" width="24" height="24"/></svg>"#,
            path.display()
        );
        let icon = decode(external.as_bytes()).unwrap();
        std::fs::remove_file(path).unwrap();
        assert!(icon.rgba().iter().all(|byte| *byte == 0));
        use base64::Engine as _;
        let data = base64::engine::general_purpose::STANDARD.encode(red.as_bytes());
        let embedded = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24"><image href="data:image/svg+xml;base64,{data}" width="24" height="24"/></svg>"#
        );
        let icon = decode(embedded.as_bytes()).unwrap();
        assert!(icon.rgba().iter().all(|byte| *byte == 0));
    }
}

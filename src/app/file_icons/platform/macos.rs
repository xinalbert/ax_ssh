use objc2::available;
use objc2::rc::autoreleasepool;
use objc2::runtime::NSObjectProtocol;
use objc2::{AnyThread, sel};
use objc2_app_kit::{
    NSBitmapImageFileType, NSBitmapImageRep, NSBitmapImageRepPropertyKey, NSImage, NSWorkspace,
};
use objc2_foundation::{NSDictionary, NSPoint, NSRect, NSSize, NSString};
use objc2_uniform_type_identifiers::UTType;

use super::*;

#[derive(Default)]
pub(in crate::app::file_icons) struct Resolver {
    _private: (),
}

impl IconResolver for Resolver {
    fn cache_identity(&self) -> CacheIdentity {
        if available!(macos = 11.0) {
            CacheIdentity::new("macos-appkit-uttype-v1")
        } else {
            CacheIdentity::new("macos-appkit-filetype-v1")
        }
    }

    fn resolve(&self, key: &FileIconKey) -> Option<FileIcon> {
        autoreleasepool(|_| {
            let workspace = NSWorkspace::sharedWorkspace();
            let image =
                modern_icon(&workspace, key).unwrap_or_else(|| legacy_icon(&workspace, key));
            let mut proposed_rect = NSRect::new(
                NSPoint::new(0.0, 0.0),
                NSSize::new(FILE_ICON_EDGE as f64, FILE_ICON_EDGE as f64),
            );
            // SAFETY: `proposed_rect` is a valid writable rect and the optional context and
            // hints are deliberately null; AppKit chooses the best bounded representation.
            let cg_image = unsafe {
                image.CGImageForProposedRect_context_hints(&mut proposed_rect, None, None)
            }?;
            let bitmap_rep =
                NSBitmapImageRep::initWithCGImage(NSBitmapImageRep::alloc(), &cg_image);
            let properties =
                NSDictionary::<NSBitmapImageRepPropertyKey, objc2::runtime::AnyObject>::new();
            // SAFETY: `bitmap_rep` is a valid AppKit bitmap representation and the empty
            // properties dictionary is the documented default for PNG encoding.
            let encoded = unsafe {
                bitmap_rep
                    .representationUsingType_properties(NSBitmapImageFileType::PNG, &properties)
            }?;
            if encoded.length() > MAX_ENCODED_ICON_BYTES {
                return None;
            }
            // SAFETY: `encoded` remains retained while the borrowed bytes are decoded, and
            // NSData promises a stable contiguous region for its immutable lifetime.
            let bytes = unsafe { encoded.as_bytes_unchecked() };
            decode_encoded_icon(bytes)
        })
    }
}

fn modern_icon(workspace: &NSWorkspace, key: &FileIconKey) -> Option<objc2::rc::Retained<NSImage>> {
    if !available!(macos = 11.0) || !workspace.respondsToSelector(sel!(iconForContentType:)) {
        return None;
    }
    let content_type = match key {
        FileIconKey::Folder => UTType::typeWithIdentifier(&NSString::from_str("public.folder")),
        FileIconKey::Symlink => UTType::typeWithIdentifier(&NSString::from_str("public.symlink")),
        FileIconKey::GenericFile => UTType::typeWithIdentifier(&NSString::from_str("public.data")),
        FileIconKey::Extension(extension) => {
            UTType::typeWithFilenameExtension(&NSString::from_str(extension))
        }
    }?;
    Some(workspace.iconForContentType(&content_type))
}

#[allow(deprecated)]
fn legacy_icon(workspace: &NSWorkspace, key: &FileIconKey) -> objc2::rc::Retained<NSImage> {
    let file_type = match key {
        FileIconKey::Folder => "public.folder",
        FileIconKey::Symlink => "public.symlink",
        FileIconKey::GenericFile => "public.data",
        FileIconKey::Extension(extension) => extension,
    };
    workspace.iconForFileType(&NSString::from_str(file_type))
}

//! Bounded platform file-icon lookup for SFTP and local directory rows.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex, OnceLock};
use tokio::runtime::Handle;

pub(super) const FILE_ICON_EDGE: u32 = 24;

const FILE_ICON_BYTE_LEN: usize = FILE_ICON_EDGE as usize * FILE_ICON_EDGE as usize * 4;
const ICON_CACHE_CAPACITY: usize = 128;
const PREWARM_KEY_LIMIT: usize = 64;
const EXTENSION_BYTE_LIMIT: usize = 24;
const IDENTITY_CHARACTER_LIMIT: usize = 96;
#[cfg(any(target_os = "linux", target_os = "macos"))]
const MAX_ENCODED_ICON_BYTES: usize = 2 * 1024 * 1024;
#[cfg(any(target_os = "linux", target_os = "macos"))]
const MAX_SOURCE_ICON_EDGE: u32 = 512;
#[cfg(any(target_os = "linux", target_os = "macos"))]
const MAX_DECODE_ALLOCATION: u64 = 8 * 1024 * 1024;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) enum FileIconKey {
    Folder,
    Symlink,
    GenericFile,
    Extension(String),
}

impl FileIconKey {
    pub(super) fn for_entry(name: &str, is_dir: bool, is_symlink: bool) -> Self {
        if is_symlink {
            return Self::Symlink;
        }
        if is_dir {
            return Self::Folder;
        }
        normalized_extension(name).map_or(Self::GenericFile, Self::Extension)
    }

    fn is_pinned(&self) -> bool {
        !matches!(self, Self::Extension(_))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct FileIcon {
    width: u32,
    height: u32,
    rgba: Arc<[u8]>,
}

impl FileIcon {
    pub(super) fn width(&self) -> u32 {
        self.width
    }

    pub(super) fn height(&self) -> u32 {
        self.height
    }

    pub(super) fn rgba(&self) -> &[u8] {
        &self.rgba
    }

    fn from_rgba(width: u32, height: u32, rgba: Vec<u8>) -> Option<Self> {
        let expected = width
            .checked_mul(height)?
            .checked_mul(4)
            .and_then(|bytes| usize::try_from(bytes).ok())?;
        if width == 0
            || height == 0
            || width > FILE_ICON_EDGE
            || height > FILE_ICON_EDGE
            || rgba.len() != expected
            || rgba.len() > FILE_ICON_BYTE_LEN
        {
            return None;
        }
        Some(Self {
            width,
            height,
            rgba: rgba.into(),
        })
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct IconPrewarmReport {
    pub(super) requested: usize,
    pub(super) system_resolved: usize,
    pub(super) fallback_used: usize,
    pub(super) already_cached: usize,
    pub(super) cache_entries: usize,
    pub(super) identity_changed: bool,
    pub(super) truncated: bool,
    pub(super) cache_available: bool,
}

pub(super) struct FileIconProvider {
    resolver: Arc<dyn IconResolver>,
    fallbacks: FallbackIcons,
    cache: Mutex<IconCache>,
    prewarm_gate: Mutex<()>,
}

static GLOBAL_FILE_ICON_PROVIDER: OnceLock<Arc<FileIconProvider>> = OnceLock::new();

pub(super) fn global_provider() -> Arc<FileIconProvider> {
    GLOBAL_FILE_ICON_PROVIDER
        .get_or_init(|| Arc::new(FileIconProvider::new()))
        .clone()
}

pub(super) fn prewarm_async(
    runtime: &Handle,
    keys: Vec<FileIconKey>,
) -> tokio::task::JoinHandle<IconPrewarmReport> {
    let provider = global_provider();
    runtime.spawn_blocking(move || provider.prewarm(keys))
}

impl FileIconProvider {
    pub(super) fn new() -> Self {
        Self::with_boxed_resolver(Arc::new(platform::Resolver::default()))
    }

    /// Return a cached icon without invoking platform APIs or reading the filesystem.
    pub(super) fn cached_icon(&self, key: &FileIconKey) -> FileIcon {
        let Ok(mut cache) = self.cache.lock() else {
            return self.fallbacks.for_key(key);
        };
        if let Some(icon) = cache.icon(key) {
            return icon;
        }
        self.fallbacks.for_key(key)
    }

    /// Resolve at most one bounded batch. Call this from a blocking worker, never the UI thread.
    pub(super) fn prewarm<I>(&self, keys: I) -> IconPrewarmReport
    where
        I: IntoIterator<Item = FileIconKey>,
    {
        let Ok(_gate) = self.prewarm_gate.lock() else {
            return IconPrewarmReport::default();
        };
        let (keys, truncated) = bounded_unique_keys(keys);
        let identity = self.resolver.cache_identity();
        let mut report = IconPrewarmReport {
            requested: keys.len(),
            truncated,
            cache_available: true,
            ..IconPrewarmReport::default()
        };

        {
            let Ok(mut cache) = self.cache.lock() else {
                report.cache_available = false;
                return report;
            };
            if cache.identity != identity {
                *cache = IconCache::new(identity.clone(), &self.fallbacks);
                report.identity_changed = true;
            }
        }

        for key in keys {
            let already_cached = match self.cache.lock() {
                Ok(mut cache) => cache.was_attempted(&key),
                Err(_) => {
                    report.cache_available = false;
                    break;
                }
            };
            if already_cached {
                report.already_cached += 1;
                continue;
            }

            let resolved = self.resolver.resolve(&key);
            let system_resolved = resolved.is_some();
            match self.cache.lock() {
                Ok(mut cache) if cache.identity == identity => {
                    cache.store(key, resolved, &self.fallbacks);
                    if system_resolved {
                        report.system_resolved += 1;
                    } else {
                        report.fallback_used += 1;
                    }
                }
                Ok(_) => {
                    report.cache_available = false;
                    break;
                }
                Err(_) => {
                    report.cache_available = false;
                    break;
                }
            }
        }

        report.cache_entries = self.cache.lock().map_or(0, |cache| cache.icons.len());
        report
    }

    fn with_boxed_resolver(resolver: Arc<dyn IconResolver>) -> Self {
        let fallbacks = FallbackIcons::new();
        Self {
            resolver,
            cache: Mutex::new(IconCache::new(
                CacheIdentity::new("uninitialized"),
                &fallbacks,
            )),
            fallbacks,
            prewarm_gate: Mutex::new(()),
        }
    }

    #[cfg(test)]
    fn with_resolver(resolver: Arc<dyn IconResolver>) -> Self {
        Self::with_boxed_resolver(resolver)
    }

    #[cfg(test)]
    fn cache_len(&self) -> usize {
        self.cache.lock().map_or(0, |cache| cache.icons.len())
    }
}

trait IconResolver: Send + Sync {
    fn cache_identity(&self) -> CacheIdentity;
    fn resolve(&self, key: &FileIconKey) -> Option<FileIcon>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CacheIdentity(String);

impl CacheIdentity {
    fn new(value: &str) -> Self {
        let value = value
            .chars()
            .filter(|character| !character.is_control())
            .take(IDENTITY_CHARACTER_LIMIT)
            .collect();
        Self(value)
    }
}

struct IconCache {
    identity: CacheIdentity,
    icons: HashMap<FileIconKey, FileIcon>,
    attempted: HashSet<FileIconKey>,
    extension_lru: VecDeque<FileIconKey>,
}

impl IconCache {
    fn new(identity: CacheIdentity, fallbacks: &FallbackIcons) -> Self {
        let icons = HashMap::from([
            (FileIconKey::Folder, fallbacks.folder.clone()),
            (FileIconKey::Symlink, fallbacks.symlink.clone()),
            (FileIconKey::GenericFile, fallbacks.generic.clone()),
        ]);
        Self {
            identity,
            icons,
            attempted: HashSet::new(),
            extension_lru: VecDeque::new(),
        }
    }

    fn icon(&mut self, key: &FileIconKey) -> Option<FileIcon> {
        let icon = self.icons.get(key).cloned();
        if icon.is_some() {
            self.touch(key);
        }
        icon
    }

    fn was_attempted(&mut self, key: &FileIconKey) -> bool {
        let attempted = self.attempted.contains(key);
        if attempted {
            self.touch(key);
        }
        attempted
    }

    fn store(&mut self, key: FileIconKey, resolved: Option<FileIcon>, fallbacks: &FallbackIcons) {
        if !self.icons.contains_key(&key) {
            while self.icons.len() >= ICON_CACHE_CAPACITY {
                let Some(evicted) = self.extension_lru.pop_front() else {
                    return;
                };
                self.icons.remove(&evicted);
                self.attempted.remove(&evicted);
            }
        }
        let icon = resolved.unwrap_or_else(|| fallbacks.for_key(&key));
        self.icons.insert(key.clone(), icon);
        self.attempted.insert(key.clone());
        self.touch(&key);
    }

    fn touch(&mut self, key: &FileIconKey) {
        if key.is_pinned() {
            return;
        }
        self.extension_lru.retain(|cached| cached != key);
        self.extension_lru.push_back(key.clone());
    }
}

#[derive(Clone)]
struct FallbackIcons {
    folder: FileIcon,
    symlink: FileIcon,
    generic: FileIcon,
}

impl FallbackIcons {
    fn new() -> Self {
        Self {
            folder: fallback_folder_icon(),
            symlink: fallback_symlink_icon(),
            generic: fallback_file_icon(),
        }
    }

    fn for_key(&self, key: &FileIconKey) -> FileIcon {
        match key {
            FileIconKey::Folder => self.folder.clone(),
            FileIconKey::Symlink => self.symlink.clone(),
            FileIconKey::GenericFile | FileIconKey::Extension(_) => self.generic.clone(),
        }
    }
}

fn normalized_extension(name: &str) -> Option<String> {
    let name = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let (stem, extension) = name.rsplit_once('.')?;
    if stem.is_empty()
        || extension.is_empty()
        || extension.len() > EXTENSION_BYTE_LIMIT
        || !extension
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'_'))
        || !extension.bytes().any(|byte| byte.is_ascii_alphanumeric())
    {
        return None;
    }
    Some(extension.to_ascii_lowercase())
}

fn bounded_unique_keys<I>(keys: I) -> (Vec<FileIconKey>, bool)
where
    I: IntoIterator<Item = FileIconKey>,
{
    let mut unique = HashSet::with_capacity(PREWARM_KEY_LIMIT);
    let mut bounded = Vec::with_capacity(PREWARM_KEY_LIMIT);
    let mut truncated = false;
    for key in keys {
        if unique.contains(&key) {
            continue;
        }
        if bounded.len() == PREWARM_KEY_LIMIT {
            truncated = true;
            break;
        }
        unique.insert(key.clone());
        bounded.push(key);
    }
    (bounded, truncated)
}

fn fallback_file_icon() -> FileIcon {
    let mut pixels = transparent_pixels();
    fill_rect(&mut pixels, 5, 2, 18, 21, [236, 240, 244, 255]);
    stroke_rect(&mut pixels, 5, 2, 18, 21, [92, 103, 115, 255]);
    fill_rect(&mut pixels, 14, 2, 18, 7, [198, 208, 217, 255]);
    draw_line(&mut pixels, 14, 2, 18, 6, [92, 103, 115, 255]);
    fill_rect(&mut pixels, 8, 11, 15, 12, [137, 151, 164, 255]);
    fill_rect(&mut pixels, 8, 15, 15, 16, [137, 151, 164, 255]);
    icon_from_fallback_pixels(pixels)
}

fn fallback_folder_icon() -> FileIcon {
    let mut pixels = transparent_pixels();
    fill_rect(&mut pixels, 2, 7, 21, 20, [235, 177, 65, 255]);
    fill_rect(&mut pixels, 4, 4, 11, 8, [246, 196, 83, 255]);
    stroke_rect(&mut pixels, 2, 7, 21, 20, [142, 99, 28, 255]);
    fill_rect(&mut pixels, 3, 10, 20, 19, [247, 195, 78, 255]);
    icon_from_fallback_pixels(pixels)
}

fn fallback_symlink_icon() -> FileIcon {
    let mut pixels = fallback_file_icon().rgba().to_vec();
    fill_rect(&mut pixels, 2, 14, 8, 20, [255, 255, 255, 235]);
    stroke_rect(&mut pixels, 2, 14, 8, 20, [32, 126, 145, 255]);
    draw_line(&mut pixels, 5, 17, 11, 17, [32, 126, 145, 255]);
    draw_line(&mut pixels, 9, 15, 11, 17, [32, 126, 145, 255]);
    draw_line(&mut pixels, 9, 19, 11, 17, [32, 126, 145, 255]);
    icon_from_fallback_pixels(pixels)
}

fn transparent_pixels() -> Vec<u8> {
    vec![0; FILE_ICON_BYTE_LEN]
}

fn icon_from_fallback_pixels(pixels: Vec<u8>) -> FileIcon {
    FileIcon {
        width: FILE_ICON_EDGE,
        height: FILE_ICON_EDGE,
        rgba: pixels.into(),
    }
}

fn fill_rect(pixels: &mut [u8], left: u32, top: u32, right: u32, bottom: u32, color: [u8; 4]) {
    for y in top..=bottom {
        for x in left..=right {
            set_pixel(pixels, x, y, color);
        }
    }
}

fn stroke_rect(pixels: &mut [u8], left: u32, top: u32, right: u32, bottom: u32, color: [u8; 4]) {
    for x in left..=right {
        set_pixel(pixels, x, top, color);
        set_pixel(pixels, x, bottom, color);
    }
    for y in top..=bottom {
        set_pixel(pixels, left, y, color);
        set_pixel(pixels, right, y, color);
    }
}

fn draw_line(pixels: &mut [u8], mut x0: u32, mut y0: u32, x1: u32, y1: u32, color: [u8; 4]) {
    while x0 != x1 || y0 != y1 {
        set_pixel(pixels, x0, y0, color);
        if x0 < x1 {
            x0 += 1;
        } else if x0 > x1 {
            x0 -= 1;
        }
        if y0 < y1 {
            y0 += 1;
        } else if y0 > y1 {
            y0 -= 1;
        }
    }
    set_pixel(pixels, x1, y1, color);
}

fn set_pixel(pixels: &mut [u8], x: u32, y: u32, color: [u8; 4]) {
    if x >= FILE_ICON_EDGE || y >= FILE_ICON_EDGE {
        return;
    }
    let offset = ((y * FILE_ICON_EDGE + x) * 4) as usize;
    pixels[offset..offset + 4].copy_from_slice(&color);
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn decode_encoded_icon(encoded: &[u8]) -> Option<FileIcon> {
    use std::io::Cursor;

    if encoded.is_empty() || encoded.len() > MAX_ENCODED_ICON_BYTES {
        return None;
    }
    let mut reader = image::ImageReader::new(Cursor::new(encoded))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SOURCE_ICON_EDGE);
    limits.max_image_height = Some(MAX_SOURCE_ICON_EDGE);
    limits.max_alloc = Some(MAX_DECODE_ALLOCATION);
    reader.limits(limits);
    let decoded = reader.decode().ok()?;
    let rgba = decoded
        .resize_exact(
            FILE_ICON_EDGE,
            FILE_ICON_EDGE,
            image::imageops::FilterType::Triangle,
        )
        .into_rgba8()
        .into_raw();
    FileIcon::from_rgba(FILE_ICON_EDGE, FILE_ICON_EDGE, rgba)
}

#[cfg(target_os = "macos")]
mod platform {
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
    pub(super) struct Resolver {
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

    fn modern_icon(
        workspace: &NSWorkspace,
        key: &FileIconKey,
    ) -> Option<objc2::rc::Retained<NSImage>> {
        if !available!(macos = 11.0) || !workspace.respondsToSelector(sel!(iconForContentType:)) {
            return None;
        }
        let content_type = match key {
            FileIconKey::Folder => UTType::typeWithIdentifier(&NSString::from_str("public.folder")),
            FileIconKey::Symlink => {
                UTType::typeWithIdentifier(&NSString::from_str("public.symlink"))
            }
            FileIconKey::GenericFile => {
                UTType::typeWithIdentifier(&NSString::from_str("public.data"))
            }
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
}

#[cfg(windows)]
mod platform {
    use std::ffi::c_void;
    use std::mem::size_of;
    use std::ptr::null_mut;

    use windows_sys::Win32::Foundation::RPC_E_CHANGED_MODE;
    use windows_sys::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS,
        DeleteDC, DeleteObject, HBITMAP, HDC, HGDIOBJ, SelectObject,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL,
    };
    use windows_sys::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
    use windows_sys::Win32::UI::Shell::{
        SHFILEINFOW, SHGFI_ICON, SHGFI_LINKOVERLAY, SHGFI_SMALLICON, SHGFI_USEFILEATTRIBUTES,
        SHGetFileInfoW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{DI_NORMAL, DestroyIcon, DrawIconEx, HICON};

    use super::*;

    #[derive(Default)]
    pub(super) struct Resolver {
        _private: (),
    }

    impl IconResolver for Resolver {
        fn cache_identity(&self) -> CacheIdentity {
            CacheIdentity::new("windows-shell-small-icon-v1")
        }

        fn resolve(&self, key: &FileIconKey) -> Option<FileIcon> {
            let _com = ComGuard::initialize()?;
            let (name, attributes, extra_flags) = match key {
                FileIconKey::Folder => ("folder".to_owned(), FILE_ATTRIBUTE_DIRECTORY, 0),
                FileIconKey::Symlink => (
                    "remote-file".to_owned(),
                    FILE_ATTRIBUTE_NORMAL,
                    SHGFI_LINKOVERLAY,
                ),
                FileIconKey::GenericFile => ("remote-file".to_owned(), FILE_ATTRIBUTE_NORMAL, 0),
                FileIconKey::Extension(extension) => {
                    (format!("remote-file.{extension}"), FILE_ATTRIBUTE_NORMAL, 0)
                }
            };
            let wide_name = name.encode_utf16().chain([0]).collect::<Vec<_>>();
            let mut info = SHFILEINFOW::default();
            // SAFETY: `wide_name` is NUL-terminated, `info` is writable for its declared size,
            // and SHGFI_USEFILEATTRIBUTES ensures the synthetic path is not accessed.
            let result = unsafe {
                SHGetFileInfoW(
                    wide_name.as_ptr(),
                    attributes,
                    &mut info,
                    size_of::<SHFILEINFOW>() as u32,
                    SHGFI_ICON | SHGFI_SMALLICON | SHGFI_USEFILEATTRIBUTES | extra_flags,
                )
            };
            if result == 0 || info.hIcon.is_null() {
                return None;
            }
            let icon = OwnedIcon(info.hIcon);
            icon_to_rgba(icon.0)
        }
    }

    struct ComGuard {
        uninitialize: bool,
    }

    impl ComGuard {
        fn initialize() -> Option<Self> {
            // SAFETY: this call only changes COM state for the current blocking worker thread; a
            // null reserved pointer and the MTA flag are the documented initialization contract.
            let result = unsafe { CoInitializeEx(null_mut(), COINIT_MULTITHREADED as u32) };
            if result >= 0 {
                Some(Self { uninitialize: true })
            } else if result == RPC_E_CHANGED_MODE {
                // The thread is already initialized under another COM apartment model.
                // Do not call CoUninitialize because this call did not increment its COM count.
                Some(Self {
                    uninitialize: false,
                })
            } else {
                None
            }
        }
    }

    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.uninitialize {
                // SAFETY: `uninitialize` is true only when this guard successfully incremented the
                // current thread's COM initialization count.
                unsafe { CoUninitialize() };
            }
        }
    }

    struct OwnedIcon(HICON);

    impl Drop for OwnedIcon {
        fn drop(&mut self) {
            // SAFETY: this wrapper uniquely owns the HICON returned by SHGetFileInfoW.
            unsafe { DestroyIcon(self.0) };
        }
    }

    struct MemoryDc(HDC);

    impl Drop for MemoryDc {
        fn drop(&mut self) {
            // SAFETY: this wrapper uniquely owns the memory DC returned by CreateCompatibleDC.
            unsafe { DeleteDC(self.0) };
        }
    }

    struct OwnedBitmap(HBITMAP);

    impl Drop for OwnedBitmap {
        fn drop(&mut self) {
            if self.0.is_null() {
                return;
            }
            // SAFETY: this wrapper uniquely owns the HBITMAP returned by CreateDIBSection.
            unsafe { DeleteObject(self.0) };
        }
    }

    fn select_failed(object: HGDIOBJ) -> bool {
        object.is_null() || object == (-1_isize as HGDIOBJ)
    }

    fn icon_to_rgba(icon: HICON) -> Option<FileIcon> {
        // SAFETY: a null compatible HDC requests a memory DC compatible with the screen.
        let dc = MemoryDc(unsafe { CreateCompatibleDC(null_mut()) });
        if dc.0.is_null() {
            return None;
        }

        let bitmap_info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: FILE_ICON_EDGE as i32,
                biHeight: -(FILE_ICON_EDGE as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                biSizeImage: FILE_ICON_BYTE_LEN as u32,
                ..BITMAPINFOHEADER::default()
            },
            ..BITMAPINFO::default()
        };
        let mut bits: *mut c_void = null_mut();
        // SAFETY: `bitmap_info` describes a bounded 24x24 32-bit DIB and `bits` is a valid out
        // pointer. No file mapping is supplied, so GDI allocates the backing storage.
        let bitmap = OwnedBitmap(unsafe {
            CreateDIBSection(dc.0, &bitmap_info, DIB_RGB_COLORS, &mut bits, null_mut(), 0)
        });
        if bitmap.0.is_null() || bits.is_null() {
            return None;
        }
        // SAFETY: the DIB allocation is exactly FILE_ICON_BYTE_LEN bytes and is exclusively owned
        // here. Clearing it gives DrawIconEx a deterministic transparent destination.
        unsafe { std::ptr::write_bytes(bits.cast::<u8>(), 0, FILE_ICON_BYTE_LEN) };

        // SAFETY: both handles are live GDI objects owned by this scope.
        let previous = unsafe { SelectObject(dc.0, bitmap.0 as HGDIOBJ) };
        if select_failed(previous) {
            return None;
        }
        // SAFETY: the selected DIB is exactly FILE_ICON_EDGE square and `icon` is live for the
        // duration of the call.
        let drawn = unsafe {
            DrawIconEx(
                dc.0,
                0,
                0,
                icon,
                FILE_ICON_EDGE as i32,
                FILE_ICON_EDGE as i32,
                0,
                null_mut(),
                DI_NORMAL,
            )
        };
        // SAFETY: `previous` is the object returned by SelectObject for this same DC.
        let restored = unsafe { SelectObject(dc.0, previous) };
        if select_failed(restored) {
            // A failed restore leaves ownership of the bitmap ambiguous: it may still be selected
            // into the DC. Leaking this tiny bounded GDI object is safer than allowing Drop to call
            // DeleteObject on a selected bitmap, which violates the GDI lifetime contract.
            std::mem::forget(bitmap);
            return None;
        }
        if drawn == 0 {
            return None;
        }

        // SAFETY: CreateDIBSection returned a buffer of FILE_ICON_BYTE_LEN bytes that remains
        // valid until `bitmap` is dropped below. It is copied before either GDI owner is released.
        let bgra = unsafe { std::slice::from_raw_parts(bits.cast::<u8>(), FILE_ICON_BYTE_LEN) };
        if !bgra.chunks_exact(4).any(|pixel| pixel[3] != 0) {
            return None;
        }
        let mut rgba = Vec::with_capacity(FILE_ICON_BYTE_LEN);
        for pixel in bgra.chunks_exact(4) {
            let alpha = pixel[3];
            rgba.push(unpremultiply(pixel[2], alpha));
            rgba.push(unpremultiply(pixel[1], alpha));
            rgba.push(unpremultiply(pixel[0], alpha));
            rgba.push(alpha);
        }
        FileIcon::from_rgba(FILE_ICON_EDGE, FILE_ICON_EDGE, rgba)
    }

    fn unpremultiply(channel: u8, alpha: u8) -> u8 {
        if alpha == 0 {
            return 0;
        }
        ((u32::from(channel) * 255 + u32::from(alpha) / 2) / u32::from(alpha)).min(255) as u8
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use std::fs::File;
    use std::io::Read;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    use super::*;

    const THEME_QUERY_TIMEOUT: Duration = Duration::from_millis(300);
    const THEME_OUTPUT_LIMIT: u64 = 256;
    const THEME_NAME_LIMIT: usize = 64;

    pub(super) struct Resolver {
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
        let source = image::RgbaImage::from_raw(
            source.width(),
            source.height(),
            source.as_bytes().to_vec(),
        )?;
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
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
mod platform {
    use super::*;

    #[derive(Default)]
    pub(super) struct Resolver {
        _private: (),
    }

    impl IconResolver for Resolver {
        fn cache_identity(&self) -> CacheIdentity {
            CacheIdentity::new("unsupported-platform-v1")
        }

        fn resolve(&self, _key: &FileIconKey) -> Option<FileIcon> {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    struct TestResolver {
        identity: AtomicUsize,
        calls: AtomicUsize,
        fail: bool,
    }

    impl TestResolver {
        fn new(fail: bool) -> Self {
            Self {
                identity: AtomicUsize::new(1),
                calls: AtomicUsize::new(0),
                fail,
            }
        }
    }

    impl IconResolver for TestResolver {
        fn cache_identity(&self) -> CacheIdentity {
            CacheIdentity::new(&format!("test-{}", self.identity.load(Ordering::SeqCst)))
        }

        fn resolve(&self, _key: &FileIconKey) -> Option<FileIcon> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                return None;
            }
            let color = self.identity.load(Ordering::SeqCst) as u8;
            FileIcon::from_rgba(
                FILE_ICON_EDGE,
                FILE_ICON_EDGE,
                vec![color; FILE_ICON_BYTE_LEN],
            )
        }
    }

    #[test]
    fn entry_keys_are_normalized_and_bounded() {
        assert_eq!(
            FileIconKey::for_entry("folder.txt", true, false),
            FileIconKey::Folder
        );
        assert_eq!(
            FileIconKey::for_entry("folder", true, true),
            FileIconKey::Symlink
        );
        assert_eq!(
            FileIconKey::for_entry("ARCHIVE.TAR.GZ", false, false),
            FileIconKey::Extension("gz".to_owned())
        );
        assert_eq!(
            FileIconKey::for_entry("/remote/.config.JSON", false, false),
            FileIconKey::Extension("json".to_owned())
        );
        assert_eq!(
            FileIconKey::for_entry(".gitignore", false, false),
            FileIconKey::GenericFile
        );
        assert_eq!(
            FileIconKey::for_entry("unsafe.ext!", false, false),
            FileIconKey::GenericFile
        );
        assert_eq!(
            FileIconKey::for_entry("too-long.abcdefghijklmnopqrstuvwxyz", false, false),
            FileIconKey::GenericFile
        );
    }

    #[test]
    fn owned_rgba_rejects_invalid_dimensions_and_lengths() {
        assert!(FileIcon::from_rgba(0, FILE_ICON_EDGE, Vec::new()).is_none());
        assert!(
            FileIcon::from_rgba(
                FILE_ICON_EDGE + 1,
                FILE_ICON_EDGE,
                vec![0; FILE_ICON_BYTE_LEN]
            )
            .is_none()
        );
        assert!(FileIcon::from_rgba(FILE_ICON_EDGE, FILE_ICON_EDGE, vec![0; 4]).is_none());
        let icon = FileIcon::from_rgba(FILE_ICON_EDGE, FILE_ICON_EDGE, vec![7; FILE_ICON_BYTE_LEN])
            .expect("bounded RGBA icon should be accepted");
        assert_eq!(icon.rgba().len(), FILE_ICON_BYTE_LEN);
    }

    #[test]
    fn cached_lookup_never_invokes_the_resolver() {
        let resolver = Arc::new(TestResolver::new(false));
        let provider = FileIconProvider::with_resolver(resolver.clone());
        let generic = provider.cached_icon(&FileIconKey::GenericFile);
        let missing = provider.cached_icon(&FileIconKey::Extension("rs".to_owned()));

        assert_eq!(missing, generic);
        assert_eq!(resolver.calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn prewarm_deduplicates_and_limits_each_batch() {
        let resolver = Arc::new(TestResolver::new(false));
        let provider = FileIconProvider::with_resolver(resolver.clone());
        let keys = (0..100)
            .flat_map(|index| {
                let key = FileIconKey::Extension(format!("x{index}"));
                [key.clone(), key]
            })
            .collect::<Vec<_>>();

        let report = provider.prewarm(keys);

        assert_eq!(report.requested, PREWARM_KEY_LIMIT);
        assert!(report.truncated);
        assert_eq!(report.system_resolved, PREWARM_KEY_LIMIT);
        assert_eq!(resolver.calls.load(Ordering::SeqCst), PREWARM_KEY_LIMIT);
        assert!(provider.cache_len() <= ICON_CACHE_CAPACITY);
    }

    #[test]
    fn cache_capacity_evicts_old_extensions() {
        let resolver = Arc::new(TestResolver::new(false));
        let provider = FileIconProvider::with_resolver(resolver);
        for batch in 0..3 {
            provider.prewarm(
                (0..PREWARM_KEY_LIMIT)
                    .map(|index| FileIconKey::Extension(format!("b{batch}x{index}"))),
            );
        }

        assert_eq!(provider.cache_len(), ICON_CACHE_CAPACITY);
        assert_eq!(
            provider.cached_icon(&FileIconKey::Folder),
            provider.fallbacks.folder
        );
    }

    #[test]
    fn identity_change_invalidates_resolved_entries() {
        let resolver = Arc::new(TestResolver::new(false));
        let provider = FileIconProvider::with_resolver(resolver.clone());
        let key = FileIconKey::Extension("rs".to_owned());
        let first = provider.prewarm([key.clone()]);
        let first_icon = provider.cached_icon(&key);
        resolver.identity.store(2, Ordering::SeqCst);

        let second = provider.prewarm([key.clone()]);
        let second_icon = provider.cached_icon(&key);

        assert!(first.identity_changed);
        assert!(second.identity_changed);
        assert_ne!(first_icon, second_icon);
        assert_eq!(resolver.calls.load(Ordering::SeqCst), 2);
        assert_eq!(provider.cache_len(), 4);
    }

    #[test]
    fn failed_resolution_is_cached_as_deterministic_fallback() {
        let resolver = Arc::new(TestResolver::new(true));
        let provider = FileIconProvider::with_resolver(resolver.clone());
        let key = FileIconKey::Extension("unknown".to_owned());

        let first = provider.prewarm([key.clone()]);
        let second = provider.prewarm([key.clone()]);

        assert_eq!(first.fallback_used, 1);
        assert_eq!(second.already_cached, 1);
        assert_eq!(resolver.calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            provider.cached_icon(&key),
            provider.cached_icon(&FileIconKey::GenericFile)
        );
    }

    #[test]
    fn fallback_icons_are_fixed_size_and_semantically_distinct() {
        let fallbacks = FallbackIcons::new();
        for icon in [&fallbacks.folder, &fallbacks.symlink, &fallbacks.generic] {
            assert_eq!(icon.width(), FILE_ICON_EDGE);
            assert_eq!(icon.height(), FILE_ICON_EDGE);
            assert_eq!(icon.rgba().len(), FILE_ICON_BYTE_LEN);
        }
        assert_ne!(fallbacks.folder, fallbacks.generic);
        assert_ne!(fallbacks.symlink, fallbacks.generic);
    }
}

//! Bounded platform file-icon lookup for SFTP and local directory rows.

mod platform;

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

pub(super) fn clear_global_cache() {
    if let Some(provider) = GLOBAL_FILE_ICON_PROVIDER.get() {
        provider.clear_cache();
    }
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

    pub(super) fn clear_cache(&self) {
        if let Ok(mut cache) = self.cache.lock() {
            *cache = IconCache::new(CacheIdentity::new("uninitialized"), &self.fallbacks);
        }
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
    fn clearing_cache_releases_resolved_extensions_but_keeps_fallbacks() {
        let resolver = Arc::new(TestResolver::new(false));
        let provider = FileIconProvider::with_resolver(resolver);
        provider.prewarm([
            FileIconKey::Extension("rs".to_owned()),
            FileIconKey::Extension("json".to_owned()),
        ]);

        provider.clear_cache();

        assert_eq!(provider.cache_len(), 3);
        assert_eq!(
            provider.cached_icon(&FileIconKey::Extension("rs".to_owned())),
            provider.cached_icon(&FileIconKey::GenericFile)
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

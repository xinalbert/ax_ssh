#![doc = include_str!("../README.md")]
#![allow(clippy::needless_doctest_main)]
#![deny(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate core;

mod backend_dispatch;
use backend_dispatch::*;
mod backend_interface;
use backend_interface::*;
mod backends;
mod error;
mod util;

use std::cell::Cell;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::num::NonZeroU32;
use std::ops;
use std::sync::{Arc, OnceLock, RwLock};

#[cfg(target_vendor = "apple")]
use std::sync::atomic::{AtomicBool, Ordering};

use error::InitError;
pub use error::SoftBufferError;

use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};

#[cfg(target_family = "wasm")]
pub use backends::web::SurfaceExtWeb;

/// An instance of this struct contains the platform-specific data that must be managed in order to
/// write to a window on that platform.
#[derive(Clone, Debug)]
pub struct Context<D> {
    /// The inner static dispatch object.
    context_impl: ContextDispatch<D>,

    /// This is Send+Sync IFF D is Send+Sync.
    _marker: PhantomData<Arc<D>>,
}

impl<D: HasDisplayHandle> Context<D> {
    /// Creates a new instance of this struct, using the provided display.
    pub fn new(display: D) -> Result<Self, SoftBufferError> {
        match ContextDispatch::new(display) {
            Ok(context_impl) => Ok(Self {
                context_impl,
                _marker: PhantomData,
            }),
            Err(InitError::Unsupported(display)) => {
                let raw = display.display_handle()?.as_raw();
                Err(SoftBufferError::UnsupportedDisplayPlatform {
                    human_readable_display_platform_name: display_handle_type_name(&raw),
                    display_handle: raw,
                })
            }
            Err(InitError::Failure(f)) => Err(f),
        }
    }
}

/// A rectangular region of the buffer coordinate space.
#[derive(Clone, Copy, Debug)]
pub struct Rect {
    /// x coordinate of top left corner
    pub x: u32,
    /// y coordinate of top left corner
    pub y: u32,
    /// width
    pub width: NonZeroU32,
    /// height
    pub height: NonZeroU32,
}

/// Describes how a software surface consumes damage rectangles.
///
/// A surface always accepts [`Buffer::present_with_damage`]. This value lets a
/// caller decide whether producing a detailed damage list is worthwhile for the
/// current native presentation path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DamageSupport {
    /// The backend forwards each rectangle to the native compositor or blitter.
    Rectangles,
    /// The backend first uploads the bounding rectangle of all damage.
    BoundingRect,
    /// The backend maps damage to its own persistent presentation tiles.
    Tiles,
    /// The backend asks the display driver to apply rectangles; drivers may
    /// ignore that request and update the full framebuffer.
    DriverDependent,
    /// The backend has no partial-present operation and submits the full frame.
    FullFrame,
    /// The platform requires damage before the buffer is locked, so this API
    /// submits a full frame after the lock-time decision has already passed.
    LockTimeRequired,
}

impl DamageSupport {
    /// Returns whether the backend can use a damage list to reduce some work.
    pub const fn supports_partial_damage(self) -> bool {
        matches!(
            self,
            Self::Rectangles | Self::BoundingRect | Self::Tiles | Self::DriverDependent
        )
    }

    /// Returns whether callers should treat a presentation as a full-frame
    /// update regardless of the supplied rectangles.
    pub const fn is_full_frame_fallback(self) -> bool {
        matches!(self, Self::FullFrame | Self::LockTimeRequired)
    }
}

/// A terminal grid region used to align software presentation tiles to rows.
///
/// Coordinates are physical pixels in the surface buffer. Regions must not
/// extend outside the surface and are ignored when they are empty.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PresentationRegion {
    /// Top-left x coordinate.
    pub x: u32,
    /// Top-left y coordinate.
    pub y: u32,
    /// Region width.
    pub width: u32,
    /// Region height.
    pub height: u32,
    /// Physical height of one terminal row.
    pub row_height: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PresentationLayoutSnapshot {
    pub generation: u64,
    pub rows_per_block: u32,
    pub regions: Vec<PresentationRegion>,
}

#[derive(Default)]
struct PresentationLayouts {
    next_generation: u64,
    layouts: HashMap<u64, PresentationLayoutSnapshot>,
}

static PRESENTATION_LAYOUTS: OnceLock<RwLock<PresentationLayouts>> = OnceLock::new();

#[cfg(target_vendor = "apple")]
static MACOS_CA_BACKING_STORE_ENABLED: AtomicBool = AtomicBool::new(false);

/// Selects the CoreAnimation backing-store path for subsequently created Apple surfaces.
///
/// Existing surfaces keep the mode selected at creation. The
/// `AXSSH_EXPERIMENT_CA_BACKING_STORE` environment variable overrides this
/// process setting when it is present.
#[cfg(target_vendor = "apple")]
pub fn set_macos_ca_backing_store_enabled(enabled: bool) {
    MACOS_CA_BACKING_STORE_ENABLED.store(enabled, Ordering::Relaxed);
}

#[cfg(target_vendor = "apple")]
pub(crate) fn macos_ca_backing_store_enabled() -> bool {
    MACOS_CA_BACKING_STORE_ENABLED.load(Ordering::Relaxed)
}

/// Updates the software presentation layout for one native window.
///
/// The key is intentionally opaque to softbuffer. Applications should derive
/// it from their windowing toolkit's stable native window identity. A key of
/// zero is the fallback layout used before a native window is available.
pub fn set_presentation_layout(key: u64, rows_per_block: u32, regions: &[PresentationRegion]) {
    let rows_per_block = rows_per_block.clamp(1, 16);
    let regions = regions
        .iter()
        .copied()
        .filter(|region| region.width > 0 && region.height > 0 && region.row_height > 0)
        .take(64)
        .collect::<Vec<_>>();
    let layouts = PRESENTATION_LAYOUTS.get_or_init(|| RwLock::new(PresentationLayouts::default()));
    let Ok(mut layouts) = layouts.write() else {
        return;
    };
    if layouts.layouts.get(&key).is_some_and(|current| {
        current.rows_per_block == rows_per_block && current.regions == regions
    }) {
        return;
    }
    layouts.next_generation = layouts.next_generation.wrapping_add(1).max(1);
    let generation = layouts.next_generation;
    layouts.layouts.insert(
        key,
        PresentationLayoutSnapshot {
            generation,
            rows_per_block,
            regions,
        },
    );
    // Detached windows are bounded by the application, but keep the backend
    // registry bounded as well if a caller supplies short-lived keys.
    if layouts.layouts.len() > 32 {
        if let Some(oldest_key) = layouts
            .layouts
            .iter()
            .min_by_key(|(_, layout)| layout.generation)
            .map(|(key, _)| *key)
        {
            layouts.layouts.remove(&oldest_key);
        }
    }
}

/// Removes an application-managed software presentation layout.
///
/// Surfaces using this key fall back to the default presentation partition on
/// their next layout check. Removing a missing key is a no-op.
pub fn remove_presentation_layout(key: u64) {
    let Some(layouts) = PRESENTATION_LAYOUTS.get() else {
        return;
    };
    let Ok(mut layouts) = layouts.write() else {
        return;
    };
    layouts.layouts.remove(&key);
}

#[cfg_attr(not(target_vendor = "apple"), allow(dead_code))]
pub(crate) fn presentation_layout_generation(key: u64) -> u64 {
    let Some(layouts) = PRESENTATION_LAYOUTS.get() else {
        return 0;
    };
    let Ok(layouts) = layouts.read() else {
        return 0;
    };
    layouts
        .layouts
        .get(&key)
        .or_else(|| layouts.layouts.get(&0))
        .map_or(0, |layout| layout.generation)
}

#[cfg_attr(not(target_vendor = "apple"), allow(dead_code))]
pub(crate) fn presentation_layout(key: u64) -> PresentationLayoutSnapshot {
    let Some(layouts) = PRESENTATION_LAYOUTS.get() else {
        return PresentationLayoutSnapshot {
            generation: 0,
            rows_per_block: 4,
            regions: Vec::new(),
        };
    };
    let Ok(layouts) = layouts.read() else {
        return PresentationLayoutSnapshot {
            generation: 0,
            rows_per_block: 4,
            regions: Vec::new(),
        };
    };
    layouts
        .layouts
        .get(&key)
        .or_else(|| layouts.layouts.get(&0))
        .cloned()
        .unwrap_or(PresentationLayoutSnapshot {
            generation: 0,
            rows_per_block: 4,
            regions: Vec::new(),
        })
}

/// A surface for drawing to a window with software buffers.
#[derive(Debug)]
pub struct Surface<D, W> {
    /// This is boxed so that `Surface` is the same size on every platform.
    surface_impl: Box<SurfaceDispatch<D, W>>,
    _marker: PhantomData<Cell<()>>,
}

impl<D: HasDisplayHandle, W: HasWindowHandle> Surface<D, W> {
    /// Creates a new surface for the context for the provided window.
    pub fn new(context: &Context<D>, window: W) -> Result<Self, SoftBufferError> {
        match SurfaceDispatch::new(window, &context.context_impl) {
            Ok(surface_dispatch) => Ok(Self {
                surface_impl: Box::new(surface_dispatch),
                _marker: PhantomData,
            }),
            Err(InitError::Unsupported(window)) => {
                let raw = window.window_handle()?.as_raw();
                Err(SoftBufferError::UnsupportedWindowPlatform {
                    human_readable_window_platform_name: window_handle_type_name(&raw),
                    human_readable_display_platform_name: context.context_impl.variant_name(),
                    window_handle: raw,
                })
            }
            Err(InitError::Failure(f)) => Err(f),
        }
    }

    /// Get a reference to the underlying window handle.
    pub fn window(&self) -> &W {
        self.surface_impl.window()
    }

    /// Set the size of the buffer that will be returned by [`Surface::buffer_mut`].
    ///
    /// If the size of the buffer does not match the size of the window, the buffer is drawn
    /// in the upper-left corner of the window. It is recommended in most production use cases
    /// to have the buffer fill the entire window. Use your windowing library to find the size
    /// of the window.
    pub fn resize(&mut self, width: NonZeroU32, height: NonZeroU32) -> Result<(), SoftBufferError> {
        self.surface_impl.resize(width, height)
    }

    /// Associates this surface with an application-managed presentation layout.
    ///
    /// Backends that do not support independent software presentation layers
    /// accept the key and continue using their normal presentation path.
    pub fn set_presentation_layout_key(&mut self, key: u64) {
        self.surface_impl.set_presentation_layout_key(key);
    }

    /// Mark the contents of the surface as invalid.
    ///
    /// The next [`Surface::buffer_mut`] call reports a new buffer through
    /// [`Buffer::age`]. This is required after a window is hidden or its
    /// native backing surface is recreated.
    pub fn invalidate(&mut self) {
        self.surface_impl.invalidate();
    }

    /// Reports how this surface consumes damage rectangles at presentation.
    ///
    /// The result can change with runtime backend state, such as Wayland
    /// protocol version or X11 shared-memory availability.
    pub fn damage_support(&self) -> DamageSupport {
        self.surface_impl.damage_support()
    }

    /// Copies the window contents into a buffer.
    ///
    /// ## Platform Dependent Behavior
    ///
    /// - On X11, the window must be visible.
    /// - On AppKit, UIKit, Redox and Wayland, this function is unimplemented.
    /// - On Web, this will fail if the content was supplied by
    ///   a different origin depending on the sites CORS rules.
    pub fn fetch(&mut self) -> Result<Vec<u32>, SoftBufferError> {
        self.surface_impl.fetch()
    }

    /// Return a [`Buffer`] that the next frame should be rendered into. The size must
    /// be set with [`Surface::resize`] first. The initial contents of the buffer may be zeroed, or
    /// may contain a previous frame. Call [`Buffer::age`] to determine this.
    ///
    /// ## Platform Dependent Behavior
    ///
    /// - On DRM/KMS, there is no reliable and sound way to wait for the page flip to happen from within
    ///   `softbuffer`. Therefore it is the responsibility of the user to wait for the page flip before
    ///   sending another frame.
    pub fn buffer_mut(&mut self) -> Result<Buffer<'_, D, W>, SoftBufferError> {
        Ok(Buffer {
            buffer_impl: self.surface_impl.buffer_mut()?,
            _marker: PhantomData,
        })
    }
}

impl<D: HasDisplayHandle, W: HasWindowHandle> AsRef<W> for Surface<D, W> {
    #[inline]
    fn as_ref(&self) -> &W {
        self.window()
    }
}

impl<D: HasDisplayHandle, W: HasWindowHandle> HasWindowHandle for Surface<D, W> {
    #[inline]
    fn window_handle(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        self.window().window_handle()
    }
}

/// A buffer that can be written to by the CPU and presented to the window.
///
/// This derefs to a `[u32]`, which depending on the backend may be a mapping into shared memory
/// accessible to the display server, so presentation doesn't require any (client-side) copying.
///
/// This trusts the display server not to mutate the buffer, which could otherwise be unsound.
///
/// # Data representation
///
/// The format of the buffer is as follows. There is one `u32` in the buffer for each pixel in
/// the area to draw. The first entry is the upper-left most pixel. The second is one to the right
/// etc. (Row-major top to bottom left to right one `u32` per pixel). Within each `u32` the highest
/// order 8 bits are to be set to 0. The next highest order 8 bits are the red channel, then the
/// green channel, and then the blue channel in the lowest-order 8 bits. See the examples for
/// one way to build this format using bitwise operations.
///
/// --------
///
/// Pixel format (`u32`):
///
/// 00000000RRRRRRRRGGGGGGGGBBBBBBBB
///
/// 0: Bit is 0
/// R: Red channel
/// G: Green channel
/// B: Blue channel
///
/// # Platform dependent behavior
/// No-copy presentation is currently supported on:
/// - Wayland
/// - X, when XShm is available
/// - Win32
/// - Orbital, when buffer size matches window size
///
/// Currently [`Buffer::present`] must block copying image data on:
/// - Web
/// - AppKit
/// - UIKit
///
/// Buffer copies an channel swizzling happen on:
/// - Android
#[derive(Debug)]
pub struct Buffer<'a, D, W> {
    buffer_impl: BufferDispatch<'a, D, W>,
    _marker: PhantomData<(Arc<D>, Cell<()>)>,
}

impl<D: HasDisplayHandle, W: HasWindowHandle> Buffer<'_, D, W> {
    /// The amount of pixels wide the buffer is.
    pub fn width(&self) -> NonZeroU32 {
        let width = self.buffer_impl.width();
        debug_assert_eq!(
            width.get() as usize * self.buffer_impl.height().get() as usize,
            self.len(),
            "buffer must be sized correctly"
        );
        width
    }

    /// The amount of pixels tall the buffer is.
    pub fn height(&self) -> NonZeroU32 {
        let height = self.buffer_impl.height();
        debug_assert_eq!(
            height.get() as usize * self.buffer_impl.width().get() as usize,
            self.len(),
            "buffer must be sized correctly"
        );
        height
    }

    /// `age` is the number of frames ago this buffer was last presented. So if the value is
    /// `1`, it is the same as the last frame, and if it is `2`, it is the same as the frame
    /// before that (for backends using double buffering). If the value is `0`, it is a new
    /// buffer that has unspecified contents.
    ///
    /// This can be used to update only a portion of the buffer.
    pub fn age(&self) -> u8 {
        self.buffer_impl.age()
    }

    /// Presents buffer to the window.
    ///
    /// # Platform dependent behavior
    ///
    /// ## Wayland
    ///
    /// On Wayland, calling this function may send requests to the underlying `wl_surface`. The
    /// graphics context may issue `wl_surface.attach`, `wl_surface.damage`, `wl_surface.damage_buffer`
    /// and `wl_surface.commit` requests when presenting the buffer.
    ///
    /// If the caller wishes to synchronize other surface/window changes, such requests must be sent to the
    /// Wayland compositor before calling this function.
    pub fn present(self) -> Result<(), SoftBufferError> {
        self.buffer_impl.present()
    }

    /// Presents buffer to the window, with damage regions.
    ///
    /// # Platform dependent behavior
    ///
    /// The backend-specific behavior is available from
    /// [`Surface::damage_support`]. Backends that report [`DamageSupport::FullFrame`]
    /// or [`DamageSupport::LockTimeRequired`] treat this call as equivalent to
    /// [`Self::present`].
    pub fn present_with_damage(self, damage: &[Rect]) -> Result<(), SoftBufferError> {
        self.buffer_impl.present_with_damage(damage)
    }
}

impl<D: HasDisplayHandle, W: HasWindowHandle> ops::Deref for Buffer<'_, D, W> {
    type Target = [u32];

    #[inline]
    fn deref(&self) -> &[u32] {
        self.buffer_impl.pixels()
    }
}

impl<D: HasDisplayHandle, W: HasWindowHandle> ops::DerefMut for Buffer<'_, D, W> {
    #[inline]
    fn deref_mut(&mut self) -> &mut [u32] {
        self.buffer_impl.pixels_mut()
    }
}

/// There is no display handle.
#[derive(Debug)]
#[allow(dead_code)]
pub struct NoDisplayHandle(core::convert::Infallible);

impl HasDisplayHandle for NoDisplayHandle {
    fn display_handle(
        &self,
    ) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError> {
        match self.0 {}
    }
}

/// There is no window handle.
#[derive(Debug)]
pub struct NoWindowHandle(());

impl HasWindowHandle for NoWindowHandle {
    fn window_handle(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        Err(raw_window_handle::HandleError::NotSupported)
    }
}

fn window_handle_type_name(handle: &RawWindowHandle) -> &'static str {
    match handle {
        RawWindowHandle::Xlib(_) => "Xlib",
        RawWindowHandle::Win32(_) => "Win32",
        RawWindowHandle::WinRt(_) => "WinRt",
        RawWindowHandle::Web(_) => "Web",
        RawWindowHandle::Wayland(_) => "Wayland",
        RawWindowHandle::AndroidNdk(_) => "AndroidNdk",
        RawWindowHandle::AppKit(_) => "AppKit",
        RawWindowHandle::Orbital(_) => "Orbital",
        RawWindowHandle::UiKit(_) => "UiKit",
        RawWindowHandle::Xcb(_) => "XCB",
        RawWindowHandle::Drm(_) => "DRM",
        RawWindowHandle::Gbm(_) => "GBM",
        RawWindowHandle::Haiku(_) => "Haiku",
        _ => "Unknown Name", //don't completely fail to compile if there is a new raw window handle type that's added at some point
    }
}

fn display_handle_type_name(handle: &RawDisplayHandle) -> &'static str {
    match handle {
        RawDisplayHandle::Xlib(_) => "Xlib",
        RawDisplayHandle::Web(_) => "Web",
        RawDisplayHandle::Wayland(_) => "Wayland",
        RawDisplayHandle::AppKit(_) => "AppKit",
        RawDisplayHandle::Orbital(_) => "Orbital",
        RawDisplayHandle::UiKit(_) => "UiKit",
        RawDisplayHandle::Xcb(_) => "XCB",
        RawDisplayHandle::Drm(_) => "DRM",
        RawDisplayHandle::Gbm(_) => "GBM",
        RawDisplayHandle::Haiku(_) => "Haiku",
        RawDisplayHandle::Windows(_) => "Windows",
        RawDisplayHandle::Android(_) => "Android",
        _ => "Unknown Name", //don't completely fail to compile if there is a new raw window handle type that's added at some point
    }
}

#[cfg(test)]
mod presentation_layout_tests {
    use super::*;

    #[test]
    fn damage_support_reports_partial_and_full_frame_paths() {
        for support in [
            DamageSupport::Rectangles,
            DamageSupport::BoundingRect,
            DamageSupport::Tiles,
            DamageSupport::DriverDependent,
        ] {
            assert!(support.supports_partial_damage());
            assert!(!support.is_full_frame_fallback());
        }

        for support in [DamageSupport::FullFrame, DamageSupport::LockTimeRequired] {
            assert!(!support.supports_partial_damage());
            assert!(support.is_full_frame_fallback());
        }
    }

    #[test]
    fn layout_generation_deduplicates_updates_and_removal_unregisters_key() {
        const KEY: u64 = u64::MAX - 41;
        let region = PresentationRegion {
            x: 10,
            y: 20,
            width: 300,
            height: 180,
            row_height: 18,
        };

        remove_presentation_layout(KEY);
        set_presentation_layout(KEY, 4, &[region]);
        let first_generation = presentation_layout_generation(KEY);
        assert_ne!(first_generation, 0);

        set_presentation_layout(KEY, 4, &[region]);
        assert_eq!(presentation_layout_generation(KEY), first_generation);

        set_presentation_layout(KEY, 5, &[region]);
        assert_ne!(presentation_layout_generation(KEY), first_generation);

        remove_presentation_layout(KEY);
        let layouts = PRESENTATION_LAYOUTS
            .get()
            .expect("test layout registry initialized")
            .read()
            .expect("test layout registry lock available");
        assert!(!layouts.layouts.contains_key(&KEY));
    }
}

#[cfg(not(target_family = "wasm"))]
fn __assert_send() {
    fn is_send<T: Send>() {}
    fn is_sync<T: Sync>() {}

    is_send::<Context<()>>();
    is_sync::<Context<()>>();
    is_send::<Surface<(), ()>>();
    is_send::<Buffer<'static, (), ()>>();

    /// ```compile_fail
    /// use softbuffer::Surface;
    ///
    /// fn __is_sync<T: Sync>() {}
    /// __is_sync::<Surface<(), ()>>();
    /// ```
    fn __surface_not_sync() {}
    /// ```compile_fail
    /// use softbuffer::Buffer;
    ///
    /// fn __is_sync<T: Sync>() {}
    /// __is_sync::<Buffer<'static, (), ()>>();
    /// ```
    fn __buffer_not_sync() {}
}

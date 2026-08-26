//! Softbuffer implementation using CoreGraphics.
use crate::backend_interface::*;
use crate::error::InitError;
use crate::{util, Rect, SoftBufferError};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Bool};
use objc2::{define_class, msg_send, AllocAnyThread, DefinedClass, MainThreadMarker, Message};
use objc2_core_foundation::{CFRetained, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGBitmapInfo, CGColorRenderingIntent, CGColorSpace, CGDataProvider, CGImage, CGImageAlphaInfo,
    CGImageByteOrderInfo, CGImageComponentInfo, CGImagePixelFormatInfo,
};
use objc2_foundation::{
    ns_string, NSDictionary, NSKeyValueChangeKey, NSKeyValueChangeNewKey,
    NSKeyValueObservingOptions, NSNumber, NSObject, NSObjectNSKeyValueObserverRegistration,
    NSString, NSValue,
};
use objc2_quartz_core::{kCAGravityTopLeft, CALayer, CATransaction};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawWindowHandle};

use std::ffi::c_void;
use std::marker::PhantomData;
use std::mem::size_of;
use std::num::NonZeroU32;
use std::ops::Deref;
use std::ptr::{self, slice_from_raw_parts_mut, NonNull};

const TILE_WIDTH: usize = 256;
const TILE_HEIGHT: usize = 128;
const DEBUG_TILE_BORDER_COLOR: u32 = 0xffff3b81;
const DEBUG_TILE_LABEL_BACKGROUND: u32 = 0xff10202a;
const DEBUG_TILE_LABEL_COLOR: u32 = 0xffffffff;
const DEBUG_TILE_LABEL_WIDTH: usize = 72;
const DEBUG_TILE_LABEL_HEIGHT: usize = 18;
const DEBUG_TILE_GLYPH_SCALE: usize = 2;

/// A compact 3x5 font for the diagnostic `T<index>` label.
const DEBUG_TILE_GLYPHS: [[u8; 5]; 11] = [
    [0b111, 0b101, 0b101, 0b101, 0b111], // 0
    [0b010, 0b110, 0b010, 0b010, 0b111], // 1
    [0b110, 0b001, 0b010, 0b100, 0b111], // 2
    [0b110, 0b001, 0b010, 0b001, 0b110], // 3
    [0b101, 0b101, 0b111, 0b001, 0b001], // 4
    [0b111, 0b100, 0b110, 0b001, 0b110], // 5
    [0b011, 0b100, 0b111, 0b101, 0b111], // 6
    [0b111, 0b001, 0b010, 0b010, 0b010], // 7
    [0b111, 0b101, 0b111, 0b101, 0b111], // 8
    [0b111, 0b101, 0b111, 0b001, 0b110], // 9
    [0b111, 0b010, 0b010, 0b010, 0b010], // T
];

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "SoftbufferObserver"]
    #[ivars = SendCALayer]
    #[derive(Debug)]
    struct Observer;

    /// NSKeyValueObserving
    impl Observer {
        #[unsafe(method(observeValueForKeyPath:ofObject:change:context:))]
        fn observe_value(
            &self,
            key_path: Option<&NSString>,
            _object: Option<&AnyObject>,
            change: Option<&NSDictionary<NSKeyValueChangeKey, AnyObject>>,
            _context: *mut c_void,
        ) {
            self.update(key_path, change);
        }
    }
);

impl Observer {
    fn new(layer: &CALayer) -> Retained<Self> {
        let this = Self::alloc().set_ivars(SendCALayer(layer.retain()));
        unsafe { msg_send![super(this), init] }
    }

    fn update(
        &self,
        key_path: Option<&NSString>,
        change: Option<&NSDictionary<NSKeyValueChangeKey, AnyObject>>,
    ) {
        let layer = self.ivars();

        let change =
            change.expect("requested a change dictionary in `addObserver`, but none was provided");
        let new = change
            .objectForKey(unsafe { NSKeyValueChangeNewKey })
            .expect("requested change dictionary did not contain `NSKeyValueChangeNewKey`");

        // NOTE: Setting these values usually causes a quarter second animation to occur, which is
        // undesirable.
        //
        // However, since we're setting them inside an observer, there already is a transaction
        // ongoing, and as such we don't need to wrap this in a `CATransaction` ourselves.

        if key_path == Some(ns_string!("contentsScale")) {
            let new = new.downcast::<NSNumber>().unwrap();
            let scale_factor = new.as_cgfloat();

            // Set the scale factor of the layer to match the root layer when it changes (e.g. if
            // moved to a different monitor, or monitor settings changed).
            layer.setContentsScale(scale_factor);
        } else if key_path == Some(ns_string!("bounds")) {
            let new = new.downcast::<NSValue>().unwrap();
            let bounds = new.get_rect().expect("new bounds value was not CGRect");

            // Set `bounds` and `position` so that the new layer is inside the superlayer.
            //
            // This differs from just setting the `bounds`, as it also takes into account any
            // translation that the superlayer may have that we'd want to preserve.
            layer.setFrame(bounds);
        } else {
            panic!("unknown observed keypath {key_path:?}");
        }
    }
}

#[derive(Debug)]
pub struct CGImpl<D, W> {
    /// Container layer for the independently owned tile contents.
    layer: SendCALayer,
    /// The layer that our layer was created from.
    ///
    /// Can also be retrieved from `layer.superlayer()`.
    root_layer: SendCALayer,
    observer: Retained<Observer>,
    color_space: CFRetained<CGColorSpace>,
    /// The width of the underlying buffer.
    width: usize,
    /// The height of the underlying buffer.
    height: usize,
    /// Pixels persist between frames so Slint can render into a reused buffer.
    buffer: util::PixelBuffer,
    /// Whether the current framebuffer has been submitted successfully.
    buffer_valid: bool,
    /// Contents scale used when the tile layer geometry was built.
    tile_scale: f64,
    /// Tile layers keep old immutable images alive while the CPU framebuffer is reused.
    tiles: Vec<TileLayer>,
    /// Diagnostic-only tile boundaries and indexes, disabled unless explicitly requested.
    debug_tiles: bool,
    window_handle: W,
    _display: PhantomData<D>,
}

impl<D, W> Drop for CGImpl<D, W> {
    fn drop(&mut self) {
        // SAFETY: Registered in `new`, must be removed before the observer is deallocated.
        unsafe {
            self.root_layer
                .removeObserver_forKeyPath(&self.observer, ns_string!("contentsScale"));
            self.root_layer
                .removeObserver_forKeyPath(&self.observer, ns_string!("bounds"));
        }
    }
}

impl<D: HasDisplayHandle, W: HasWindowHandle> SurfaceInterface<D, W> for CGImpl<D, W> {
    type Context = D;
    type Buffer<'a>
        = BufferImpl<'a, D, W>
    where
        Self: 'a;

    fn new(window_src: W, _display: &D) -> Result<Self, InitError<W>> {
        // `NSView`/`UIView` can only be accessed from the main thread.
        let _mtm = MainThreadMarker::new().ok_or(SoftBufferError::PlatformError(
            Some("can only access Core Graphics handles from the main thread".to_string()),
            None,
        ))?;

        let root_layer = match window_src.window_handle()?.as_raw() {
            RawWindowHandle::AppKit(handle) => {
                // SAFETY: The pointer came from `WindowHandle`, which ensures that the
                // `AppKitWindowHandle` contains a valid pointer to an `NSView`.
                //
                // We use `NSObject` here to avoid importing `objc2-app-kit`.
                let view: &NSObject = unsafe { handle.ns_view.cast().as_ref() };

                // Force the view to become layer backed
                let _: () = unsafe { msg_send![view, setWantsLayer: Bool::YES] };

                // SAFETY: `-[NSView layer]` returns an optional `CALayer`
                let layer: Option<Retained<CALayer>> = unsafe { msg_send![view, layer] };
                layer.expect("failed making the view layer-backed")
            }
            RawWindowHandle::UiKit(handle) => {
                // SAFETY: The pointer came from `WindowHandle`, which ensures that the
                // `UiKitWindowHandle` contains a valid pointer to an `UIView`.
                //
                // We use `NSObject` here to avoid importing `objc2-ui-kit`.
                let view: &NSObject = unsafe { handle.ui_view.cast().as_ref() };

                // SAFETY: `-[UIView layer]` returns `CALayer`
                let layer: Retained<CALayer> = unsafe { msg_send![view, layer] };
                layer
            }
            _ => return Err(InitError::Unsupported(window_src)),
        };

        // Add a sublayer, to avoid interfering with the root layer, since setting the contents of
        // e.g. a view-controlled layer is brittle.
        let layer = CALayer::new();
        root_layer.addSublayer(&layer);

        // Set the anchor point and geometry. Softbuffer's uses a coordinate system with the origin
        // in the top-left corner.
        //
        // NOTE: This doesn't really matter unless we start modifying the `position` of our layer
        // ourselves, but it's nice to have in place.
        layer.setAnchorPoint(CGPoint::new(0.0, 0.0));
        layer.setGeometryFlipped(true);

        // Do not use auto-resizing mask.
        //
        // This is done to work around a bug in macOS 14 and above, where views using auto layout
        // may end up setting fractional values as the bounds, and that in turn doesn't propagate
        // properly through the auto-resizing mask and with contents gravity.
        //
        // Instead, we keep the bounds of the layer in sync with the root layer using an observer,
        // see below.
        //
        // layer.setAutoresizingMask(kCALayerHeightSizable | kCALayerWidthSizable);

        let observer = Observer::new(&layer);
        // Observe changes to the root layer's bounds and scale factor, and apply them to our layer.
        //
        // The previous implementation updated the scale factor inside `resize`, but this works
        // poorly with transactions, and is generally inefficient. Instead, we update the scale
        // factor only when needed because the super layer's scale factor changed.
        //
        // Note that inherent in this is an explicit design decision: We control the `bounds` and
        // `contentsScale` of the layer directly, and instead let the `resize` call that the user
        // controls only be the size of the underlying buffer.
        //
        // SAFETY: Observer deregistered in `Drop` before the observer object is deallocated.
        unsafe {
            root_layer.addObserver_forKeyPath_options_context(
                &observer,
                ns_string!("contentsScale"),
                NSKeyValueObservingOptions::New | NSKeyValueObservingOptions::Initial,
                ptr::null_mut(),
            );
            root_layer.addObserver_forKeyPath_options_context(
                &observer,
                ns_string!("bounds"),
                NSKeyValueObservingOptions::New | NSKeyValueObservingOptions::Initial,
                ptr::null_mut(),
            );
        }

        // Set the content so that it is placed in the top-left corner if it does not have the same
        // size as the surface itself.
        //
        // TODO(madsmtm): Consider changing this to `kCAGravityResize` to stretch the content if
        // resized to something that doesn't fit, see #177.
        layer.setContentsGravity(unsafe { kCAGravityTopLeft });

        // Initialize color space here, to reduce work later on.
        let color_space = CGColorSpace::new_device_rgb().ok_or_else(|| {
            SoftBufferError::PlatformError(
                Some("failed to create CoreGraphics RGB color space".to_string()),
                None,
            )
        })?;

        // Grab initial width and height from the layer (whose properties have just been initialized
        // by the observer using `NSKeyValueObservingOptionInitial`).
        let size = layer.bounds().size;
        let scale_factor = layer.contentsScale();
        let width = (size.width * scale_factor) as usize;
        let height = (size.height * scale_factor) as usize;

        let width = width.max(1);
        let height = height.max(1);
        let buffer_len = width
            .checked_mul(height)
            .ok_or_else(|| size_out_of_range(width, height))?;

        let mut this = Self {
            layer: SendCALayer(layer),
            root_layer: SendCALayer(root_layer),
            observer,
            color_space,
            width,
            height,
            buffer: util::PixelBuffer(vec![0; buffer_len]),
            buffer_valid: false,
            tile_scale: 1.0,
            tiles: Vec::new(),
            debug_tiles: software_tile_debug_enabled(),
            _display: PhantomData,
            window_handle: window_src,
        };
        this.rebuild_tiles()?;
        Ok(this)
    }

    #[inline]
    fn window(&self) -> &W {
        &self.window_handle
    }

    fn resize(&mut self, width: NonZeroU32, height: NonZeroU32) -> Result<(), SoftBufferError> {
        let width = width.get() as usize;
        let height = height.get() as usize;
        let len = width.checked_mul(height).ok_or_else(|| {
            size_out_of_range(width, height)
        })?;
        if self.width == width && self.height == height && self.buffer.len() == len {
            return self.ensure_tile_layout();
        }
        self.width = width;
        self.height = height;
        self.buffer = util::PixelBuffer(vec![0; len]);
        self.buffer_valid = false;
        self.rebuild_tiles()
    }

    fn buffer_mut(&mut self) -> Result<BufferImpl<'_, D, W>, SoftBufferError> {
        self.ensure_tile_layout()?;
        if self.buffer.len() != self.width.saturating_mul(self.height) {
            let len = self
                .width
                .checked_mul(self.height)
                .ok_or_else(|| size_out_of_range(self.width, self.height))?;
            self.buffer = util::PixelBuffer(vec![0; len]);
            self.buffer_valid = false;
        }
        Ok(BufferImpl { imp: self })
    }

    fn invalidate(&mut self) {
        self.buffer_valid = false;
    }
}

impl<D, W> CGImpl<D, W> {
    fn present_full(&mut self) -> Result<(), SoftBufferError> {
        self.ensure_tile_layout()?;
        let dirty_tiles = vec![true; self.tiles.len()];
        self.present_tiles(&dirty_tiles)
    }

    fn present_with_damage(&mut self, damage: &[Rect]) -> Result<(), SoftBufferError> {
        self.ensure_tile_layout()?;
        if !self.buffer_valid {
            return self.present_full();
        }
        let dirty_tiles = self.dirty_tile_mask(damage);
        self.present_tiles(&dirty_tiles)
    }

    fn present_tiles(&mut self, dirty_tiles: &[bool]) -> Result<(), SoftBufferError> {
        let mut images = Vec::new();
        for (index, dirty) in dirty_tiles.iter().copied().enumerate() {
            if dirty {
                images.push((index, self.tile_image(&self.tiles[index], index)?));
            }
        }
        if images.is_empty() {
            return Ok(());
        }

        CATransaction::begin();
        CATransaction::setDisableActions(true);
        for (index, image) in &images {
            // SAFETY: The contents is `CGImage`, which is a valid class for `contents`.
            unsafe {
                self.tiles[*index]
                    .layer
                    .setContents(Some(image.as_ref()))
            };
        }
        CATransaction::commit();
        self.buffer_valid = true;
        Ok(())
    }

    fn rebuild_tiles(&mut self) -> Result<(), SoftBufferError> {
        for tile in &self.tiles {
            tile.layer.removeFromSuperlayer();
        }
        self.tiles.clear();

        let columns = self.width.div_ceil(TILE_WIDTH);
        let rows = self.height.div_ceil(TILE_HEIGHT);
        let tile_count = columns.checked_mul(rows).ok_or_else(|| {
            SoftBufferError::PlatformError(
                Some("CoreGraphics tile layout is too large".to_string()),
                None,
            )
        })?;
        self.tiles.try_reserve(tile_count).map_err(|_| {
            SoftBufferError::PlatformError(
                Some("failed to allocate CoreGraphics tile layout".to_string()),
                None,
            )
        })?;

        let scale = self.layer.contentsScale().max(1.0);
        self.layer.setMasksToBounds(true);
        for origin_y in (0..self.height).step_by(TILE_HEIGHT) {
            for origin_x in (0..self.width).step_by(TILE_WIDTH) {
                let width = (self.width - origin_x).min(TILE_WIDTH);
                let height = (self.height - origin_y).min(TILE_HEIGHT);
                let tile = CALayer::new();
                tile.setAnchorPoint(CGPoint::new(0.0, 0.0));
                // The parent surface already uses a top-left coordinate system. Flipping this
                // child again reverses the CGImage contents on macOS.
                tile.setContentsGravity(unsafe { kCAGravityTopLeft });
                tile.setContentsScale(scale);
                tile.setFrame(CGRect::new(
                    CGPoint::new(origin_x as f64 / scale, origin_y as f64 / scale),
                    CGSize::new(width as f64 / scale, height as f64 / scale),
                ));
                self.layer.addSublayer(&tile);
                self.tiles.push(TileLayer {
                    layer: SendCALayer(tile),
                    origin_x,
                    origin_y,
                    width,
                    height,
                });
            }
        }
        self.tile_scale = scale;
        Ok(())
    }

    fn ensure_tile_layout(&mut self) -> Result<(), SoftBufferError> {
        let scale = self.layer.contentsScale().max(1.0);
        if scale != self.tile_scale {
            self.rebuild_tiles()?;
            self.buffer_valid = false;
        }
        Ok(())
    }

    fn dirty_tile_mask(&self, damage: &[Rect]) -> Vec<bool> {
        dirty_tile_mask_for_size(self.width, self.height, damage)
    }
}

fn dirty_tile_mask_for_size(width: usize, height: usize, damage: &[Rect]) -> Vec<bool> {
    let columns = width.div_ceil(TILE_WIDTH);
    let rows = height.div_ceil(TILE_HEIGHT);
    let mut dirty_tiles = vec![false; columns * rows];
    for rect in damage {
        let origin_x = usize::try_from(rect.x).unwrap_or(usize::MAX).min(width);
        let origin_y = usize::try_from(rect.y).unwrap_or(usize::MAX).min(height);
        let end_x = origin_x
            .saturating_add(rect.width.get() as usize)
            .min(width);
        let end_y = origin_y
            .saturating_add(rect.height.get() as usize)
            .min(height);
        if origin_x >= end_x || origin_y >= end_y {
            continue;
        }
        let first_column = origin_x / TILE_WIDTH;
        let last_column = (end_x - 1) / TILE_WIDTH;
        let first_row = origin_y / TILE_HEIGHT;
        let last_row = (end_y - 1) / TILE_HEIGHT;
        for row in first_row..=last_row {
            for column in first_column..=last_column {
                dirty_tiles[row * columns + column] = true;
            }
        }
    }
    dirty_tiles
}

impl<D, W> CGImpl<D, W> {
    fn tile_image(
        &self,
        tile: &TileLayer,
        index: usize,
    ) -> Result<CFRetained<CGImage>, SoftBufferError> {
        let mut pixels = Vec::with_capacity(tile.width * tile.height);
        for row in 0..tile.height {
            let start = (tile.origin_y + row) * self.width + tile.origin_x;
            let end = start + tile.width;
            pixels.extend_from_slice(&self.buffer.0[start..end]);
        }
        if self.debug_tiles {
            draw_tile_debug_overlay(&mut pixels, tile.width, tile.height, index);
        }
        let data_provider = owned_data_provider(pixels)?;
        let bitmap_info = CGBitmapInfo(
            CGImageAlphaInfo::NoneSkipFirst.0
                | CGImageComponentInfo::Integer.0
                | CGImageByteOrderInfo::Order32Little.0
                | CGImagePixelFormatInfo::Packed.0,
        );
        unsafe {
            CGImage::new(
                tile.width,
                tile.height,
                8,
                32,
                tile.width * size_of::<u32>(),
                Some(&self.color_space),
                bitmap_info,
                Some(&data_provider),
                ptr::null(),
                false,
                CGColorRenderingIntent::RenderingIntentDefault,
            )
        }
        .ok_or_else(|| {
            SoftBufferError::PlatformError(
                Some("failed to create CoreGraphics tile image".to_string()),
                None,
            )
        })
    }
}

fn software_tile_debug_enabled() -> bool {
    std::env::var("AXSSH_DEBUG_SOFTWARE_TILES").map_or(false, |value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

fn draw_tile_debug_overlay(pixels: &mut [u32], width: usize, height: usize, index: usize) {
    if width == 0 || height == 0 || pixels.len() < width.saturating_mul(height) {
        return;
    }

    for x in 0..width {
        pixels[x] = DEBUG_TILE_BORDER_COLOR;
        pixels[(height - 1) * width + x] = DEBUG_TILE_BORDER_COLOR;
    }
    for y in 0..height {
        let row = y * width;
        pixels[row] = DEBUG_TILE_BORDER_COLOR;
        pixels[row + width - 1] = DEBUG_TILE_BORDER_COLOR;
    }

    let label_width = width.min(DEBUG_TILE_LABEL_WIDTH);
    let label_height = height.min(DEBUG_TILE_LABEL_HEIGHT);
    for y in 0..label_height {
        let row = y * width;
        for x in 0..label_width {
            pixels[row + x] = DEBUG_TILE_LABEL_BACKGROUND;
        }
    }

    let mut glyph_x = 4;
    draw_tile_debug_glyph(pixels, width, height, glyph_x, 2, 10);
    glyph_x += 3 * DEBUG_TILE_GLYPH_SCALE + DEBUG_TILE_GLYPH_SCALE;
    for digit in index.to_string().bytes() {
        if glyph_x + 3 * DEBUG_TILE_GLYPH_SCALE > label_width {
            break;
        }
        draw_tile_debug_glyph(
            pixels,
            width,
            height,
            glyph_x,
            2,
            usize::from(digit - b'0'),
        );
        glyph_x += 3 * DEBUG_TILE_GLYPH_SCALE + DEBUG_TILE_GLYPH_SCALE;
    }
}

fn draw_tile_debug_glyph(
    pixels: &mut [u32],
    width: usize,
    height: usize,
    origin_x: usize,
    origin_y: usize,
    glyph_index: usize,
) {
    let Some(glyph) = DEBUG_TILE_GLYPHS.get(glyph_index) else {
        return;
    };
    for (row, bits) in glyph.iter().copied().enumerate() {
        for column in 0..3 {
            if bits & (1 << (2 - column)) == 0 {
                continue;
            }
            for y in 0..DEBUG_TILE_GLYPH_SCALE {
                for x in 0..DEBUG_TILE_GLYPH_SCALE {
                    let pixel_x = origin_x + column * DEBUG_TILE_GLYPH_SCALE + x;
                    let pixel_y = origin_y + row * DEBUG_TILE_GLYPH_SCALE + y;
                    if pixel_x < width && pixel_y < height {
                        pixels[pixel_y * width + pixel_x] = DEBUG_TILE_LABEL_COLOR;
                    }
                }
            }
        }
    }
}

#[derive(Debug)]
struct TileLayer {
    layer: SendCALayer,
    origin_x: usize,
    origin_y: usize,
    width: usize,
    height: usize,
}

fn size_out_of_range(width: usize, height: usize) -> SoftBufferError {
    let width = u32::try_from(width).unwrap_or(u32::MAX).max(1);
    let height = u32::try_from(height).unwrap_or(u32::MAX).max(1);
    SoftBufferError::SizeOutOfRange {
        width: NonZeroU32::new(width).unwrap_or(NonZeroU32::MIN),
        height: NonZeroU32::new(height).unwrap_or(NonZeroU32::MIN),
    }
}

fn owned_data_provider(pixels: Vec<u32>) -> Result<CFRetained<CGDataProvider>, SoftBufferError> {
    unsafe extern "C-unwind" fn release(
        _info: *mut c_void,
        data: NonNull<c_void>,
        size: usize,
    ) {
        let data = data.cast::<u32>();
        let slice = slice_from_raw_parts_mut(data.as_ptr(), size / size_of::<u32>());
        // SAFETY: This is the exact boxed slice passed to `CGDataProvider::with_data`.
        drop(unsafe { Box::from_raw(slice) });
    }

    let len = pixels.len().checked_mul(size_of::<u32>()).ok_or_else(|| {
        SoftBufferError::PlatformError(
                Some("CoreGraphics image backing is too large".to_string()),
            None,
        )
    })?;
    let raw_slice = Box::into_raw(pixels.into_boxed_slice());
    let data_ptr = raw_slice.cast::<c_void>();
    // SAFETY: The data pointer and byte length describe the owned boxed slice.
    let provider = unsafe { CGDataProvider::with_data(ptr::null_mut(), data_ptr, len, Some(release)) };
    match provider {
        Some(provider) => Ok(provider),
        None => {
            // SAFETY: Provider creation failed, so its release callback did not take ownership.
            drop(unsafe { Box::from_raw(raw_slice) });
            Err(SoftBufferError::PlatformError(
                Some("failed to create CoreGraphics image data provider".to_string()),
                None,
            ))
        }
    }
}

#[derive(Debug)]
pub struct BufferImpl<'a, D, W> {
    imp: &'a mut CGImpl<D, W>,
}

impl<D: HasDisplayHandle, W: HasWindowHandle> BufferInterface for BufferImpl<'_, D, W> {
    fn width(&self) -> NonZeroU32 {
        NonZeroU32::new(self.imp.width as u32).unwrap()
    }

    fn height(&self) -> NonZeroU32 {
        NonZeroU32::new(self.imp.height as u32).unwrap()
    }

    #[inline]
    fn pixels(&self) -> &[u32] {
        &self.imp.buffer
    }

    #[inline]
    fn pixels_mut(&mut self) -> &mut [u32] {
        &mut self.imp.buffer
    }

    fn age(&self) -> u8 {
        u8::from(self.imp.buffer_valid)
    }

    fn present(self) -> Result<(), SoftBufferError> {
        self.imp.present_full()
    }

    fn present_with_damage(self, damage: &[Rect]) -> Result<(), SoftBufferError> {
        self.imp.present_with_damage(damage)
    }
}

#[derive(Debug)]
struct SendCALayer(Retained<CALayer>);

// SAFETY: CALayer is dubiously thread safe, like most things in Core Animation.
// But since we make sure to do our changes within a CATransaction, it is
// _probably_ fine for us to use CALayer from different threads.
//
// See also:
// https://developer.apple.com/documentation/quartzcore/catransaction/1448267-lock?language=objc
// https://stackoverflow.com/questions/76250226/how-to-render-content-of-calayer-on-a-background-thread
unsafe impl Send for SendCALayer {}
// SAFETY: Same as above.
unsafe impl Sync for SendCALayer {}

impl Deref for SendCALayer {
    type Target = CALayer;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn damage_marks_only_intersecting_tiles() {
        let damage = [Rect {
            x: 300,
            y: 150,
            width: NonZeroU32::new(1).expect("non-zero test width"),
            height: NonZeroU32::new(1).expect("non-zero test height"),
        }];
        let dirty = dirty_tile_mask_for_size(600, 300, &damage);

        assert_eq!(dirty.len(), 3 * 3);
        assert!(dirty[4]);
        assert_eq!(dirty.iter().filter(|value| **value).count(), 1);
    }

    #[test]
    fn damage_crossing_tile_edges_marks_each_intersecting_tile() {
        let damage = [Rect {
            x: 255,
            y: 127,
            width: NonZeroU32::new(3).expect("non-zero test width"),
            height: NonZeroU32::new(3).expect("non-zero test height"),
        }];
        let dirty = dirty_tile_mask_for_size(600, 300, &damage);

        assert!(dirty[0]);
        assert!(dirty[1]);
        assert!(dirty[3]);
        assert!(dirty[4]);
        assert_eq!(dirty.iter().filter(|value| **value).count(), 4);
    }

    #[test]
    fn debug_tile_overlay_marks_edges_and_keeps_tile_interior() {
        let width = 32;
        let height = 24;
        let mut pixels = vec![0; width * height];

        draw_tile_debug_overlay(&mut pixels, width, height, 7);

        assert_eq!(pixels[width - 1], DEBUG_TILE_BORDER_COLOR);
        assert_eq!(pixels[(height - 1) * width + 12], DEBUG_TILE_BORDER_COLOR);
        assert_eq!(pixels[12 * width + 20], 0);
        assert_eq!(pixels[2 * width + 4], DEBUG_TILE_LABEL_COLOR);
    }
}

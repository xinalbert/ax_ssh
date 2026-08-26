//! Softbuffer implementation using CoreGraphics.
use crate::backend_interface::*;
use crate::error::InitError;
use crate::{
    presentation_layout, presentation_layout_generation, util, PresentationRegion, Rect,
    SoftBufferError,
};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Bool, ProtocolObject};
use objc2::{
    define_class, msg_send, AllocAnyThread, AnyThread, DefinedClass, MainThreadMarker, Message,
};
use objc2_core_foundation::{CFRetained, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    kCGColorSpaceSRGB, CGBitmapInfo, CGColorRenderingIntent, CGColorSpace, CGContext,
    CGDataProvider, CGImage, CGImageAlphaInfo, CGImageByteOrderInfo, CGImageComponentInfo,
    CGImagePixelFormatInfo,
};
use objc2_foundation::{
    ns_string, NSDictionary, NSKeyValueChangeKey, NSKeyValueChangeNewKey,
    NSKeyValueObservingOptions, NSNumber, NSObject, NSObjectNSKeyValueObserverRegistration,
    NSObjectProtocol, NSString, NSValue,
};
use objc2_quartz_core::{kCAGravityTopLeft, CALayer, CALayerDelegate, CATransaction};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawWindowHandle};

use std::ffi::c_void;
use std::marker::PhantomData;
use std::mem::size_of;
use std::num::NonZeroU32;
use std::ops::Deref;
use std::ptr::{self, slice_from_raw_parts_mut, NonNull};
use std::sync::Mutex;

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

#[derive(Debug)]
struct BackingStoreIvars {
    state: Mutex<BackingStoreState>,
    color_space: CFRetained<CGColorSpace>,
}

// Delegate used by the optional dirty-backing-store presentation path. Core Animation may invoke
// it after present returns, so both producer and callback synchronize access to persistent pixels.
define_class!(
    #[unsafe(super(NSObject))]
    #[name = "SoftbufferBackingStoreDelegate"]
    #[thread_kind = AnyThread]
    #[ivars = BackingStoreIvars]
    #[derive(Debug)]
    struct BackingStoreDelegate;

    unsafe impl NSObjectProtocol for BackingStoreDelegate {}

    unsafe impl CALayerDelegate for BackingStoreDelegate {
        #[unsafe(method(drawLayer:inContext:))]
        #[allow(non_snake_case)]
        fn drawLayer_inContext(&self, _layer: &CALayer, context: &CGContext) {
            self.draw(context);
        }
    }
);

#[derive(Debug)]
struct BackingStoreState {
    width: usize,
    height: usize,
    scale: f64,
    pixels: Vec<u32>,
}

impl BackingStoreDelegate {
    fn new(
        width: usize,
        height: usize,
        scale: f64,
        color_space: &CFRetained<CGColorSpace>,
    ) -> Result<Retained<Self>, SoftBufferError> {
        let len = width
            .checked_mul(height)
            .ok_or_else(|| size_out_of_range(width, height))?;
        let state = BackingStoreState {
            width,
            height,
            scale,
            pixels: vec![0; len],
        };
        let this = Self::alloc().set_ivars(BackingStoreIvars {
            state: Mutex::new(state),
            color_space: color_space.clone(),
        });
        // SAFETY: NSObject has no subclass initialization requirements and this class has no Drop.
        Ok(unsafe { msg_send![super(this), init] })
    }

    fn update(
        &self,
        source: &[u32],
        source_width: usize,
        tile: &TileLayer,
        regions: &[PixelRect],
    ) -> Result<(), SoftBufferError> {
        let mut state = self.ivars().state.lock().map_err(|_| {
            SoftBufferError::PlatformError(
                Some("CoreAnimation backing-store lock was poisoned".to_string()),
                None,
            )
        })?;
        for &(x, y, width, height) in regions {
            for row in 0..height {
                let source_start = (tile.origin_y + y + row) * source_width + tile.origin_x + x;
                let source_end = source_start + width;
                let target_start = (y + row) * tile.width + x;
                let target_end = target_start + width;
                state.pixels[target_start..target_end]
                    .copy_from_slice(&source[source_start..source_end]);
            }
        }
        Ok(())
    }

    fn draw_debug_overlay(&self, index: usize) -> Result<(), SoftBufferError> {
        let mut state = self.ivars().state.lock().map_err(|_| {
            SoftBufferError::PlatformError(
                Some("CoreAnimation backing-store lock was poisoned".to_string()),
                None,
            )
        })?;
        let width = state.width;
        let height = state.height;
        draw_tile_debug_overlay(&mut state.pixels, width, height, index);
        Ok(())
    }

    fn draw(&self, context: &CGContext) {
        let clip = CGContext::clip_bounding_box(Some(context));
        let Ok(state) = self.ivars().state.lock() else {
            return;
        };
        let Some((x, y, width, height)) =
            layer_clip_to_pixel_rect(clip, state.scale, state.width, state.height)
        else {
            return;
        };
        let y_end = y + height;

        let mut pixels = Vec::with_capacity(width.saturating_mul(height));
        for row in y..y_end {
            let start = row * state.width + x;
            pixels.extend_from_slice(&state.pixels[start..start + width]);
        }
        let scale = state.scale;
        let layer_height = state.height;
        drop(state);

        let Ok(image) = image_from_pixels(width, height, &self.ivars().color_space, pixels) else {
            return;
        };
        let rect = pixel_rect_to_layer_rect((x, y, width, height), scale, layer_height);
        // Core Animation's context clip carries the dirty region, so pixels outside it remain in
        // the layer backing store while this owned image is consumed synchronously.
        CGContext::draw_image(Some(context), rect, Some(&image));
    }
}

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
    /// Optional delegates and immutable backing snapshots for experimental dirty-rect drawing.
    backing_store: bool,
    /// Diagnostic-only tile boundaries and indexes, disabled unless explicitly requested.
    debug_tiles: bool,
    /// Opaque application key selecting the current terminal layout.
    presentation_layout_key: u64,
    /// Generation of the layout used to build `tiles`.
    presentation_layout_generation: u64,
    /// Avoid repeated registry reads during one resize/buffer/present cycle.
    layout_checked: bool,
    window_handle: W,
    _display: PhantomData<D>,
}

impl<D, W> Drop for CGImpl<D, W> {
    fn drop(&mut self) {
        for tile in &self.tiles {
            tile.layer.setDelegate(None);
        }
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
        // SAFETY: `kCGColorSpaceSRGB` is an immutable CoreGraphics constant with static lifetime.
        let color_space =
            CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB })).ok_or_else(|| {
                SoftBufferError::PlatformError(
                    Some("failed to create CoreGraphics sRGB color space".to_string()),
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
        let backing_store = software_backing_store_enabled();
        if backing_store {
            tracing::info!("using experimental CoreAnimation dirty backing store");
        }

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
            backing_store,
            debug_tiles: software_tile_debug_enabled(),
            presentation_layout_key: 0,
            presentation_layout_generation: 0,
            layout_checked: false,
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
        let len = width
            .checked_mul(height)
            .ok_or_else(|| size_out_of_range(width, height))?;
        if self.width == width && self.height == height && self.buffer.len() == len {
            return self.ensure_tile_layout();
        }
        self.width = width;
        self.height = height;
        self.buffer = util::PixelBuffer(vec![0; len]);
        self.buffer_valid = false;
        self.layout_checked = false;
        self.rebuild_tiles()
    }

    fn set_presentation_layout_key(&mut self, key: u64) {
        if self.presentation_layout_key != key {
            self.presentation_layout_key = key;
            self.buffer_valid = false;
            self.layout_checked = false;
        }
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
        self.layout_checked = false;
    }
}

impl<D, W> CGImpl<D, W> {
    fn present_full(&mut self) -> Result<(), SoftBufferError> {
        if !self.layout_checked {
            self.ensure_tile_layout()?;
        }
        let dirty_tiles = vec![true; self.tiles.len()];
        let result = self.present_tiles(&dirty_tiles, None);
        self.layout_checked = false;
        result
    }

    fn present_with_damage(&mut self, damage: &[Rect]) -> Result<(), SoftBufferError> {
        if !self.layout_checked {
            self.ensure_tile_layout()?;
        }
        if !self.buffer_valid {
            return self.present_full();
        }
        let dirty_tiles = self.dirty_tile_mask(damage);
        let result = self.present_tiles(&dirty_tiles, Some(damage));
        self.layout_checked = false;
        result
    }

    fn present_tiles(
        &mut self,
        dirty_tiles: &[bool],
        damage: Option<&[Rect]>,
    ) -> Result<(), SoftBufferError> {
        if self.backing_store {
            return self.present_backing_tiles(dirty_tiles, damage);
        }

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
            let tile = &self.tiles[*index];
            // SAFETY: The contents is `CGImage`, which is a valid class for `contents`.
            unsafe { tile.layer.setContents(Some(image.as_ref())) };
        }
        CATransaction::commit();
        self.buffer_valid = true;
        Ok(())
    }

    fn present_backing_tiles(
        &mut self,
        dirty_tiles: &[bool],
        damage: Option<&[Rect]>,
    ) -> Result<(), SoftBufferError> {
        let source_width = self.width;
        let source = &self.buffer.0;
        let scale = self.tile_scale;
        let mut invalidations = Vec::new();
        for (index, dirty) in dirty_tiles.iter().copied().enumerate() {
            if !dirty {
                continue;
            }
            let tile = &self.tiles[index];
            let pixel_regions = tile_invalidated_pixel_rects(
                tile.origin_x,
                tile.origin_y,
                tile.width,
                tile.height,
                damage,
            );
            let invalidated = pixel_regions
                .iter()
                .copied()
                .map(|rect| pixel_rect_to_layer_rect(rect, scale, tile.height))
                .collect::<Vec<_>>();
            let delegate = tile.delegate.as_ref().ok_or_else(|| {
                SoftBufferError::PlatformError(
                    Some("CoreAnimation backing-store delegate is missing".to_string()),
                    None,
                )
            })?;
            delegate.update(source, source_width, tile, &pixel_regions)?;
            if self.debug_tiles {
                delegate.draw_debug_overlay(index)?;
            }
            invalidations.push((index, invalidated));
        }
        if invalidations.is_empty() {
            return Ok(());
        }

        CATransaction::begin();
        CATransaction::setDisableActions(true);
        for (index, rectangles) in invalidations {
            for rect in rectangles {
                self.tiles[index].layer.setNeedsDisplayInRect(rect);
            }
        }
        CATransaction::commit();
        self.buffer_valid = true;
        Ok(())
    }

    fn rebuild_tiles(&mut self) -> Result<(), SoftBufferError> {
        for tile in &self.tiles {
            tile.layer.setDelegate(None);
            tile.layer.removeFromSuperlayer();
        }
        self.tiles.clear();

        let layout = presentation_layout(self.presentation_layout_key);
        let rectangles = build_tile_rects(
            self.width,
            self.height,
            &layout.regions,
            layout.rows_per_block,
        );
        let tile_count = rectangles.len();
        if tile_count == 0 {
            return Err(SoftBufferError::PlatformError(
                Some("CoreGraphics tile layout is empty".to_string()),
                None,
            ));
        }
        self.tiles.try_reserve(tile_count).map_err(|_| {
            SoftBufferError::PlatformError(
                Some("failed to allocate CoreGraphics tile layout".to_string()),
                None,
            )
        })?;

        let scale = self.layer.contentsScale().max(1.0);
        self.layer.setMasksToBounds(true);
        for (origin_x, origin_y, width, height) in rectangles {
            let tile = CALayer::new();
            tile.setAnchorPoint(CGPoint::new(0.0, 0.0));
            // The parent surface already uses a top-left coordinate system. Flipping this
            // child again reverses the CGImage contents on macOS.
            tile.setContentsGravity(unsafe { kCAGravityTopLeft });
            tile.setContentsScale(scale);
            // The framebuffer and damage rectangles use a top-left origin, while this
            // CoreAnimation layer tree resolves child frames from the bottom edge. Convert
            // the tile's top-left framebuffer coordinate before placing the child layer.
            let frame_y = top_left_to_core_animation_y(self.height, origin_y, height);
            tile.setFrame(CGRect::new(
                CGPoint::new(origin_x as f64 / scale, frame_y as f64 / scale),
                CGSize::new(width as f64 / scale, height as f64 / scale),
            ));
            self.layer.addSublayer(&tile);
            let delegate = if self.backing_store {
                let delegate = BackingStoreDelegate::new(width, height, scale, &self.color_space)?;
                tile.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
                Some(delegate)
            } else {
                None
            };
            self.tiles.push(TileLayer {
                layer: SendCALayer(tile),
                delegate,
                origin_x,
                origin_y,
                width,
                height,
            });
        }
        self.tile_scale = scale;
        self.presentation_layout_generation = layout.generation;
        self.layout_checked = true;
        Ok(())
    }

    fn ensure_tile_layout(&mut self) -> Result<(), SoftBufferError> {
        if self.layout_checked {
            return Ok(());
        }
        let scale = self.layer.contentsScale().max(1.0);
        let generation = presentation_layout_generation(self.presentation_layout_key);
        if scale != self.tile_scale || generation != self.presentation_layout_generation {
            self.rebuild_tiles()?;
            self.buffer_valid = false;
        }
        self.layout_checked = true;
        Ok(())
    }

    fn dirty_tile_mask(&self, damage: &[Rect]) -> Vec<bool> {
        dirty_tile_mask_for_tiles(
            self.tiles
                .iter()
                .map(|tile| (tile.origin_x, tile.origin_y, tile.width, tile.height)),
            damage,
        )
    }
}

fn dirty_tile_mask_for_tiles<I>(tiles: I, damage: &[Rect]) -> Vec<bool>
where
    I: IntoIterator<Item = (usize, usize, usize, usize)>,
{
    tiles
        .into_iter()
        .map(|(origin_x, origin_y, width, height)| {
            damage
                .iter()
                .any(|rect| tile_intersects_rect(rect, origin_x, origin_y, width, height))
        })
        .collect()
}

fn tile_intersects_rect(
    rect: &Rect,
    tile_origin_x: usize,
    tile_origin_y: usize,
    tile_width: usize,
    tile_height: usize,
) -> bool {
    let origin_x = usize::try_from(rect.x).unwrap_or(usize::MAX);
    let origin_y = usize::try_from(rect.y).unwrap_or(usize::MAX);
    let end_x = origin_x.saturating_add(rect.width.get() as usize);
    let end_y = origin_y.saturating_add(rect.height.get() as usize);
    origin_x < tile_origin_x.saturating_add(tile_width)
        && end_x > tile_origin_x
        && origin_y < tile_origin_y.saturating_add(tile_height)
        && end_y > tile_origin_y
}

fn build_tile_rects(
    width: usize,
    height: usize,
    regions: &[PresentationRegion],
    rows_per_block: u32,
) -> Vec<(usize, usize, usize, usize)> {
    let mut regions = regions
        .iter()
        .filter_map(|region| {
            let x = usize::try_from(region.x).ok()?;
            let y = usize::try_from(region.y).ok()?;
            let region_width = usize::try_from(region.width).ok()?;
            let region_height = usize::try_from(region.height).ok()?;
            let row_height = usize::try_from(region.row_height).ok()?.max(1);
            let x_end = x.saturating_add(region_width).min(width);
            let y_end = y.saturating_add(region_height).min(height);
            (x < x_end && y < y_end).then_some((x, y, x_end, y_end, row_height))
        })
        .collect::<Vec<_>>();
    regions
        .sort_unstable_by_key(|&(x, y, x_end, y_end, row_height)| (y, x, y_end, x_end, row_height));
    let mut accepted_regions = Vec::with_capacity(regions.len());
    for region in regions {
        if accepted_regions
            .iter()
            .any(|accepted| pixel_rects_intersect(region_bounds(&region), region_bounds(accepted)))
        {
            continue;
        }
        accepted_regions.push(region);
    }

    let mut rectangles = Vec::new();
    for &(x, y, x_end, y_end, row_height) in &accepted_regions {
        let block_height = row_height
            .saturating_mul(rows_per_block.max(1) as usize)
            .max(1);
        let mut block_y = y;
        while block_y < y_end {
            let block_end = block_y.saturating_add(block_height).min(y_end);
            rectangles.push((x, block_y, x_end - x, block_end - block_y));
            block_y = block_end;
        }
    }

    // Keep fallback tiles independent from terminal row boundaries. Each fixed
    // tile is clipped around whole panes, so sidebar/tab layers never cross a
    // pane and terminal layers stay exact row-height multiples.
    for tile_y in (0..height).step_by(TILE_HEIGHT) {
        let tile_end_y = tile_y.saturating_add(TILE_HEIGHT).min(height);
        for tile_x in (0..width).step_by(TILE_WIDTH) {
            let tile_end_x = tile_x.saturating_add(TILE_WIDTH).min(width);
            let mut pieces = vec![(tile_x, tile_y, tile_end_x - tile_x, tile_end_y - tile_y)];
            for region in &accepted_regions {
                let mut remaining = Vec::with_capacity(pieces.len().saturating_mul(2));
                for piece in pieces {
                    subtract_pixel_rect(piece, region_bounds(region), &mut remaining);
                }
                pieces = remaining;
            }
            rectangles.extend(pieces);
        }
    }
    rectangles
        .sort_unstable_by_key(|&(x, y, rect_width, rect_height)| (y, x, rect_height, rect_width));
    rectangles
}

type PixelRect = (usize, usize, usize, usize);
type TerminalRegion = (usize, usize, usize, usize, usize);

fn region_bounds(region: &TerminalRegion) -> PixelRect {
    (region.0, region.1, region.2 - region.0, region.3 - region.1)
}

fn pixel_rects_intersect(a: PixelRect, b: PixelRect) -> bool {
    let (a_x, a_y, a_width, a_height) = a;
    let (b_x, b_y, b_width, b_height) = b;
    a_x < b_x.saturating_add(b_width)
        && a_x.saturating_add(a_width) > b_x
        && a_y < b_y.saturating_add(b_height)
        && a_y.saturating_add(a_height) > b_y
}

fn subtract_pixel_rect(rect: PixelRect, cut: PixelRect, output: &mut Vec<PixelRect>) {
    if !pixel_rects_intersect(rect, cut) {
        output.push(rect);
        return;
    }
    let (x, y, rect_width, rect_height) = rect;
    let (cut_x, cut_y, cut_width, cut_height) = cut;
    let x_end = x + rect_width;
    let y_end = y + rect_height;
    let cut_x_end = cut_x.saturating_add(cut_width);
    let cut_y_end = cut_y.saturating_add(cut_height);
    let intersection_x = x.max(cut_x);
    let intersection_y = y.max(cut_y);
    let intersection_x_end = x_end.min(cut_x_end);
    let intersection_y_end = y_end.min(cut_y_end);

    if y < intersection_y {
        output.push((x, y, rect_width, intersection_y - y));
    }
    if intersection_y_end < y_end {
        output.push((
            x,
            intersection_y_end,
            rect_width,
            y_end - intersection_y_end,
        ));
    }
    if x < intersection_x {
        output.push((
            x,
            intersection_y,
            intersection_x - x,
            intersection_y_end - intersection_y,
        ));
    }
    if intersection_x_end < x_end {
        output.push((
            intersection_x_end,
            intersection_y,
            x_end - intersection_x_end,
            intersection_y_end - intersection_y,
        ));
    }
}

fn tile_invalidated_pixel_rects(
    tile_origin_x: usize,
    tile_origin_y: usize,
    tile_width: usize,
    tile_height: usize,
    damage: Option<&[Rect]>,
) -> Vec<PixelRect> {
    let Some(damage) = damage else {
        return vec![(0, 0, tile_width, tile_height)];
    };
    damage
        .iter()
        .filter_map(|rect| {
            let rect_x = usize::try_from(rect.x).ok()?;
            let rect_y = usize::try_from(rect.y).ok()?;
            let rect_x_end = rect_x.saturating_add(rect.width.get() as usize);
            let rect_y_end = rect_y.saturating_add(rect.height.get() as usize);
            let tile_x_end = tile_origin_x.saturating_add(tile_width);
            let tile_y_end = tile_origin_y.saturating_add(tile_height);
            let x = rect_x.max(tile_origin_x);
            let y = rect_y.max(tile_origin_y);
            let x_end = rect_x_end.min(tile_x_end);
            let y_end = rect_y_end.min(tile_y_end);
            if x < x_end && y < y_end {
                Some((x - tile_origin_x, y - tile_origin_y, x_end - x, y_end - y))
            } else {
                None
            }
        })
        .collect()
}

fn pixel_rect_to_layer_rect(rect: PixelRect, scale: f64, layer_height: usize) -> CGRect {
    let (x, y, width, height) = rect;
    let scale = scale.max(1.0);
    // The framebuffer uses a top-left origin. Standalone CALayers use the macOS
    // bottom-left coordinate system, even when their parent geometry is flipped.
    let layer_y = if cfg!(target_os = "macos") {
        layer_height.saturating_sub(y.saturating_add(height))
    } else {
        y
    };
    CGRect::new(
        CGPoint::new(x as f64 / scale, layer_y as f64 / scale),
        CGSize::new(width as f64 / scale, height as f64 / scale),
    )
}

fn layer_clip_to_pixel_rect(
    clip: CGRect,
    scale: f64,
    layer_width: usize,
    layer_height: usize,
) -> Option<PixelRect> {
    let scale = scale.max(1.0);
    let clip_x_end = clip.origin.x + clip.size.width;
    let clip_y_end = clip.origin.y + clip.size.height;
    let layer_x0 = (clip.origin.x.min(clip_x_end).max(0.0) * scale).floor() as usize;
    let layer_y0 = (clip.origin.y.min(clip_y_end).max(0.0) * scale).floor() as usize;
    let layer_x1 = (clip.origin.x.max(clip_x_end).max(0.0) * scale).ceil() as usize;
    let layer_y1 = (clip.origin.y.max(clip_y_end).max(0.0) * scale).ceil() as usize;
    let layer_x0 = layer_x0.min(layer_width);
    let layer_y0 = layer_y0.min(layer_height);
    let layer_x1 = layer_x1.min(layer_width);
    let layer_y1 = layer_y1.min(layer_height);
    if layer_x0 >= layer_x1 || layer_y0 >= layer_y1 {
        return None;
    }

    let (pixel_y0, pixel_y1) = if cfg!(target_os = "macos") {
        (
            layer_height.saturating_sub(layer_y1),
            layer_height.saturating_sub(layer_y0),
        )
    } else {
        (layer_y0, layer_y1)
    };
    Some((layer_x0, pixel_y0, layer_x1 - layer_x0, pixel_y1 - pixel_y0))
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
        image_from_pixels(tile.width, tile.height, &self.color_space, pixels)
    }
}

fn image_from_pixels(
    width: usize,
    height: usize,
    color_space: &CGColorSpace,
    pixels: Vec<u32>,
) -> Result<CFRetained<CGImage>, SoftBufferError> {
    let data_provider = owned_data_provider(pixels)?;
    let bitmap_info = CGBitmapInfo(
        CGImageAlphaInfo::NoneSkipFirst.0
            | CGImageComponentInfo::Integer.0
            | CGImageByteOrderInfo::Order32Little.0
            | CGImagePixelFormatInfo::Packed.0,
    );
    unsafe {
        CGImage::new(
            width,
            height,
            8,
            32,
            width * size_of::<u32>(),
            Some(color_space),
            bitmap_info,
            Some(&data_provider),
            ptr::null(),
            false,
            CGColorRenderingIntent::RenderingIntentDefault,
        )
    }
    .ok_or_else(|| {
        SoftBufferError::PlatformError(
            Some("failed to create CoreGraphics image".to_string()),
            None,
        )
    })
}

fn top_left_to_core_animation_y(
    surface_height: usize,
    origin_y: usize,
    tile_height: usize,
) -> usize {
    debug_assert!(origin_y <= surface_height);
    debug_assert!(tile_height <= surface_height - origin_y);
    surface_height - origin_y - tile_height
}

fn software_tile_debug_enabled() -> bool {
    std::env::var("AXSSH_DEBUG_SOFTWARE_TILES").map_or(false, |value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

fn software_backing_store_enabled() -> bool {
    std::env::var("AXSSH_EXPERIMENT_CA_BACKING_STORE").map_or(false, |value| {
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
        draw_tile_debug_glyph(pixels, width, height, glyph_x, 2, usize::from(digit - b'0'));
        glyph_x += 3 * DEBUG_TILE_GLYPH_SCALE + DEBUG_TILE_GLYPH_SCALE;
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
    delegate: Option<Retained<BackingStoreDelegate>>,
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
    unsafe extern "C-unwind" fn release(_info: *mut c_void, data: NonNull<c_void>, size: usize) {
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
    let provider =
        unsafe { CGDataProvider::with_data(ptr::null_mut(), data_ptr, len, Some(release)) };
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
        let tiles = [(0, 0, 256, 128), (256, 128, 256, 128), (512, 128, 88, 128)];
        assert_eq!(
            dirty_tile_mask_for_tiles(tiles, &damage),
            vec![false, true, false]
        );
    }

    #[test]
    fn damage_crossing_tile_edges_marks_each_intersecting_tile() {
        let damage = [Rect {
            x: 255,
            y: 127,
            width: NonZeroU32::new(3).expect("non-zero test width"),
            height: NonZeroU32::new(3).expect("non-zero test height"),
        }];
        let tiles = [
            (0, 0, 256, 128),
            (256, 0, 256, 128),
            (0, 128, 256, 128),
            (256, 128, 256, 128),
            (512, 128, 88, 128),
        ];
        assert_eq!(
            dirty_tile_mask_for_tiles(tiles, &damage),
            vec![true, true, true, true, false]
        );
    }

    #[test]
    fn backing_store_damage_is_clipped_to_tile_local_pixels() {
        let damage = [
            Rect {
                x: 90,
                y: 40,
                width: NonZeroU32::new(20).expect("non-zero test width"),
                height: NonZeroU32::new(20).expect("non-zero test height"),
            },
            Rect {
                x: 250,
                y: 100,
                width: NonZeroU32::new(100).expect("non-zero test width"),
                height: NonZeroU32::new(100).expect("non-zero test height"),
            },
            Rect {
                x: 0,
                y: 0,
                width: NonZeroU32::new(10).expect("non-zero test width"),
                height: NonZeroU32::new(10).expect("non-zero test height"),
            },
        ];

        assert_eq!(
            tile_invalidated_pixel_rects(100, 50, 200, 80, Some(&damage)),
            vec![(0, 0, 10, 10), (150, 50, 50, 30)]
        );
        assert_eq!(
            tile_invalidated_pixel_rects(100, 50, 200, 80, None),
            vec![(0, 0, 200, 80)]
        );
    }

    #[test]
    fn backing_store_damage_converts_top_left_pixels_to_macos_layer_points() {
        assert_eq!(
            pixel_rect_to_layer_rect((6, 8, 20, 10), 2.0, 100),
            CGRect::new(CGPoint::new(3.0, 41.0), CGSize::new(10.0, 5.0))
        );
        assert_eq!(
            pixel_rect_to_layer_rect((0, 0, 200, 100), 2.0, 100),
            CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(100.0, 50.0))
        );
        assert_eq!(
            pixel_rect_to_layer_rect((6, 90, 20, 10), 2.0, 100),
            CGRect::new(CGPoint::new(3.0, 0.0), CGSize::new(10.0, 5.0))
        );
    }

    #[test]
    fn backing_store_layer_clip_round_trips_to_top_left_pixels() {
        let pixels = (6, 8, 20, 10);
        let layer_rect = pixel_rect_to_layer_rect(pixels, 2.0, 100);

        assert_eq!(
            layer_clip_to_pixel_rect(layer_rect, 2.0, 200, 100),
            Some(pixels)
        );
    }

    #[test]
    fn debug_tile_overlay_marks_edges_and_keeps_tile_interior() {
        let width = 96;
        let height = 40;
        let mut pixels = vec![0; width * height];

        draw_tile_debug_overlay(&mut pixels, width, height, 7);

        assert_eq!(pixels[width - 1], DEBUG_TILE_BORDER_COLOR);
        assert_eq!(pixels[(height - 1) * width + 12], DEBUG_TILE_BORDER_COLOR);
        assert_eq!(pixels[24 * width + 80], 0);
        assert_eq!(pixels[2 * width + 4], DEBUG_TILE_LABEL_COLOR);
    }

    #[test]
    fn tile_y_conversion_keeps_top_left_buffer_order() {
        assert_eq!(top_left_to_core_animation_y(300, 0, 128), 172);
        assert_eq!(top_left_to_core_animation_y(300, 128, 128), 44);
        assert_eq!(top_left_to_core_animation_y(300, 256, 44), 0);
    }

    #[test]
    fn configured_regions_partition_the_surface_on_row_blocks() {
        let region = PresentationRegion {
            x: 128,
            y: 33,
            width: 384,
            height: 253,
            row_height: 17,
        };
        let rectangles = build_tile_rects(640, 320, &[region], 4);
        let covered_area = rectangles
            .iter()
            .map(|(_, _, width, height)| width * height)
            .sum::<usize>();
        assert_eq!(covered_area, 640 * 320);
        assert!(rectangles
            .iter()
            .all(|(_, _, width, height)| *width > 0 && *height > 0));
        for (index, rectangle) in rectangles.iter().enumerate() {
            assert!(rectangles[index + 1..]
                .iter()
                .all(|other| !pixel_rects_intersect(*rectangle, *other)));
        }
        assert!(rectangles.windows(2).all(|pair| {
            let (left_x, left_y, _, _) = pair[0];
            let (right_x, right_y, _, _) = pair[1];
            (left_y, left_x) <= (right_y, right_x)
        }));
        let terminal_blocks = rectangles
            .iter()
            .copied()
            .filter(|(x, y, width, _)| *x == 128 && *width == 384 && (33..286).contains(y))
            .collect::<Vec<_>>();
        assert_eq!(
            terminal_blocks,
            vec![
                (128, 33, 384, 68),
                (128, 101, 384, 68),
                (128, 169, 384, 68),
                (128, 237, 384, 49),
            ]
        );
    }

    #[test]
    fn multiple_regions_remain_disjoint_after_clipping_and_overlap_filtering() {
        let regions = [
            PresentationRegion {
                x: 40,
                y: 20,
                width: 200,
                height: 170,
                row_height: 17,
            },
            PresentationRegion {
                x: 220,
                y: 20,
                width: 180,
                height: 170,
                row_height: 17,
            },
            PresentationRegion {
                x: 500,
                y: 180,
                width: 300,
                height: 200,
                row_height: 19,
            },
        ];
        let rectangles = build_tile_rects(640, 320, &regions, 3);
        let covered_area = rectangles
            .iter()
            .map(|(_, _, width, height)| width * height)
            .sum::<usize>();

        assert_eq!(covered_area, 640 * 320);
        for (index, rectangle) in rectangles.iter().enumerate() {
            assert!(rectangles[index + 1..]
                .iter()
                .all(|other| !pixel_rects_intersect(*rectangle, *other)));
        }
        assert!(rectangles
            .iter()
            .any(|&(x, y, width, height)| { x == 40 && y == 20 && width == 200 && height == 51 }));
        assert!(rectangles.iter().any(|&(x, y, width, height)| {
            x == 500 && y == 180 && width == 140 && height == 57
        }));
    }
}

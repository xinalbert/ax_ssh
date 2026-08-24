//! Softbuffer implementation using CoreGraphics.
use crate::backend_interface::*;
use crate::error::InitError;
use crate::{Rect, SoftBufferError};
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

// CoreAnimation has no damage-aware `contents` setter. Keep the software
// framebuffer persistent and expose it through a bounded grid of child layers;
// presenting a dirty region then replaces only the intersecting tile images.
const TILE_WIDTH: usize = 512;
const TILE_HEIGHT: usize = 64;

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
    /// Our layer.
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
    /// Persistent pixels required by software partial rendering. The previous
    /// implementation allocated a new buffer on every frame and therefore
    /// could not preserve clean pixels between damage submissions.
    buffer: Vec<u32>,
    buffer_valid: bool,
    tile_layers: Vec<SendCALayer>,
    tile_columns: usize,
    tile_rows: usize,
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
        let color_space = CGColorSpace::new_device_rgb().unwrap();

        // Grab initial width and height from the layer (whose properties have just been initialized
        // by the observer using `NSKeyValueObservingOptionInitial`).
        let size = layer.bounds().size;
        let scale_factor = layer.contentsScale();
        let width = (size.width * scale_factor) as usize;
        let height = (size.height * scale_factor) as usize;

        Ok(Self {
            layer: SendCALayer(layer),
            root_layer: SendCALayer(root_layer),
            observer,
            color_space,
            width,
            height,
            buffer: Vec::new(),
            buffer_valid: false,
            tile_layers: Vec::new(),
            tile_columns: 0,
            tile_rows: 0,
            _display: PhantomData,
            window_handle: window_src,
        })
    }

    #[inline]
    fn window(&self) -> &W {
        &self.window_handle
    }

    fn resize(&mut self, width: NonZeroU32, height: NonZeroU32) -> Result<(), SoftBufferError> {
        let width = width.get() as usize;
        let height = height.get() as usize;
        if self.width == width && self.height == height && self.buffer.len() == width * height {
            return Ok(());
        }
        self.width = width;
        self.height = height;
        self.buffer = vec![0; width * height];
        self.buffer_valid = false;
        self.configure_tile_layers();
        Ok(())
    }

    fn buffer_mut(&mut self) -> Result<BufferImpl<'_, D, W>, SoftBufferError> {
        if self.buffer.len() != self.width * self.height {
            self.buffer = vec![0; self.width * self.height];
            self.buffer_valid = false;
        }
        Ok(BufferImpl { imp: self })
    }
}

impl<D, W> CGImpl<D, W> {
    fn configure_tile_layers(&mut self) {
        for tile in self.tile_layers.drain(..) {
            tile.0.removeFromSuperlayer();
        }
        let scale_factor = self.layer.contentsScale().max(1.0);
        self.tile_columns = self.width.div_ceil(TILE_WIDTH);
        self.tile_rows = self.height.div_ceil(TILE_HEIGHT);
        for tile_y in 0..self.tile_rows {
            for tile_x in 0..self.tile_columns {
                let tile = CALayer::new();
                tile.setAnchorPoint(CGPoint::new(0.0, 0.0));
                // The parent surface already uses a top-left coordinate system. Flipping this
                // child again reverses the CGImage contents on macOS.
                tile.setContentsScale(scale_factor);
                tile.setContentsGravity(unsafe { kCAGravityTopLeft });
                tile.setFrame(CGRect::new(
                    CGPoint::new(
                        (tile_x * TILE_WIDTH) as f64 / scale_factor,
                        (tile_y * TILE_HEIGHT) as f64 / scale_factor,
                    ),
                    CGSize::new(
                        self.tile_width(tile_x) as f64 / scale_factor,
                        self.tile_height(tile_y) as f64 / scale_factor,
                    ),
                ));
                self.layer.addSublayer(&tile);
                self.tile_layers.push(SendCALayer(tile));
            }
        }
    }

    fn tile_width(&self, tile_x: usize) -> usize {
        TILE_WIDTH.min(self.width.saturating_sub(tile_x * TILE_WIDTH))
    }

    fn tile_height(&self, tile_y: usize) -> usize {
        TILE_HEIGHT.min(self.height.saturating_sub(tile_y * TILE_HEIGHT))
    }

    fn tile_image(
        &self,
        tile_x: usize,
        tile_y: usize,
    ) -> Result<CFRetained<CGImage>, SoftBufferError> {
        let width = self.tile_width(tile_x);
        let height = self.tile_height(tile_y);
        let mut pixels = Vec::with_capacity(width * height);
        for row in 0..height {
            let source_start = (tile_y * TILE_HEIGHT + row) * self.width + tile_x * TILE_WIDTH;
            pixels.extend_from_slice(&self.buffer[source_start..source_start + width]);
        }

        unsafe extern "C-unwind" fn release(
            _info: *mut c_void,
            data: NonNull<c_void>,
            size: usize,
        ) {
            let data = data.cast::<u32>();
            let slice = slice_from_raw_parts_mut(data.as_ptr(), size / size_of::<u32>());
            // SAFETY: This is the same boxed slice passed to the data provider.
            drop(unsafe { Box::from_raw(slice) });
        }

        let len = pixels.len() * size_of::<u32>();
        let data_ptr = Box::into_raw(pixels.into_boxed_slice()).cast::<c_void>();
        let data_provider = match unsafe {
            CGDataProvider::with_data(ptr::null_mut(), data_ptr, len, Some(release))
        } {
            Some(provider) => provider,
            None => {
                // SAFETY: `data_ptr` is the thin pointer produced by
                // `Box::into_raw` above, and `len` is its exact byte length.
                let slice =
                    slice_from_raw_parts_mut(data_ptr.cast::<u32>(), len / size_of::<u32>());
                drop(unsafe { Box::from_raw(slice) });
                return Err(SoftBufferError::PlatformError(
                    Some("failed to create CoreGraphics tile data provider".to_string()),
                    None,
                ));
            }
        };
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
                width * 4,
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

    fn present_tiles(&mut self, damage: &[Rect]) -> Result<(), SoftBufferError> {
        let mut dirty = vec![false; self.tile_layers.len()];
        if !self.buffer_valid || damage.is_empty() {
            dirty.fill(true);
        } else {
            for rect in damage {
                let right = rect
                    .x
                    .checked_add(rect.width.get())
                    .ok_or(SoftBufferError::DamageOutOfRange { rect: *rect })?;
                let bottom = rect
                    .y
                    .checked_add(rect.height.get())
                    .ok_or(SoftBufferError::DamageOutOfRange { rect: *rect })?;
                if right > self.width as u32 || bottom > self.height as u32 {
                    return Err(SoftBufferError::DamageOutOfRange { rect: *rect });
                }
                let first_x = rect.x as usize / TILE_WIDTH;
                let last_x = (right.saturating_sub(1) as usize) / TILE_WIDTH;
                let first_y = rect.y as usize / TILE_HEIGHT;
                let last_y = (bottom.saturating_sub(1) as usize) / TILE_HEIGHT;
                for tile_y in first_y..=last_y {
                    for tile_x in first_x..=last_x {
                        dirty[tile_y * self.tile_columns + tile_x] = true;
                    }
                }
            }
        }

        let updates = dirty
            .into_iter()
            .enumerate()
            .filter_map(|(index, is_dirty)| is_dirty.then_some(index))
            .map(|index| {
                let tile_x = index % self.tile_columns;
                let tile_y = index / self.tile_columns;
                self.tile_image(tile_x, tile_y).map(|image| (index, image))
            })
            .collect::<Result<Vec<_>, _>>()?;

        CATransaction::begin();
        CATransaction::setDisableActions(true);
        for (index, image) in updates {
            // SAFETY: `image` is a valid CGImage object accepted by CALayer.contents.
            unsafe { self.tile_layers[index].0.setContents(Some(image.as_ref())) };
        }
        CATransaction::commit();
        self.buffer_valid = true;
        Ok(())
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
        self.present_with_damage(&[])
    }

    fn present_with_damage(self, damage: &[Rect]) -> Result<(), SoftBufferError> {
        self.imp.present_tiles(damage)
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

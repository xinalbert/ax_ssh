//! Interface implemented by backends

use crate::{DamageSupport, InitError, Rect, SoftBufferError};

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::num::NonZeroU32;

pub(crate) trait ContextInterface<D: HasDisplayHandle + ?Sized> {
    fn new(display: D) -> Result<Self, InitError<D>>
    where
        D: Sized,
        Self: Sized;
}

pub(crate) trait SurfaceInterface<D: HasDisplayHandle + ?Sized, W: HasWindowHandle + ?Sized> {
    type Context: ContextInterface<D>;
    type Buffer<'a>: BufferInterface
    where
        Self: 'a;

    fn new(window: W, context: &Self::Context) -> Result<Self, InitError<W>>
    where
        W: Sized,
        Self: Sized;
    /// Get the inner window handle.
    fn window(&self) -> &W;
    /// Resize the internal buffer to the given width and height.
    fn resize(&mut self, width: NonZeroU32, height: NonZeroU32) -> Result<(), SoftBufferError>;
    /// Select an application-managed software presentation layout.
    ///
    /// Most backends do not need this information and retain the default no-op.
    fn set_presentation_layout_key(&mut self, _key: u64) {}
    /// Mark the surface contents as invalid.
    ///
    /// Backends with persistent buffers must discard their buffer age after a
    /// window is hidden, restored, or otherwise loses its backing surface.
    /// Other backends can keep the default no-op implementation.
    fn invalidate(&mut self) {}
    /// Report how this backend consumes damage rectangles.
    fn damage_support(&self) -> DamageSupport {
        DamageSupport::FullFrame
    }
    /// Get a mutable reference to the buffer.
    fn buffer_mut(&mut self) -> Result<Self::Buffer<'_>, SoftBufferError>;
    /// Fetch the buffer from the window.
    fn fetch(&mut self) -> Result<Vec<u32>, SoftBufferError> {
        Err(SoftBufferError::Unimplemented)
    }
}

pub(crate) trait BufferInterface {
    fn width(&self) -> NonZeroU32;
    fn height(&self) -> NonZeroU32;
    fn pixels(&self) -> &[u32];
    fn pixels_mut(&mut self) -> &mut [u32];
    fn age(&self) -> u8;
    fn present_with_damage(self, damage: &[Rect]) -> Result<(), SoftBufferError>;
    fn present(self) -> Result<(), SoftBufferError>;
}

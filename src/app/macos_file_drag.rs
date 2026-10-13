//! Main-thread AppKit bridge for SFTP remote-file promises.
//!
//! This module deliberately exposes only owned local targets and transfer IDs
//! to the application bridge. It does not know about Slint-generated types or
//! russh handles; the supplied queue closure returns after it has handed an
//! owned request to the existing SFTP worker.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use anyhow::{Context, Result};
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{NSObjectProtocol, ProtocolObject};
use objc2::{AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSApplication, NSDragOperation, NSDraggingContext, NSDraggingDestination, NSDraggingInfo,
    NSDraggingItem, NSDraggingSession, NSDraggingSource, NSEvent, NSEventType,
    NSFilePromiseProvider, NSFilePromiseProviderDelegate, NSFilePromiseReceiver, NSView,
};
use objc2_foundation::{
    NSArray, NSError, NSObject, NSOperationQueue, NSPoint, NSRect, NSSize, NSString, NSURL,
};
use uuid::Uuid;

use super::macos_window;

const FILE_PROMISE_UTI: &str = "public.data";
const FILE_PROMISE_ERROR_DOMAIN: &str = "AxSSHSftpFilePromise";
const MAX_FILE_NAME_CHARS: usize = 512;
const MAX_NATIVE_DRAGS: usize = 64;
const MAX_MOUSE_DOWN_WINDOWS: usize = 64;

type PromiseCompletion = RcBlock<dyn Fn(*mut NSError)>;

/// A promise belongs to one connection attempt, including across reconnects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct FilePromiseOwner {
    pub(super) tab_id: Uuid,
    pub(super) attempt_id: Uuid,
}

/// Ensure a stopped or aborted monitor cannot leave Finder waiting forever.
pub(super) struct FilePromiseAttemptGuard {
    owner: FilePromiseOwner,
    cancelled: bool,
}

impl FilePromiseAttemptGuard {
    pub(super) fn new(tab_id: Uuid, attempt_id: Uuid) -> Self {
        Self {
            owner: FilePromiseOwner { tab_id, attempt_id },
            cancelled: false,
        }
    }

    pub(super) fn cancel(&mut self) {
        if !self.cancelled {
            self.cancelled = true;
            cancel_file_promise_attempt(self.owner);
        }
    }
}

impl Drop for FilePromiseAttemptGuard {
    fn drop(&mut self) {
        self.cancel();
    }
}

struct CapturedMouseDown<T> {
    window_number: isize,
    event: T,
}

struct MouseDownEvents<T> {
    by_window: HashMap<Uuid, CapturedMouseDown<T>>,
}

impl<T> Default for MouseDownEvents<T> {
    fn default() -> Self {
        Self {
            by_window: HashMap::new(),
        }
    }
}

impl<T> MouseDownEvents<T> {
    fn record(
        &mut self,
        window_id: Uuid,
        event_type: NSEventType,
        event_window: isize,
        native_window: isize,
        event: T,
    ) -> Result<()> {
        self.clear(window_id);
        if event_type != NSEventType::LeftMouseDown
            || event_window <= 0
            || event_window != native_window
        {
            anyhow::bail!("native drag requires a left mouse-down event from this window");
        }
        if self.by_window.len() >= MAX_MOUSE_DOWN_WINDOWS {
            anyhow::bail!("too many windows hold native mouse-down events");
        }
        self.by_window.insert(
            window_id,
            CapturedMouseDown {
                window_number: event_window,
                event,
            },
        );
        Ok(())
    }

    fn clear(&mut self, window_id: Uuid) {
        self.by_window.remove(&window_id);
    }

    fn take(&mut self, window_id: Uuid, native_window: isize, left_pressed: bool) -> Result<T> {
        let captured = self
            .by_window
            .remove(&window_id)
            .context("native drag has no active mouse-down event")?;
        if !left_pressed || captured.window_number != native_window {
            anyhow::bail!("native drag's mouse gesture has ended or changed windows");
        }
        Ok(captured.event)
    }
}

/// Called before Slint handles the Winit press, while AppKit still owns it.
pub(super) fn capture_file_drag_mouse_down(window: &slint::Window, window_id: Uuid) -> Result<()> {
    clear_file_drag_mouse_down(window_id);
    let mtm = MainThreadMarker::new().context("native mouse capture requires the main thread")?;
    let event = NSApplication::sharedApplication(mtm)
        .currentEvent()
        .context("macOS did not provide a mouse-down event")?;
    macos_window::with_native_view(window, |view| {
        let native_window = view.window().context("native drag window is unavailable")?;
        MOUSE_DOWN_EVENTS.with(|events| {
            events.borrow_mut().record(
                window_id,
                event.r#type(),
                event.windowNumber(),
                native_window.windowNumber(),
                event,
            )
        })
    })
}

pub(super) fn clear_file_drag_mouse_down(window_id: Uuid) {
    MOUSE_DOWN_EVENTS.with(|events| events.borrow_mut().clear(window_id));
}

struct PromiseRequest {
    owner: FilePromiseOwner,
    transfer_id: Option<Uuid>,
    completion: Option<PromiseCompletion>,
}

impl PromiseRequest {
    fn new(owner: FilePromiseOwner) -> Self {
        Self {
            owner,
            transfer_id: None,
            completion: None,
        }
    }

    fn start(
        &mut self,
        transfer_id: Uuid,
        completion: &mut Option<PromiseCompletion>,
    ) -> Result<()> {
        if self.transfer_id.is_some() {
            anyhow::bail!("native file promise was already requested");
        }
        self.transfer_id = Some(transfer_id);
        self.completion = completion.take();
        Ok(())
    }

    fn finish(&mut self, succeeded: bool) {
        if let Some(completion) = self.completion.take() {
            call_promise_completion(&completion, succeeded);
        }
    }
}

impl Drop for PromiseRequest {
    fn drop(&mut self) {
        self.finish(false);
    }
}

/// Logical coordinates of the Local files target, relative to the Slint
/// content area. This is an owned UI DTO; AppKit receives only the resulting
/// clipped native frame and never sees generated Slint types.
#[derive(Clone, Copy, Debug)]
pub(super) struct NativeDropRegion {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl NativeDropRegion {
    pub(super) fn from_logical(x: f32, y: f32, width: f32, height: f32) -> Option<Self> {
        let region = Self {
            x: f64::from(x),
            y: f64::from(y),
            width: f64::from(width),
            height: f64::from(height),
        };
        (region.x.is_finite()
            && region.y.is_finite()
            && region.width.is_finite()
            && region.height.is_finite()
            && region.x >= 0.0
            && region.y >= 0.0
            && region.width > 0.0
            && region.height > 0.0)
            .then_some(region)
    }

    fn frame_in_view(self, view: &NSView) -> Option<NSRect> {
        let bounds = view.bounds();
        let max_x = (self.x + self.width).min(bounds.size.width);
        let max_y = (self.y + self.height).min(bounds.size.height);
        let min_x = self.x.max(0.0);
        let min_y = self.y.max(0.0);
        if max_x <= min_x || max_y <= min_y {
            return None;
        }
        let y = if view.isFlipped() {
            bounds.origin.y + min_y
        } else {
            bounds.origin.y + bounds.size.height - max_y
        };
        Some(NSRect::new(
            NSPoint::new(bounds.origin.x + min_x, y),
            NSSize::new(max_x - min_x, max_y - min_y),
        ))
    }
}

#[derive(Default)]
struct PromiseTransferRegistry {
    drag_by_transfer: HashMap<Uuid, (Uuid, FilePromiseOwner)>,
}

impl PromiseTransferRegistry {
    fn register(&mut self, transfer_id: Uuid, drag_id: Uuid, owner: FilePromiseOwner) {
        self.drag_by_transfer.insert(transfer_id, (drag_id, owner));
    }

    fn take_completed_drag(&mut self, transfer_id: Uuid) -> Option<Uuid> {
        self.drag_by_transfer
            .remove(&transfer_id)
            .map(|(drag_id, _)| drag_id)
    }

    fn remove_drag_transfer(&mut self, transfer_id: Uuid) {
        self.drag_by_transfer.remove(&transfer_id);
    }

    fn remove_attempt(&mut self, owner: FilePromiseOwner) {
        self.drag_by_transfer
            .retain(|_, (_, current)| *current != owner);
    }
}

struct NativeFilePromiseDrag {
    file_name: String,
    local_target: PathBuf,
    queue_download: Box<dyn Fn(Uuid, PathBuf) -> Result<()>>,
    request: PromiseRequest,
    // AppKit's provider delegate is weak. Retain all native objects for the
    // life of the drag / promised transfer so their callbacks remain valid.
    _provider: Retained<NSFilePromiseProvider>,
    _delegate: Retained<NativeFilePromiseDelegate>,
    _source: Retained<NativeFilePromiseDragSource>,
    destination: Option<Retained<NativeFilePromiseDropDestination>>,
}

thread_local! {
    static ACTIVE_DRAGS: RefCell<HashMap<Uuid, NativeFilePromiseDrag>> = RefCell::new(HashMap::new());
    static MOUSE_DOWN_EVENTS: RefCell<MouseDownEvents<Retained<NSEvent>>> = RefCell::default();
}

fn promised_transfers() -> &'static Mutex<PromiseTransferRegistry> {
    static TRANSFERS: OnceLock<Mutex<PromiseTransferRegistry>> = OnceLock::new();
    TRANSFERS.get_or_init(|| Mutex::new(PromiseTransferRegistry::default()))
}

struct NativeFilePromiseDelegateIvars {
    drag_id: Uuid,
}

define_class!(
    // SAFETY: NSObject has no additional subclassing invariants. The class is
    // main-thread-only because all access to the native drag registry is on
    // AppKit's main operation queue.
    #[unsafe(super(NSObject))]
    #[name = "AxSSHSftpFilePromiseDelegate"]
    #[thread_kind = MainThreadOnly]
    #[ivars = NativeFilePromiseDelegateIvars]
    struct NativeFilePromiseDelegate;

    // SAFETY: NSObjectProtocol has no extra implementation requirements.
    unsafe impl NSObjectProtocol for NativeFilePromiseDelegate {}

    // SAFETY: The provider delegate only reads the per-drag ID and schedules
    // its download through the main-thread registry. The operation queue is
    // explicitly constrained to NSOperationQueue::mainQueue below.
    #[allow(non_snake_case)]
    unsafe impl NSFilePromiseProviderDelegate for NativeFilePromiseDelegate {
        #[unsafe(method_id(filePromiseProvider:fileNameForType:))]
        fn filePromiseProvider_fileNameForType(
            &self,
            _provider: &NSFilePromiseProvider,
            _file_type: &NSString,
        ) -> Retained<NSString> {
            file_name_for_drag(self.ivars().drag_id)
        }

        #[unsafe(method(filePromiseProvider:writePromiseToURL:completionHandler:))]
        fn filePromiseProvider_writePromiseToURL_completionHandler(
            &self,
            _provider: &NSFilePromiseProvider,
            url: &NSURL,
            completion_handler: &block2::DynBlock<dyn Fn(*mut NSError)>,
        ) {
            fulfill_file_promise(
                self.ivars().drag_id,
                file_promise_target(url),
                completion_handler.copy(),
            );
        }

        #[unsafe(method_id(operationQueueForFilePromiseProvider:))]
        fn operationQueueForFilePromiseProvider(
            &self,
            _provider: &NSFilePromiseProvider,
        ) -> Retained<NSOperationQueue> {
            NSOperationQueue::mainQueue()
        }
    }
);

impl NativeFilePromiseDelegate {
    fn new(mtm: MainThreadMarker, drag_id: Uuid) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(NativeFilePromiseDelegateIvars { drag_id });
        // SAFETY: `this` is a newly allocated NSObject subclass and the
        // selector exactly matches NSObject's designated initializer.
        unsafe { msg_send![super(this), init] }
    }
}

struct NativeFilePromiseDragSourceIvars {
    drag_id: Uuid,
}

define_class!(
    // SAFETY: NSObject has no additional subclassing invariants. Its only
    // callback removes the matching main-thread registry entry on cancellation.
    #[unsafe(super(NSObject))]
    #[name = "AxSSHSftpFilePromiseDragSource"]
    #[thread_kind = MainThreadOnly]
    #[ivars = NativeFilePromiseDragSourceIvars]
    struct NativeFilePromiseDragSource;

    // SAFETY: NSObjectProtocol has no extra implementation requirements.
    unsafe impl NSObjectProtocol for NativeFilePromiseDragSource {}

    // SAFETY: This source offers copy-only semantics and never mutates a
    // remote or local file during the AppKit drag lifecycle.
    #[allow(non_snake_case)]
    unsafe impl NSDraggingSource for NativeFilePromiseDragSource {
        #[unsafe(method(draggingSession:sourceOperationMaskForDraggingContext:))]
        fn draggingSession_sourceOperationMaskForDraggingContext(
            &self,
            _session: &NSDraggingSession,
            _context: NSDraggingContext,
        ) -> NSDragOperation {
            NSDragOperation::Copy
        }

        #[unsafe(method(draggingSession:endedAtPoint:operation:))]
        fn draggingSession_endedAtPoint_operation(
            &self,
            _session: &NSDraggingSession,
            _screen_point: NSPoint,
            operation: NSDragOperation,
        ) {
            if operation == NSDragOperation::None {
                complete_drag(self.ivars().drag_id, false);
            }
        }
    }
);

impl NativeFilePromiseDragSource {
    fn new(mtm: MainThreadMarker, drag_id: Uuid) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(NativeFilePromiseDragSourceIvars { drag_id });
        // SAFETY: `this` is a newly allocated NSObject subclass and the
        // selector exactly matches NSObject's designated initializer.
        unsafe { msg_send![super(this), init] }
    }
}

struct NativeFilePromiseDropDestinationIvars {
    drag_id: Uuid,
}

define_class!(
    // SAFETY: The destination is a transparent NSView limited to the Local
    // files frame for one active drag. It accepts only its matching source.
    #[unsafe(super(NSView))]
    #[name = "AxSSHSftpFilePromiseDropDestination"]
    #[thread_kind = MainThreadOnly]
    #[ivars = NativeFilePromiseDropDestinationIvars]
    struct NativeFilePromiseDropDestination;

    // SAFETY: NSObjectProtocol has no extra implementation requirements.
    unsafe impl NSObjectProtocol for NativeFilePromiseDropDestination {}

    // SAFETY: Every accepted operation is restricted to the exact native
    // source created for this drag ID, then queues an owned local target.
    #[allow(non_snake_case)]
    unsafe impl NSDraggingDestination for NativeFilePromiseDropDestination {
        #[unsafe(method(draggingEntered:))]
        fn draggingEntered(&self, sender: &ProtocolObject<dyn NSDraggingInfo>) -> NSDragOperation {
            if sender_matches_drag(sender, self.ivars().drag_id) {
                NSDragOperation::Copy
            } else {
                NSDragOperation::None
            }
        }

        #[unsafe(method(draggingUpdated:))]
        fn draggingUpdated(&self, sender: &ProtocolObject<dyn NSDraggingInfo>) -> NSDragOperation {
            if sender_matches_drag(sender, self.ivars().drag_id) {
                NSDragOperation::Copy
            } else {
                NSDragOperation::None
            }
        }

        #[unsafe(method(prepareForDragOperation:))]
        fn prepareForDragOperation(&self, sender: &ProtocolObject<dyn NSDraggingInfo>) -> bool {
            sender_matches_drag(sender, self.ivars().drag_id)
        }

        #[unsafe(method(performDragOperation:))]
        fn performDragOperation(&self, sender: &ProtocolObject<dyn NSDraggingInfo>) -> bool {
            if !sender_matches_drag(sender, self.ivars().drag_id) {
                return false.into();
            }
            let local_target = ACTIVE_DRAGS.with(|drags| {
                drags
                    .borrow()
                    .get(&self.ivars().drag_id)
                    .map(|drag| drag.local_target.clone())
            });
            let Some(local_target) = local_target else {
                return false.into();
            };
            queue_drag_download(self.ivars().drag_id, local_target, &mut None).is_ok()
        }
    }
);

impl NativeFilePromiseDropDestination {
    fn new(mtm: MainThreadMarker, drag_id: Uuid, frame: NSRect) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(NativeFilePromiseDropDestinationIvars { drag_id });
        // SAFETY: `this` is a newly allocated NSView subclass and the
        // initializer signature is NSView's designated frame initializer.
        unsafe { msg_send![super(this), initWithFrame: frame] }
    }
}

/// Begin a copy-only native drag for one remote regular file.
///
/// `queue_download` receives either Finder's promised file path or the
/// captured Local files destination when the drag returns to the declared
/// local target. `local_drop_region` is absent when that target is unavailable;
/// Finder destinations still work, while a return to AxSSH is rejected.
/// The closure must enqueue only an owned SFTP worker request and return the
/// transfer ID.
pub(super) fn begin_file_promise_drag(
    window: &slint::Window,
    window_id: Uuid,
    owner: FilePromiseOwner,
    file_name: String,
    local_target: PathBuf,
    local_drop_region: Option<NativeDropRegion>,
    queue_download: impl Fn(Uuid, PathBuf) -> Result<()> + 'static,
) -> Result<()> {
    validate_file_name(&file_name)?;
    let mtm =
        MainThreadMarker::new().context("macOS native file drag must start on the main thread")?;
    if ACTIVE_DRAGS.with(|drags| drags.borrow().len() >= MAX_NATIVE_DRAGS) {
        anyhow::bail!("too many native file promises are pending");
    }
    let drag_id = Uuid::new_v4();

    macos_window::with_native_view(window, |view| {
        let native_window = view.window().context("native drag window is unavailable")?;
        let event = MOUSE_DOWN_EVENTS.with(|events| {
            events.borrow_mut().take(
                window_id,
                native_window.windowNumber(),
                NSEvent::pressedMouseButtons() & 1 != 0,
            )
        })?;
        let delegate = NativeFilePromiseDelegate::new(mtm, drag_id);
        let file_type = NSString::from_str(FILE_PROMISE_UTI);
        let provider = NSFilePromiseProvider::initWithFileType_delegate(
            NSFilePromiseProvider::alloc(),
            &file_type,
            ProtocolObject::from_ref(&*delegate),
        );
        let item = NSDraggingItem::initWithPasteboardWriter(
            NSDraggingItem::alloc(),
            ProtocolObject::from_ref(&*provider),
        );
        let origin = view.convertPoint_fromView(event.locationInWindow(), None);
        item.setDraggingFrame(NSRect::new(origin, NSSize::new(48.0, 24.0)));
        let source = NativeFilePromiseDragSource::new(mtm, drag_id);
        let destination = local_drop_region.and_then(|region| {
            let frame = region.frame_in_view(view)?;
            let destination = NativeFilePromiseDropDestination::new(mtm, drag_id, frame);
            destination.registerForDraggedTypes(&NSFilePromiseReceiver::readableDraggedTypes());
            view.addSubview(&destination);
            Some(destination)
        });

        ACTIVE_DRAGS.with(|drags| {
            drags.borrow_mut().insert(
                drag_id,
                NativeFilePromiseDrag {
                    file_name,
                    local_target,
                    queue_download: Box::new(queue_download),
                    request: PromiseRequest::new(owner),
                    _provider: provider,
                    _delegate: delegate,
                    _source: source.clone(),
                    destination,
                },
            );
        });

        let items = NSArray::from_retained_slice(&[item]);
        let session = view.beginDraggingSessionWithItems_event_source(
            &items,
            &event,
            ProtocolObject::from_ref(&*source),
        );
        session.setAnimatesToStartingPositionsOnCancelOrFail(true);
        Ok(())
    })
}

/// Remove only promises from the retired attempt; newer connections survive.
fn cancel_file_promise_attempt(owner: FilePromiseOwner) {
    if let Ok(mut transfers) = promised_transfers().lock() {
        transfers.remove_attempt(owner);
    }
    if slint::invoke_from_event_loop(move || {
        let drag_ids = ACTIVE_DRAGS.with(|drags| {
            drags
                .borrow()
                .iter()
                .filter(|(_, drag)| drag.request.owner == owner)
                .map(|(drag_id, _)| *drag_id)
                .collect::<Vec<_>>()
        });
        for drag_id in drag_ids {
            complete_drag(drag_id, false);
        }
    })
    .is_err()
    {
        tracing::debug!(target: "ax_ssh::sftp_drag", stage = "shutdown-ui-unavailable",
            "SFTP native promise event loop has stopped");
    }
}

/// Return true when a SFTP terminal event was consumed by a native promise.
///
/// This function can be called from a Tokio monitor. It removes the transfer
/// ID under a synchronous lock and returns the AppKit callback to Slint's UI
/// event loop, never exposing native objects across the task boundary.
pub(super) fn complete_file_promise_transfer(transfer_id: Uuid, succeeded: bool) -> bool {
    let Ok(mut transfers) = promised_transfers().lock() else {
        return false;
    };
    let Some(drag_id) = transfers.take_completed_drag(transfer_id) else {
        return false;
    };
    drop(transfers);
    if slint::invoke_from_event_loop(move || complete_drag(drag_id, succeeded)).is_err() {
        // The application event loop is shutting down; AppKit will release the
        // remaining drag objects with the process. Do not touch UI from here.
    }
    true
}

fn validate_file_name(file_name: &str) -> Result<()> {
    if file_name.is_empty()
        || file_name == "."
        || file_name == ".."
        || file_name.contains(['/', '\\'])
        || file_name.chars().any(char::is_control)
        || file_name.chars().count() > MAX_FILE_NAME_CHARS
    {
        anyhow::bail!("remote file name cannot be represented by a native drag")
    }
    Ok(())
}

fn file_name_for_drag(drag_id: Uuid) -> Retained<NSString> {
    let file_name = ACTIVE_DRAGS.with(|drags| {
        drags
            .borrow()
            .get(&drag_id)
            .map(|drag| drag.file_name.clone())
            .unwrap_or_else(|| "remote-file".to_owned())
    });
    NSString::from_str(&file_name)
}

fn sender_matches_drag(sender: &ProtocolObject<dyn NSDraggingInfo>, drag_id: Uuid) -> bool {
    sender
        .draggingSource()
        .as_deref()
        .and_then(|source| source.downcast_ref::<NativeFilePromiseDragSource>())
        .is_some_and(|source| source.ivars().drag_id == drag_id)
}

fn queue_drag_download(
    drag_id: Uuid,
    local_target: PathBuf,
    completion: &mut Option<PromiseCompletion>,
) -> Result<()> {
    let transfer_id = Uuid::new_v4();
    let owner = ACTIVE_DRAGS.with(|drags| {
        let mut drags = drags.borrow_mut();
        let drag = drags
            .get_mut(&drag_id)
            .context("native file drag is no longer active")?;
        drag.request.start(transfer_id, completion)?;
        Ok::<_, anyhow::Error>(drag.request.owner)
    })?;
    {
        let Ok(mut transfers) = promised_transfers().lock() else {
            complete_drag(drag_id, false);
            anyhow::bail!("native file promise state is unavailable");
        };
        transfers.register(transfer_id, drag_id, owner);
    }
    let queued = ACTIVE_DRAGS.with(|drags| {
        let drags = drags.borrow();
        let drag = drags
            .get(&drag_id)
            .context("native file drag is no longer active")?;
        (drag.queue_download)(transfer_id, local_target)
    });
    if let Err(error) = queued {
        complete_drag(drag_id, false);
        return Err(error);
    }
    remove_drop_destination(drag_id);
    Ok(())
}

fn file_promise_target(url: &NSURL) -> Result<PathBuf> {
    if !url.isFileURL() {
        anyhow::bail!("native file promise destination is not a file URL");
    }
    let path = url
        .path()
        .context("native file promise destination has no local path")?;
    let path = PathBuf::from(path.to_string());
    if !path.is_absolute() {
        anyhow::bail!("native file promise destination is not absolute");
    }
    Ok(path)
}

fn fulfill_file_promise(
    drag_id: Uuid,
    local_target: Result<PathBuf>,
    completion: PromiseCompletion,
) {
    let mut completion = Some(completion);
    let result =
        local_target.and_then(|target| queue_drag_download(drag_id, target, &mut completion));
    if result.is_err() {
        // Before registration the caller still owns this callback. After it,
        // PromiseRequest owns completion and queue errors finish that request.
        if let Some(completion) = completion {
            call_promise_completion(&completion, false);
        }
        let unrequested = ACTIVE_DRAGS.with(|drags| {
            drags
                .borrow()
                .get(&drag_id)
                .is_some_and(|drag| drag.request.transfer_id.is_none())
        });
        if unrequested {
            complete_drag(drag_id, false);
        }
        tracing::debug!(target: "ax_ssh::sftp_drag", stage = "promise-request-rejected",
            "SFTP native promise request rejected");
    }
}

fn remove_drop_destination(drag_id: Uuid) {
    ACTIVE_DRAGS.with(|drags| {
        if let Some(drag) = drags.borrow().get(&drag_id)
            && let Some(destination) = &drag.destination
        {
            destination.removeFromSuperview();
        }
    });
}

fn complete_drag(drag_id: Uuid, succeeded: bool) {
    let drag = remove_drag(drag_id);
    let Some(mut drag) = drag else {
        return;
    };
    if let Some(destination) = &drag.destination {
        destination.removeFromSuperview();
    }
    drag.request.finish(succeeded);
}

fn call_promise_completion(completion: &PromiseCompletion, succeeded: bool) {
    if succeeded {
        completion.call((std::ptr::null_mut(),));
    } else {
        let domain = NSString::from_str(FILE_PROMISE_ERROR_DOMAIN);
        // SAFETY: The fixed domain and nil user-info meet NSError's Objective-C
        // contract. The retained error remains live for the synchronous block
        // invocation below.
        let error = unsafe { NSError::errorWithDomain_code_userInfo(&domain, 1, None) };
        completion.call((Retained::as_ptr(&error).cast_mut(),));
    }
}

fn remove_drag(drag_id: Uuid) -> Option<NativeFilePromiseDrag> {
    let drag = ACTIVE_DRAGS.with(|drags| drags.borrow_mut().remove(&drag_id));
    if let Some(transfer_id) = drag.as_ref().and_then(|drag| drag.request.transfer_id)
        && let Ok(mut transfers) = promised_transfers().lock()
    {
        transfers.remove_drag_transfer(transfer_id);
    }
    drag
}

#[cfg(test)]
mod tests;

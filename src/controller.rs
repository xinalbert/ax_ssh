//! Shared terminal control, presentation state, and explicit worker cleanup.

mod connection;
mod input;
mod output;
mod panes;
mod presentation;
pub mod semantic;
mod session_labels;
#[cfg(feature = "egui-ui")]
mod sftp_session;
mod shutdown;
#[cfg(feature = "egui-ui")]
mod terminal_session;
mod text_colors;
mod viewport;
#[cfg(feature = "egui-ui")]
mod workspace;

pub use connection::{ConnectionIdentity, ReconnectState};
pub use input::{MAX_MOUSE_WHEEL_REPORTS, encode_terminal_mouse_input};
pub use output::TerminalOutput;
pub use panes::{
    MAX_TERMINAL_PANES, PaneCommand, PaneDirection, PaneDividerPlacement, PaneLayout,
    PanePlacement, PaneTree,
};
use presentation::TerminalFrames;
pub use session_labels::profile_sidebar_endpoint;
#[cfg(feature = "egui-ui")]
pub use sftp_session::{
    LocalEntryView, MAX_SFTP_DOWNLOAD_ROOTS, MAX_SFTP_TRANSFER_BATCH, MAX_SFTP_UPLOAD_ROOTS,
    SftpClient, SftpDownloadBatchResult, SftpDownloadFailure, SftpEntryAction, SftpEntryView,
    SftpIcon, SftpSession, SftpTransferBatchFailure, SftpTransferBatchResult, SftpTransferControl,
    SftpTransferPhase, SftpTransferView, SftpView,
};
pub use shutdown::WorkerShutdowns;
#[cfg(feature = "egui-ui")]
pub use terminal_session::{
    SelectionKind, TerminalClient, TerminalConnectionOptions, TerminalPrompt, TerminalPromptKind,
    TerminalSelection, TerminalSession, TerminalView,
};
pub use text_colors::adjust_terminal_text_rgb;
pub use viewport::TerminalViewport;
#[cfg(feature = "egui-ui")]
pub use workspace::{
    CatalogAction, CatalogResult, Workspace, WorkspaceAction, WorkspaceClient, WorkspacePaneView,
    WorkspaceTabView, WorkspaceView,
};

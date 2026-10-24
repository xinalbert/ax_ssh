//! Shared terminal control, presentation state, and explicit worker cleanup.

mod connection;
mod input;
mod output;
mod panes;
mod presentation;
#[cfg(feature = "egui-ui")]
mod sftp_session;
mod shutdown;
#[cfg(feature = "egui-ui")]
mod terminal_session;
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
#[cfg(feature = "egui-ui")]
pub use sftp_session::{
    LocalEntryView, SftpClient, SftpEntryView, SftpIcon, SftpSession, SftpTransferView, SftpView,
};
pub use shutdown::WorkerShutdowns;
#[cfg(feature = "egui-ui")]
pub use terminal_session::{
    SelectionKind, TerminalClient, TerminalPrompt, TerminalPromptKind, TerminalSelection,
    TerminalSession, TerminalView,
};
pub use viewport::TerminalViewport;
#[cfg(feature = "egui-ui")]
pub use workspace::{
    CatalogAction, CatalogResult, Workspace, WorkspaceAction, WorkspaceClient, WorkspacePaneView,
    WorkspaceTabView, WorkspaceView,
};

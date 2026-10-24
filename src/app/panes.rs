//! Default frontend adapter to the shared, UI-independent pane tree.

pub(super) use ax_ssh::controller::{
    MAX_TERMINAL_PANES, PaneCommand, PaneDirection, PaneDividerPlacement, PaneLayout,
    PanePlacement, PaneTree,
};

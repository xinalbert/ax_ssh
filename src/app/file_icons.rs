//! Default frontend adapter for the shared platform file-icon service.

pub(super) use ax_ssh::file_icons::{
    FileIcon, FileIconKey, clear_global_cache, global_provider, prewarm_async,
};

//! Default frontend adapter for shared bounded upload discovery.
pub(in crate::app) use ax_ssh::local_upload::{
    MAX_LOCAL_UPLOAD_FILES, PreparedLocalUpload, discover_local_uploads,
};

//! Default-frontend adapter for the shared platform identity service.

#[cfg(target_os = "linux")]
pub(super) use ax_ssh::system_icons::discover_local_platform;
pub(super) use ax_ssh::system_icons::{local_platform, platform_for_release};

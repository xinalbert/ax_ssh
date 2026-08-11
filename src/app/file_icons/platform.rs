use super::*;

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
mod fallback;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
pub(super) use self::fallback::Resolver;
#[cfg(target_os = "linux")]
pub(super) use self::linux::Resolver;
#[cfg(target_os = "macos")]
pub(super) use self::macos::Resolver;
#[cfg(windows)]
pub(super) use self::windows::Resolver;

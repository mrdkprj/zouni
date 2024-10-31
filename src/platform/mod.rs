#[cfg(target_os = "linux")]
#[path = "gio.rs"]
pub(crate) mod platform_impl;
#[cfg(target_os = "windows")]
#[path = "win.rs"]
pub(crate) mod platform_impl;

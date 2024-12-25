mod platform;
#[cfg(target_os = "linux")]
pub use platform::linux::*;
#[cfg(target_os = "windows")]
pub use platform::windows::*;

#[derive(Debug, Clone)]
pub struct Volume {
    pub mount_point: String,
    pub volume_label: String,
}

#[derive(Debug, Clone)]
pub struct FileAttribute {
    pub directory: bool,
    pub read_only: bool,
    pub hidden: bool,
    pub system: bool,
    pub device: bool,
    pub ctime: f64,
    pub mtime: f64,
    pub atime: f64,
    pub size: u64,
}

#[derive(Debug, Clone)]
pub enum Operation {
    None,
    Copy,
    Move,
}

#[derive(Debug, Clone)]
pub struct ClipboardData {
    pub operation: Operation,
    pub urls: Vec<String>,
}

mod platform;
#[cfg(target_os = "linux")]
pub use platform::linux::*;
#[cfg(target_os = "windows")]
pub use platform::windows::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Volume {
    pub mount_point: String,
    pub volume_label: String,
    pub available_units: u64,
    pub total_units: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAttribute {
    pub is_directory: bool,
    pub is_read_only: bool,
    pub is_hidden: bool,
    pub is_system: bool,
    pub is_device: bool,
    pub is_symbolic_link: bool,
    pub is_file: bool,
    pub ctime_ms: f64,
    pub mtime_ms: f64,
    pub atime_ms: f64,
    pub birthtime_ms: f64,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Operation {
    None,
    Copy,
    Move,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardData {
    pub operation: Operation,
    pub urls: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dirent {
    pub name: String,
    pub parent_path: String,
    pub full_path: String,
    pub attributes: FileAttribute,
    pub mime_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppInfo {
    pub path: String,
    pub name: String,
    pub icon: String,
}

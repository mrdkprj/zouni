use crate::{ClipboardData, FileAttribute, Operation, Volume};
use gio::{
    glib::DateTime,
    prelude::{DriveExt, FileExt, MountExt, VolumeExt, VolumeMonitorExt},
    Cancellable, File, FileQueryInfoFlags, FileType, VolumeMonitor,
};
use gtk::{gdk::Display, Clipboard, TargetEntry, TargetFlags};

pub fn list_volumes() -> Result<Vec<Volume>, String> {
    let _ = gtk::init();
    let mut volumes = Vec::new();
    let monitor = VolumeMonitor::get();

    for drive in monitor.connected_drives() {
        let mount_point = if drive.has_volumes() {
            drive.volumes().first().unwrap().get_mount().map(|m| m.default_location().to_string()).unwrap_or_else(|| String::new())
        } else {
            String::new()
        };

        let volume_label = drive.name().to_string();

        volumes.push(Volume {
            mount_point,
            volume_label,
        });
    }

    Ok(volumes)
}

pub fn get_file_attribute(file_path: &str) -> Result<FileAttribute, String> {
    let file = File::for_parse_name(file_path);
    let info = file.query_info("standard::*", FileQueryInfoFlags::NONE, Cancellable::NONE).unwrap();

    Ok(FileAttribute {
        directory: info.file_type() == FileType::Directory,
        read_only: false,
        hidden: info.is_hidden(),
        system: info.file_type() == FileType::Special,
        device: info.file_type() == FileType::Mountable,
        ctime: info.creation_date_time().unwrap_or(DateTime::now_local().unwrap()).to_unix() as f64,
        mtime: info.modification_date_time().unwrap_or(DateTime::now_local().unwrap()).to_unix() as f64,
        atime: info.access_date_time().unwrap_or(DateTime::now_local().unwrap()).to_unix() as f64,
        size: info.size() as u64,
    })
}

pub fn is_text_availabel() -> bool {
    if let Some(clipboard) = Clipboard::default(&Display::default().unwrap()) {
        return clipboard.wait_is_text_available();
    }

    false
}

pub fn read_text(_window_handle: isize) -> Result<String, String> {
    if let Some(clipboard) = Clipboard::default(&Display::default().unwrap()) {
        if clipboard.wait_is_uris_available() {
            return Ok(clipboard.wait_for_text().unwrap_or_default().to_string());
        }
    }

    Ok(String::new())
}

pub fn write_text(_window_handle: isize, text: String) -> Result<(), String> {
    if let Some(clipboard) = Clipboard::default(&Display::default().unwrap()) {
        clipboard.set_text(&text);
    }

    Ok(())
}

pub fn is_uris_available() -> bool {
    if let Some(clipboard) = Clipboard::default(&Display::default().unwrap()) {
        return clipboard.wait_is_uris_available();
    }

    false
}

pub fn read_uris(_window_handle: isize) -> Result<ClipboardData, String> {
    let data = ClipboardData {
        operation: Operation::None,
        urls: Vec::new(),
    };

    if let Some(clipboard) = Clipboard::default(&Display::default().unwrap()) {
        if clipboard.wait_is_uris_available() {
            let urls: Vec<String> = clipboard.wait_for_uris().iter().map(|gs| gs.to_string()).collect();

            return Ok(ClipboardData {
                operation: Operation::None,
                urls,
            });
        }
    }

    Ok(data)
}

pub fn write_uris(_window_handle: isize, paths: &[String], _operation: Operation) -> Result<(), String> {
    if let Some(clipboard) = Clipboard::default(&Display::default().unwrap()) {
        let targets = &[TargetEntry::new("text/uri-list", TargetFlags::OTHER_APP, 0)];
        let urls = paths.to_vec();
        let _ = clipboard.set_with_data(targets, move |_, selection, _| {
            let uri_list: Vec<&str> = urls.iter().map(|s| s.as_str()).collect();
            selection.set_uris(uri_list.as_slice());
        });
    }
    Ok(())
}

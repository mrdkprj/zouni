use crate::{Dirent, FileAttribute, Volume};
use gtk::{
    gio::{
        ffi::{G_FILE_COPY_ALL_METADATA, G_FILE_COPY_OVERWRITE},
        traits::{CancellableExt, FileExt},
        Cancellable, File, FileCopyFlags, FileInfo, FileQueryInfoFlags, FileType, IOErrorEnum,
    },
    glib::Error,
};
use once_cell::sync::Lazy;
use serde_json::Value;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Mutex,
};

static CANCELLABLES: Lazy<Mutex<HashMap<u32, Cancellable>>> = Lazy::new(|| Mutex::new(HashMap::new()));

const ATTRIBUTES: &str = "filesystem::readonly,standard::is-hidden,standard::is-symlink,standard::name,standard::size,standard::type,time::*,dos::is-system";
const ATTRIBUTES_FOR_DIALOG: &str = "filesystem::readonly,standard::is-hidden,standard::is-symlink,standard::name,standard::size,standard::type,standard::content-type,time::*,dos::is-system";

pub fn list_volumes() -> Result<Vec<Volume>, String> {
    let mut volumes = Vec::new();
    let output = std::process::Command::new("lsblk").args(["-ba", "--json", "-o", "NAME,TYPE,FSTYPE,LABEL,VENDOR,MODEL,SIZE,MOUNTPOINT,FSAVAIL"]).output().unwrap(); //.map_err(|e| e.to_string())?;
    let data: Value = serde_json::from_str(std::str::from_utf8(&output.stdout).unwrap()).map_err(|e| e.to_string())?;
    let drives: Vec<&Value> = data["blockdevices"].as_array().unwrap().iter().filter(|dev| dev["type"].as_str().unwrap_or_default() == "disk").collect();

    for drive in drives {
        let mut available_units = 0;
        let mut total_units = 0;
        let mut mount_point = String::new();

        if !drive["children"].is_null() {
            for child in drive["children"].as_array().unwrap().iter() {
                let child_mount_point = child["mountpoint"].as_str().unwrap();
                if !child_mount_point.contains("boot") {
                    mount_point = child_mount_point.to_string();
                }
                total_units += child["size"].as_u64().unwrap_or_default();
                available_units += child["fsavail"].as_u64().unwrap_or_default();
            }
        }

        let mut volume_label = if drive["label"].is_null() {
            String::new()
        } else {
            drive["label"].to_string()
        };
        volume_label.push_str(if drive["vendor"].is_null() {
            ""
        } else {
            drive["vendor"].as_str().unwrap_or_default()
        });
        volume_label.push_str(if drive["model"].is_null() {
            ""
        } else {
            drive["model"].as_str().unwrap_or_default()
        });
        volumes.push(Volume {
            mount_point,
            volume_label,
            available_units,
            total_units,
        });
    }

    Ok(volumes)
}

pub fn readdir<P: AsRef<Path>>(directory: P, recursive: bool, with_mime_type: bool) -> Result<Vec<Dirent>, String> {
    if !directory.as_ref().is_dir() {
        return Ok(Vec::new());
    }

    let file = File::for_parse_name(directory.as_ref().to_str().unwrap());

    let mut entries = Vec::new();
    try_readdir(file, &mut entries, recursive, with_mime_type)?;

    Ok(entries)
}

fn try_readdir(dir: File, entries: &mut Vec<Dirent>, recursive: bool, with_mime_type: bool) -> Result<&mut Vec<Dirent>, String> {
    for info in dir.enumerate_children(ATTRIBUTES, FileQueryInfoFlags::NOFOLLOW_SYMLINKS, Cancellable::NONE).unwrap().flatten() {
        let name = info.name();
        let mut full_path = dir.path().unwrap().to_path_buf();
        full_path.push(name.clone());

        let mime_type = if with_mime_type {
            get_mime_type(&full_path)
        } else {
            String::new()
        };

        entries.push(Dirent {
            name: name.file_name().unwrap_or_default().to_string_lossy().to_string(),
            parent_path: dir.path().unwrap().to_string_lossy().to_string(),
            full_path: full_path.to_string_lossy().to_string(),
            attributes: to_file_attribute(&info),
            mime_type,
        });

        if info.file_type() == FileType::Directory && recursive {
            let next_dir = File::for_path(full_path);
            try_readdir(next_dir, entries, recursive, with_mime_type)?;
        }
    }

    Ok(entries)
}

pub fn stat<P: AsRef<Path>>(file_path: P) -> Result<FileAttribute, String> {
    let file = File::for_parse_name(file_path.as_ref().to_str().unwrap());
    let info = file.query_info(ATTRIBUTES, FileQueryInfoFlags::NONE, Cancellable::NONE).map_err(|e| e.message().to_string())?;
    Ok(to_file_attribute(&info))
}

pub(crate) fn stat_inner<P: AsRef<Path>>(file_path: P) -> Result<FileInfo, String> {
    let file = File::for_parse_name(file_path.as_ref().to_str().unwrap());
    file.query_info(ATTRIBUTES_FOR_DIALOG, FileQueryInfoFlags::NONE, Cancellable::NONE).map_err(|e| e.message().to_string())
}

fn to_file_attribute(info: &FileInfo) -> FileAttribute {
    FileAttribute {
        is_directory: info.file_type() == FileType::Directory,
        is_read_only: info.boolean("filesystem::readonly"),
        is_hidden: info.is_hidden(),
        is_system: info.boolean("dos::is-system"),
        is_device: info.file_type() == FileType::Mountable,
        is_file: info.file_type() == FileType::Regular,
        is_symbolic_link: info.file_type() == FileType::SymbolicLink,
        ctime_ms: to_msecs(info.attribute_uint64("time::changed"), info.attribute_uint32("time::changed-usec")) as _,
        mtime_ms: to_msecs(info.attribute_uint64("time::modified"), info.attribute_uint32("time::modified-usec")) as _,
        atime_ms: to_msecs(info.attribute_uint64("time::access"), info.attribute_uint32("time::access-usec")) as _,
        birthtime_ms: to_msecs(info.attribute_uint64("time::created"), info.attribute_uint32("time::created-usec")) as _,
        size: info.size() as u64,
    }
}

fn to_msecs(secs: u64, microsecs: u32) -> f64 {
    (secs as f64) * 1000.0 + (microsecs as f64) / 1000.0
}

pub fn get_mime_type<P: AsRef<Path>>(file_path: P) -> String {
    match mime_guess::from_path(file_path).first() {
        Some(s) => s.essence_str().to_string(),
        None => String::new(),
    }
}

#[allow(dead_code)]
fn get_mime_type_fallback<P: AsRef<Path>>(file_path: P) -> Result<String, String> {
    if !file_path.as_ref().is_file() {
        return Ok(String::new());
    }

    let (ctype, _) = gtk::gio::content_type_guess(Some(file_path.as_ref().file_name().unwrap()), &[0]);
    Ok(ctype.to_string())
}

fn register_cancellable(cancel_id: u32) -> Cancellable {
    let mut tokens = CANCELLABLES.lock().unwrap();
    let token = Cancellable::new();
    tokens.insert(cancel_id, token.clone());
    token
}

pub fn mv<P1: AsRef<Path>, P2: AsRef<Path>>(source_file: P1, dest_file: P2, cancel_id: Option<u32>) -> Result<(), String> {
    execute_copy(source_file, dest_file, cancel_id, true)?;
    clean_up(cancel_id);
    Ok(())
}

pub fn copy<P1: AsRef<Path>, P2: AsRef<Path>>(source_file: P1, dest_file: P2, cancel_id: Option<u32>) -> Result<(), String> {
    execute_copy(source_file, dest_file, cancel_id, false)?;
    clean_up(cancel_id);
    Ok(())
}

fn execute_copy<P1: AsRef<Path>, P2: AsRef<Path>>(source_file: P1, dest_file: P2, cancel_id: Option<u32>, is_move: bool) -> Result<(), String> {
    let source = File::for_parse_name(source_file.as_ref().to_str().unwrap());
    let dest = File::for_parse_name(dest_file.as_ref().to_str().unwrap());

    let cancellable_token = if let Some(id) = cancel_id {
        register_cancellable(id)
    } else {
        Cancellable::new()
    };

    match source.copy(&dest, FileCopyFlags::from_bits(G_FILE_COPY_OVERWRITE | G_FILE_COPY_ALL_METADATA).unwrap(), Some(&cancellable_token), None) {
        Ok(_) => after_copy(&source, is_move)?,
        Err(e) => handel_error(e, &source, &dest, true, is_move)?,
    };

    Ok(())
}

pub fn mv_all<P1: AsRef<Path>, P2: AsRef<Path>>(source_files: &[P1], dest_dir: P2, cancel_id: Option<u32>) -> Result<(), String> {
    execute_copy_all(source_files, dest_dir, cancel_id, true)?;
    clean_up(cancel_id);
    Ok(())
}

pub fn copy_all<P1: AsRef<Path>, P2: AsRef<Path>>(source_files: &[P1], dest_dir: P2, cancel_id: Option<u32>) -> Result<(), String> {
    execute_copy_all(source_files, dest_dir, cancel_id, false)?;
    clean_up(cancel_id);
    Ok(())
}

fn execute_copy_all<P1: AsRef<Path>, P2: AsRef<Path>>(source_files: &[P1], dest_dir: P2, cancel_id: Option<u32>, is_move: bool) -> Result<(), String> {
    let sources: Vec<File> = source_files.iter().map(|f| File::for_parse_name(f.as_ref().to_str().unwrap())).collect();

    if dest_dir.as_ref().is_file() {
        return Err("Destination is file".to_string());
    }

    let mut dest_files: Vec<File> = Vec::new();

    for source_file in source_files {
        let name = source_file.as_ref().file_name().unwrap();
        let dest_file = dest_dir.as_ref().join(name);
        dest_files.push(File::for_parse_name(dest_file.to_str().unwrap()));
    }

    let cancellable_token = if let Some(id) = cancel_id {
        register_cancellable(id)
    } else {
        Cancellable::new()
    };

    for (i, source) in sources.iter().enumerate() {
        let dest = dest_files.get(i).unwrap();

        let done = match source.copy(dest, FileCopyFlags::from_bits(G_FILE_COPY_OVERWRITE | G_FILE_COPY_ALL_METADATA).unwrap(), Some(&cancellable_token), None) {
            Ok(_) => after_copy(source, is_move)?,
            Err(e) => handel_error(e, source, dest, true, is_move)?,
        };

        if !done {
            break;
        }
    }

    Ok(())
}

fn after_copy(source: &File, is_move: bool) -> Result<bool, String> {
    if is_move {
        source.delete(Cancellable::NONE).map_err(|e| e.message().to_string())?;
    }

    Ok(true)
}

fn handel_error(e: Error, source: &File, dest: &File, treat_cancel_as_error: bool, is_move: bool) -> Result<bool, String> {
    if is_move && dest.query_exists(Cancellable::NONE) {
        dest.delete(Cancellable::NONE).map_err(|e| e.message().to_string())?;
    }

    if !e.matches(IOErrorEnum::Cancelled) {
        return Err(format!("File: {}, Message: {}", source, e.message()));
    }

    if treat_cancel_as_error && e.matches(IOErrorEnum::Cancelled) {
        return Ok(false);
    }

    Ok(true)
}

fn clean_up(cancel_id: Option<u32>) {
    if let Ok(mut tokens) = CANCELLABLES.try_lock() {
        if let Some(id) = cancel_id {
            if tokens.get(&id).is_some() {
                tokens.remove(&id);
            }
        }
    }
}

pub fn delete<P: AsRef<Path>>(file_path: P) -> Result<(), String> {
    if file_path.as_ref().is_dir() {
        let files = readdir(file_path, false, false)?;
        for file in files {
            delete(file.full_path)?;
        }
    } else {
        let file = File::for_parse_name(file_path.as_ref().to_str().unwrap());
        file.delete(Cancellable::NONE).map_err(|e| e.message().to_string())?;
    }

    Ok(())
}

pub fn delete_all<P: AsRef<Path>>(file_paths: &[P]) -> Result<(), String> {
    for file_path in file_paths {
        delete(file_path)?;
    }

    Ok(())
}

pub fn trash<P: AsRef<Path>>(file: P) -> Result<(), String> {
    let file = File::for_parse_name(file.as_ref().to_str().unwrap());
    file.trash(Cancellable::NONE).map_err(|e| e.message().to_string())
}

pub fn trash_all<P: AsRef<Path>>(files: &[P]) -> Result<(), String> {
    for file in files {
        trash(file)?;
    }
    Ok(())
}

pub fn cancel(id: u32) -> bool {
    if let Ok(tokens) = CANCELLABLES.try_lock() {
        if let Some(token) = tokens.get(&id) {
            token.cancel();
            return true;
        }
    }

    false
}

const TRASH_PATH_STR: &str = "trash:///";

pub fn undelete(file_paths: Vec<String>) -> Result<(), String> {
    let trash_file = File::for_uri(TRASH_PATH_STR);
    let trash_path = PathBuf::from(TRASH_PATH_STR);

    if let Ok(children) = trash_file.enumerate_children("trash::orig-path,trash::deletion-date,standard::name", FileQueryInfoFlags::NONE, Cancellable::NONE) {
        let mut map: HashMap<String, i64> = HashMap::new();
        for child in children {
            if child.is_err() {
                continue;
            }

            let info = child.unwrap();
            let old_path = if let Some(path) = info.attribute_as_string("trash::orig-path") {
                path.to_string()
            } else {
                String::new()
            };

            let date_string = info.attribute_as_string("trash::deletion-date").unwrap();
            let date = gtk::glib::DateTime::from_iso8601(&date_string, Some(&gtk::glib::TimeZone::local())).unwrap().to_unix();

            if file_paths.contains(&old_path) {
                if map.contains_key(&old_path) {
                    let old_date = *map.get(&old_path).unwrap();
                    if old_date < date {
                        let _ = map.insert(old_path, date).unwrap();
                    }
                } else {
                    map.insert(old_path, date);
                }
            }
        }

        let items: Vec<PathBuf> = map.keys().map(PathBuf::from).collect();
        for item in items {
            let source_file = trash_path.join(item.file_name().unwrap());
            mv(source_file, item, None)?;
        }
    }

    Ok(())
}

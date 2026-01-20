use super::util::init;
use crate::{Dirent, FileAttribute, RecycleBinDirent, RecycleBinItem, Volume};
use gio::{
    ffi::{G_FILE_MEASURE_APPARENT_SIZE, G_FILE_QUERY_INFO_NONE},
    glib::ObjectExt,
    FileEnumerator, FileMeasureFlags, IOErrorEnum,
};
use gtk::{
    gio::{
        ffi::{G_FILE_COPY_ALL_METADATA, G_FILE_COPY_OVERWRITE},
        prelude::FileExtManual,
        traits::{CancellableExt, FileExt},
        Cancellable, File, FileCopyFlags, FileInfo, FileQueryInfoFlags, FileType,
    },
    glib::IsA,
    traits::{BoxExt, CssProviderExt, DialogExt, GtkWindowExt, HeaderBarExt, LabelExt, OrientableExt, ProgressBarExt, StyleContextExt, WidgetExt},
    Align, CssProvider, Dialog, DialogFlags, Label, Orientation, ProgressBar, ResponseType, STYLE_PROVIDER_PRIORITY_APPLICATION,
};
use libc::{timespec, utimensat, AT_FDCWD};
use serde_json::Value;
use smol::channel::Sender;
use std::{
    collections::HashMap,
    ffi::CString,
    path::Path,
    sync::{
        atomic::{AtomicU32, Ordering},
        LazyLock, Mutex,
    },
};

static UUID: AtomicU32 = AtomicU32::new(0);
static CANCELLABLES: LazyLock<Mutex<HashMap<u32, Cancellable>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

const ATTRIBUTES: &str = "filesystem::readonly,standard::is-hidden,standard::is-symlink,standard::name,standard::size,standard::type,time::*,dos::is-system,standard::symlink-target";
const ATTRIBUTES_FOR_DIALOG: &str =
    "filesystem::readonly,standard::is-hidden,standard::is-symlink,standard::name,standard::size,standard::type,standard::content-type,time::*,dos::is-system,standard::icon,standard::content-type";
const ATTRIBUTES_FOR_COPY: &str = "standard::name,standard::type";
const ATTRIBUTES_FOR_RECYCLE: &str =
    "trash::orig-path,trash::deletion-date,filesystem::readonly,standard::is-hidden,standard::is-symlink,standard::name,standard::size,standard::type,time::*,dos::is-system,standard::symlink-target";
const IO_CANCEL: &str = "IO_CANCEL";

/// Lists volumes
pub fn list_volumes() -> Result<Vec<Volume>, String> {
    let mut volumes = Vec::new();
    let output = std::process::Command::new("lsblk").args(["-ba", "--json", "-o", "NAME,TYPE,FSTYPE,LABEL,VENDOR,MODEL,SIZE,MOUNTPOINT,FSAVAIL"]).output().map_err(|e| e.to_string())?;
    let data: Value = serde_json::from_str(std::str::from_utf8(&output.stdout).unwrap()).map_err(|e| e.to_string())?;
    let drives: Vec<&Value> = data["blockdevices"].as_array().unwrap().iter().filter(|dev| dev["type"].as_str().unwrap_or_default() == "disk").collect();
    let exclude_mount_points = ["boot", "[SWAP]", "swap"];

    for drive in drives {
        let mut available_units = 0;
        let mut total_units = 0;
        let mut mount_point = String::new();

        if drive["children"].is_null() {
            let drive_mount_point = drive["mountpoint"].as_str().unwrap_or_default();
            mount_point = drive_mount_point.to_string();
            total_units += drive["size"].as_u64().unwrap_or_default();
            available_units += drive["fsavail"].as_u64().unwrap_or_default();
        } else {
            for child in drive["children"].as_array().unwrap().iter() {
                let child_mount_point = child["mountpoint"].as_str().unwrap_or_default();
                if !exclude_mount_points.iter().any(|p| child_mount_point.contains(p)) {
                    mount_point = child_mount_point.to_string();
                }
                total_units += child["size"].as_u64().unwrap_or_default();
                available_units += child["fsavail"].as_u64().unwrap_or_default();
            }
        }

        if mount_point.is_empty() {
            continue;
        }

        if exclude_mount_points.iter().any(|p| mount_point.contains(p)) {
            continue;
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

/// Lists all files/directories under the specified directory
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

        let full_path_string = full_path.to_string_lossy().to_string();
        let attributes = to_file_attribute(&info);

        let mime_type = if with_mime_type {
            get_mime_type(if attributes.is_symbolic_link {
                &attributes.link_path
            } else {
                &full_path_string
            })
        } else {
            String::new()
        };

        entries.push(Dirent {
            name: name.file_name().unwrap_or_default().to_string_lossy().to_string(),
            parent_path: dir.path().unwrap().to_string_lossy().to_string(),
            full_path: full_path_string,
            attributes,
            mime_type,
        });

        if info.file_type() == FileType::Directory && recursive {
            let next_dir = File::for_path(full_path);
            try_readdir(next_dir, entries, recursive, with_mime_type)?;
        }
    }

    Ok(entries)
}

/// Gets file/directory attributes
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
        is_symbolic_link: info.is_symlink(),
        ctime_ms: to_msecs(info.attribute_uint64("time::changed"), info.attribute_uint32("time::changed-usec")),
        mtime_ms: to_msecs(info.attribute_uint64("time::modified"), info.attribute_uint32("time::modified-usec")),
        atime_ms: to_msecs(info.attribute_uint64("time::access"), info.attribute_uint32("time::access-usec")),
        birthtime_ms: to_msecs(info.attribute_uint64("time::created"), info.attribute_uint32("time::created-usec")),
        size: info.size() as u64,
        link_path: if info.is_symlink() {
            info.symlink_target().unwrap_or_default().to_string_lossy().to_string()
        } else {
            String::new()
        },
    }
}

fn to_msecs(secs: u64, microsecs: u32) -> u64 {
    secs * 1000 + (microsecs as u64) / 1000
}

/// Create shortcut
pub fn create_symlink<P1: AsRef<Path>, P2: AsRef<Path>>(full_path: P1, link_path: P2) -> Result<(), String> {
    let file = gio::File::for_path(full_path);
    file.make_symbolic_link(link_path, Cancellable::NONE).map_err(|e| e.message().to_string())
}

/// Gets mime type of the file
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

fn register_cancellable() -> (u32, Cancellable) {
    let mut tokens = CANCELLABLES.lock().unwrap();
    let token = Cancellable::new();
    let id = UUID.fetch_add(1, Ordering::Relaxed);
    tokens.insert(id, token.clone());
    (id, token)
}

/// Moves an item
pub async fn mv<P1: AsRef<Path>, P2: AsRef<Path>>(from: P1, to: P2) -> Result<(), String> {
    let (sender, receiver) = smol::channel::bounded(1);

    execute_move(from, to, &Cancellable::new(), sender.clone());

    if let Ok(result) = receiver.recv().await {
        result
    } else {
        Ok(())
    }
}

/// Moves multiple items
pub async fn mv_all<P1: AsRef<Path>, P2: AsRef<Path>>(froms: &[P1], to: P2) -> Result<(), String> {
    let (cancel_id, cancellable) = register_cancellable();

    let (dir_count, file_count) = measure_size(froms);
    let mut done_count = 0;

    let message = format!("Moving {} items ", &(dir_count + file_count).to_string());
    let (dialog, progress_bar, label) = create_progress_dialog(message, froms.first().unwrap().as_ref().to_str().unwrap(), to.as_ref().to_str().unwrap(), cancel_id);

    let (sender, receiver) = smol::channel::bounded(1);

    dialog.show_all();

    for from in froms {
        label.set_text(from.as_ref().to_str().unwrap());
        execute_move(from, &to, &cancellable, sender.clone());
        if let Ok(result) = receiver.recv().await {
            if result.is_err() {
                return clean_up(result, &dialog, cancel_id);
            }
            done_count += 1;
            update_progress(&dialog, &progress_bar, done_count, file_count);
        }
    }

    clean_up(Ok(()), &dialog, cancel_id)
}

/// Copies an item
pub async fn copy<P1: AsRef<Path>, P2: AsRef<Path>>(from: P1, to: P2) -> Result<(), String> {
    let (sender, receiver) = smol::channel::bounded(1);

    execute_copy(from, to, &Cancellable::new(), sender.clone());

    if let Ok(result) = receiver.recv().await {
        result
    } else {
        Ok(())
    }
}

/// Copies multiple items
pub async fn copy_all<P1: AsRef<Path>, P2: AsRef<Path>>(froms: &[P1], to: P2) -> Result<(), String> {
    let (cancel_id, cancellable) = register_cancellable();

    let (dir_count, file_count) = measure_size(froms);
    let mut done_count = 0;

    let message = format!("Copying {} items ", &(dir_count + file_count).to_string());
    let (dialog, progress_bar, label) = create_progress_dialog(message, froms.first().unwrap().as_ref().to_str().unwrap(), to.as_ref().to_str().unwrap(), cancel_id);

    let (sender, receiver) = smol::channel::bounded(1);

    dialog.show_all();

    for from in froms {
        label.set_text(from.as_ref().to_str().unwrap());
        execute_copy(from, &to, &cancellable, sender.clone());
        if let Ok(result) = receiver.recv().await {
            if result.is_err() {
                return clean_up(result, &dialog, cancel_id);
            }
            done_count += 1;
            update_progress(&dialog, &progress_bar, done_count, file_count);
        }
    }

    clean_up(Ok(()), &dialog, cancel_id)
}

fn measure_size<P1: AsRef<Path>>(entries: &[P1]) -> (u64, u64) {
    let mut dir_count = 0;
    let mut file_count = 0;
    for entry in entries {
        let (_, num_dirs, num_files) = File::for_path(entry).measure_disk_usage(FileMeasureFlags::from_bits(G_FILE_MEASURE_APPARENT_SIZE).unwrap(), Cancellable::NONE, None).unwrap();
        dir_count += num_dirs;
        file_count += num_files;
    }

    (dir_count, file_count)
}

fn update_progress(dialog: &Dialog, progress_bar: &ProgressBar, current: u64, total: u64) {
    let (progress, text) = if current > 0 {
        let progress = (current as f64 * 1.0) / (total as f64 * 1.0);
        let percent = progress * 100.0;

        (progress as _, percent.to_string())
    } else {
        (1.0, "100%".to_string())
    };

    dialog.set_title(&format!("{}% complete", text));
    progress_bar.set_fraction(progress);
}

fn execute_move<P1: AsRef<Path>, P2: AsRef<Path>>(from: P1, to: P2, cancellable: &impl IsA<Cancellable>, sender: Sender<Result<(), String>>) {
    let source = File::for_parse_name(from.as_ref().to_str().unwrap());
    let to_dr = to.as_ref().join(from.as_ref().file_name().unwrap());
    let dest = File::for_parse_name(to_dr.to_str().unwrap());

    if from.as_ref().file_name().unwrap() == to_dr.file_name().unwrap() && to_dr.exists() && handle_result(delete(to_dr), &sender) {
        return;
    }

    source.move_async(&dest, FileCopyFlags::from_bits(G_FILE_COPY_ALL_METADATA).unwrap(), gtk::glib::Priority::DEFAULT, Some(cancellable), None, move |result| {
        handle_result(
            result.map_err(|e| {
                if e.matches(IOErrorEnum::Cancelled) {
                    IO_CANCEL.to_string()
                } else {
                    e.message().to_string()
                }
            }),
            &sender,
        );
    });
}

fn execute_copy<P1: AsRef<Path>, P2: AsRef<Path>>(from: P1, to: P2, cancellable: &impl IsA<Cancellable>, sender: Sender<Result<(), String>>) {
    if from.as_ref().is_dir() {
        return copy_directory(from, to, cancellable, sender);
    }

    let source = File::for_parse_name(from.as_ref().to_str().unwrap());
    let to_dr = to.as_ref().join(from.as_ref().file_name().unwrap());
    let dest = File::for_parse_name(to_dr.to_str().unwrap());

    if from.as_ref().file_name().unwrap() == to_dr.file_name().unwrap() && to_dr.exists() && handle_result(delete(to_dr), &sender) {
        return;
    }

    source.copy_async(&dest, FileCopyFlags::from_bits(G_FILE_COPY_ALL_METADATA).unwrap(), gtk::glib::Priority::DEFAULT, Some(cancellable), None, move |result| {
        handle_result(
            result.map_err(|e| {
                if e.matches(IOErrorEnum::Cancelled) {
                    IO_CANCEL.to_string()
                } else {
                    e.message().to_string()
                }
            }),
            &sender,
        );
    });
}

fn copy_directory<P1: AsRef<Path>, P2: AsRef<Path>>(from: P1, to: P2, cancellable: &impl IsA<Cancellable>, sender: Sender<Result<(), String>>) {
    let source = File::for_parse_name(from.as_ref().to_str().unwrap());
    let to_dr = to.as_ref().join(from.as_ref().file_name().unwrap());
    let dest = File::for_parse_name(to_dr.to_str().unwrap());

    if !dest.query_exists(Cancellable::NONE) {
        if handle_result(dest.make_directory(Cancellable::NONE).map_err(|e| e.message().to_string()), &sender) {
            return;
        }

        let settable_attributes = dest.query_settable_attributes(Cancellable::NONE).unwrap();
        let attributes_info = settable_attributes.attributes();
        let attributes = attributes_info.iter().map(|a| a.name()).collect::<Vec<&str>>().join(",");
        let info = source.query_info(&attributes, FileQueryInfoFlags::from_bits(G_FILE_QUERY_INFO_NONE).unwrap(), Cancellable::NONE).unwrap();
        dest.set_attributes_from_info(&info, FileQueryInfoFlags::from_bits(G_FILE_QUERY_INFO_NONE).unwrap(), Cancellable::NONE).unwrap();
    }

    if let Ok(mut children) = source.enumerate_children(ATTRIBUTES_FOR_COPY, FileQueryInfoFlags::from_bits(G_FILE_QUERY_INFO_NONE).unwrap(), Cancellable::NONE) {
        while let Some(Ok(info)) = children.next() {
            let from_file = from.as_ref().to_path_buf().join(info.name());
            if info.file_type() == FileType::Directory {
                copy_directory(from_file, &to_dr, cancellable, sender.clone());
            } else {
                execute_copy(from_file, &to_dr, cancellable, sender.clone());
            }
        }
    }
}

fn handle_result(result: Result<(), String>, sender: &Sender<Result<(), String>>) -> bool {
    if result.is_err() {
        let _ = sender.try_send(result);
        let _ = sender.close();
        false
    } else {
        let _ = sender.try_send(result);
        true
    }
}

fn clean_up(result: Result<(), String>, dialog: &gtk::Dialog, cancel_id: u32) -> Result<(), String> {
    dialog.close();

    if let Ok(mut tokens) = CANCELLABLES.try_lock() {
        if tokens.get(&cancel_id).is_some() {
            tokens.remove(&cancel_id);
        }
    }

    match result {
        Ok(_) => Ok(()),
        Err(e) => {
            if e == IO_CANCEL {
                Ok(())
            } else {
                Err(e)
            }
        }
    }
}

/// Deletes an item
pub fn delete<P: AsRef<Path>>(file_path: P) -> Result<(), String> {
    if file_path.as_ref().is_dir() {
        let files = readdir(&file_path, false, false)?;
        for file in files {
            delete(file.full_path)?;
        }
    }

    let file = File::for_parse_name(file_path.as_ref().to_str().unwrap());
    file.delete(Cancellable::NONE).map_err(|e| e.message().to_string())?;

    Ok(())
}

/// Deletes multiple items
pub fn delete_all<P: AsRef<Path>>(file_paths: &[P]) -> Result<(), String> {
    for file_path in file_paths {
        delete(file_path)?;
    }

    Ok(())
}

/// Moves an item to the OS-specific trash location
pub fn trash<P: AsRef<Path>>(file: P) -> Result<(), String> {
    let file = File::for_parse_name(file.as_ref().to_str().unwrap());
    file.trash(Cancellable::NONE).map_err(|e| e.message().to_string())
}

/// Moves multiple items to the OS-specific trash location
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

struct TrashData {
    date: i64,
    name: String,
}

const TRASH_PATH_STR: &str = "trash:///";

/// Gets items in recycle bin
pub fn read_recycle_bin() -> Result<Vec<RecycleBinDirent>, String> {
    let trash_file = File::for_uri(TRASH_PATH_STR);
    let mut result = Vec::new();

    if let Ok(mut children) = trash_file.enumerate_children(ATTRIBUTES_FOR_RECYCLE, FileQueryInfoFlags::NONE, Cancellable::NONE) {
        while let Some(Ok(info)) = children.next() {
            let original_path = if let Some(path) = info.attribute_as_string("trash::orig-path") {
                path.to_string()
            } else {
                String::new()
            };
            let name = if let Some(name) = info.attribute_as_string("standard::name") {
                name.to_string()
            } else {
                String::new()
            };

            let deleted_date_ms = if let Some(delete_date_string) = info.attribute_as_string("trash::deletion-date") {
                gtk::glib::DateTime::from_iso8601(&delete_date_string, Some(&gtk::glib::TimeZone::local())).unwrap().to_unix() as u64
            } else {
                0
            };

            let attributes = to_file_attribute(&info);
            let mime_type = get_mime_type(&original_path);

            let bin_item = RecycleBinDirent {
                name,
                original_path,
                deleted_date_ms,
                attributes,
                mime_type,
            };
            result.push(bin_item);
        }
    }
    Ok(result)
}

/// Undos a trash operation
pub fn undelete<P: AsRef<Path>>(file_paths: &[P]) -> Result<(), String> {
    let trash_file = File::for_uri(TRASH_PATH_STR);

    if let Ok(mut children) = trash_file.enumerate_children("trash::orig-path,trash::deletion-date,standard::name", FileQueryInfoFlags::NONE, Cancellable::NONE) {
        let file_paths: Vec<String> = file_paths.iter().map(|f| f.as_ref().to_string_lossy().to_string()).collect();
        let mut map: HashMap<String, TrashData> = HashMap::new();
        while let Some(Ok(info)) = children.next() {
            let orig_path = if let Some(path) = info.attribute_as_string("trash::orig-path") {
                path.to_string()
            } else {
                String::new()
            };

            let date_string = info.attribute_as_string("trash::deletion-date").unwrap();
            let date = gtk::glib::DateTime::from_iso8601(&date_string, Some(&gtk::glib::TimeZone::local())).unwrap().to_unix();

            if file_paths.contains(&orig_path) {
                if map.contains_key(&orig_path) {
                    let trash_data = map.get(&orig_path).unwrap();
                    if trash_data.date < date {
                        let _ = map.insert(
                            orig_path,
                            TrashData {
                                date,
                                name: info.name().to_string_lossy().to_string(),
                            },
                        );
                    }
                } else {
                    let _ = map.insert(
                        orig_path,
                        TrashData {
                            date,
                            name: info.name().to_string_lossy().to_string(),
                        },
                    );
                }
            }
        }

        for (orig_path, trash_data) in map.iter() {
            let mut trash_path = String::from(TRASH_PATH_STR);
            trash_path.push_str(&trash_data.name);

            File::for_uri(&trash_path)
                .move_(&File::for_parse_name(orig_path), FileCopyFlags::from_bits(G_FILE_COPY_OVERWRITE | G_FILE_COPY_ALL_METADATA).unwrap(), Cancellable::NONE, None)
                .map_err(|e| e.message().to_string())?;
        }
    }

    Ok(())
}

/// Undos a trash operation by deleted time
pub fn undelete_by_time(targets: &[RecycleBinItem]) -> Result<(), String> {
    let trash_file = File::for_uri(TRASH_PATH_STR);

    if let Ok(children) = trash_file.enumerate_children("trash::orig-path,trash::deletion-date,standard::name", FileQueryInfoFlags::NONE, Cancellable::NONE) {
        let args: HashMap<String, u64> = targets.iter().map(|target| (target.original_path.clone(), target.deleted_time_ms)).collect();
        let map = find_items_in_recycle_bin(children, args)?;

        for (orig_path, trash_data) in map.iter() {
            let mut trash_path = String::from(TRASH_PATH_STR);
            trash_path.push_str(&trash_data.name);

            File::for_uri(&trash_path)
                .move_(&File::for_parse_name(orig_path), FileCopyFlags::from_bits(G_FILE_COPY_OVERWRITE | G_FILE_COPY_ALL_METADATA).unwrap(), Cancellable::NONE, None)
                .map_err(|e| e.message().to_string())?;
        }
    }

    Ok(())
}

/// Delete files in Recycle Bin
pub fn delete_from_recycle_bin(targets: &[RecycleBinItem]) -> Result<(), String> {
    let trash_file = File::for_uri(TRASH_PATH_STR);

    if let Ok(children) = trash_file.enumerate_children("trash::orig-path,trash::deletion-date,standard::name", FileQueryInfoFlags::NONE, Cancellable::NONE) {
        let args: HashMap<String, u64> = targets.iter().map(|target| (target.original_path.clone(), target.deleted_time_ms)).collect();
        let map = find_items_in_recycle_bin(children, args)?;

        for (_, trash_data) in map.iter() {
            let mut trash_path = String::from(TRASH_PATH_STR);
            trash_path.push_str(&trash_data.name);

            File::for_uri(&trash_path).delete(Cancellable::NONE).map_err(|e| e.message().to_string())?;
        }
    }

    Ok(())
}

fn find_items_in_recycle_bin(mut children: FileEnumerator, map: HashMap<String, u64>) -> Result<HashMap<String, TrashData>, String> {
    let mut items: HashMap<String, TrashData> = HashMap::new();
    while let Some(Ok(info)) = children.next() {
        let orig_path = if let Some(path) = info.attribute_as_string("trash::orig-path") {
            path.to_string()
        } else {
            String::new()
        };

        let date_string = info.attribute_as_string("trash::deletion-date").unwrap();
        let date = gtk::glib::DateTime::from_iso8601(&date_string, Some(&gtk::glib::TimeZone::local())).unwrap().to_unix();

        if map.contains_key(&orig_path) && *map.get(&orig_path).unwrap() == date as u64 {
            let _ = items.insert(
                orig_path,
                TrashData {
                    date,
                    name: info.name().to_string_lossy().to_string(),
                },
            );
        }
    }
    Ok(items)
}

#[allow(unused_variables)]
/// Empty Recycle Bin
/// Parameter "root" has no effect on Linux
pub fn empty_recycle_bin(root: Option<String>) -> Result<(), String> {
    let trash_file = File::for_uri(TRASH_PATH_STR);
    if let Ok(mut children) = trash_file.enumerate_children("trash::orig-path,trash::deletion-date,standard::name", FileQueryInfoFlags::NONE, Cancellable::NONE) {
        while let Some(Ok(info)) = children.next() {
            let mut trash_path = String::from(TRASH_PATH_STR);
            trash_path.push_str(info.name().to_str().unwrap());
            File::for_uri(&trash_path).delete(Cancellable::NONE).map_err(|e| e.message().to_string())?;
        }
    }
    Ok(())
}

fn create_progress_dialog(message: String, from_item: &str, to_item: &str, cancel_id: u32) -> (Dialog, ProgressBar, Label) {
    init();

    let dialog = Dialog::with_buttons::<gtk::Window>(None, None, DialogFlags::MODAL, &[("Cancel", ResponseType::Cancel), ("Close", ResponseType::Close)]);

    // HeaderBar
    let header = gtk::HeaderBar::new();
    // header.set_decoration_layout(Some("icon:minimize,close"));
    header.set_show_close_button(true);
    let css_provider = CssProvider::new();
    let css = r#"
        headerbar entry,
        headerbar spinbutton,
        headerbar button,
        headerbar separator {
            margin-top: 0px; /* same as headerbar side padding for nicer proportions */
            margin-bottom: 0px;
        }

        headerbar {
            min-height: 0px;
            padding-left: 2px; /* same as childrens vertical margins for nicer proportions */
            padding-right: 2px;
            margin: 0px; /* same as headerbar side padding for nicer proportions */
            padding: 0px;
        }
    "#;
    css_provider.load_from_data(css.as_bytes()).unwrap();
    header.style_context().add_provider(&css_provider, STYLE_PROVIDER_PRIORITY_APPLICATION);
    dialog.set_titlebar(Some(&header));
    dialog.set_title("0% complete");

    let content_area = dialog.content_area();
    content_area.set_orientation(Orientation::Vertical);
    content_area.set_halign(Align::Start);
    content_area.set_hexpand(false);

    // Message Label
    let message_label_container = gtk::Box::new(Orientation::Vertical, 5);
    let label = Label::new(Some(&message));
    label.set_xalign(0.0);
    label.set_margin_start(10);
    message_label_container.pack_start(&label, false, false, 0);

    // From/To Label
    let progress_label_container = gtk::Box::new(Orientation::Horizontal, 0);
    let from_label = Label::new(Some("From "));
    from_label.set_xalign(0.0);
    let from = Label::new(Some(from_item));
    from.set_xalign(0.0);
    from.set_widget_name("entrylabel");
    from.set_max_width_chars(20);
    let to_label = Label::new(Some(" to "));
    to_label.set_xalign(0.0);
    let to = Label::new(Some(to_item));
    to.set_xalign(0.0);
    to.set_widget_name("entrylabel");
    to.set_max_width_chars(20);

    progress_label_container.pack_start(&from_label, false, false, 0);
    progress_label_container.pack_start(&from, false, false, 0);
    progress_label_container.pack_start(&to_label, false, false, 0);
    progress_label_container.pack_start(&to, false, false, 0);
    let css_provider = CssProvider::new();
    let css = r#"
        label#entrylabel {
            color:blue;
        }
    "#;
    css_provider.load_from_data(css.as_bytes()).unwrap();
    from.style_context().add_provider(&css_provider, STYLE_PROVIDER_PRIORITY_APPLICATION);
    from.set_ellipsize(gtk::pango::EllipsizeMode::End);
    from.set_tooltip_text(Some(from_item));
    to.style_context().add_provider(&css_provider, STYLE_PROVIDER_PRIORITY_APPLICATION);
    to.set_ellipsize(gtk::pango::EllipsizeMode::End);
    to.set_tooltip_text(Some(to_item));
    progress_label_container.set_margin_start(10);
    progress_label_container.set_width_request(100);
    message_label_container.pack_start(&progress_label_container, false, false, 0);
    content_area.pack_start(&message_label_container, false, false, 5);

    // ProgressBar
    let progress_bar = ProgressBar::new();
    let css_provider = CssProvider::new();
    let css = r#"
        progress, trough {
            min-height: 30px;
            min-width: 400px;
        }
    "#;
    css_provider.load_from_data(css.as_bytes()).unwrap();
    progress_bar.style_context().add_provider(&css_provider, STYLE_PROVIDER_PRIORITY_APPLICATION);
    progress_bar.set_fraction(0.0);
    content_area.pack_start(&progress_bar, true, true, 10);

    unsafe { dialog.set_data("cancel_id", cancel_id) };

    dialog.connect_destroy(|dialog| {
        try_cancel(dialog);
    });

    dialog.connect_close(|dialog| {
        // The default binding for this signal is the Escape key
        try_cancel(dialog);
    });

    dialog.connect_response(|dialog, response| {
        if response == ResponseType::Cancel || response == ResponseType::Close {
            try_cancel(dialog);
            dialog.close();
        }
    });

    (dialog, progress_bar, from)
}

fn try_cancel(dialog: &Dialog) {
    let cancel_id = unsafe { dialog.data::<u32>("cancel_id").unwrap().as_ref() };
    if let Some(cancellable) = CANCELLABLES.lock().unwrap().get(cancel_id) {
        cancellable.cancel();
    }
}

/// Changes the modification and access timestamps of a file
pub fn utimes<P: AsRef<Path>>(file: P, atime_ms: u64, mtime_ms: u64) -> Result<(), String> {
    let path = CString::new(file.as_ref().to_string_lossy().to_string()).map_err(|e| e.to_string())?;
    let timespecs = [to_timespec(atime_ms), to_timespec(mtime_ms)];
    let result = unsafe { utimensat(AT_FDCWD, path.as_ptr(), timespecs.as_ptr(), 0) };
    if result < 0 {
        Err("utimensat failed".to_string())
    } else {
        Ok(())
    }
}

fn to_timespec(msec: u64) -> timespec {
    let mut timespec = timespec {
        tv_sec: (msec / 1000) as _,
        tv_nsec: ((msec % 1000) * 1000000) as i64,
    };

    if timespec.tv_nsec < 0 {
        timespec.tv_nsec += 1e9 as i64;
        timespec.tv_sec -= 1;
    }

    timespec
}

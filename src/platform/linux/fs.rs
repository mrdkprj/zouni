use crate::{Dirent, FileAttribute, Volume};
use gio::{
    ffi::{g_file_copy, G_FILE_COPY_ALL_METADATA, G_FILE_COPY_OVERWRITE},
    glib::{
        ffi::{gpointer, GFALSE},
        translate::{from_glib_full, ToGlibPtr},
        DateTime, Error,
    },
    prelude::{CancellableExt, DriveExt, FileExt, MountExt, VolumeExt, VolumeMonitorExt},
    Cancellable, File, FileCopyFlags, FileQueryInfoFlags, FileType, IOErrorEnum, VolumeMonitor,
};
use once_cell::sync::Lazy;
use std::{
    collections::HashMap,
    fs,
    path::Path,
    sync::{
        atomic::{AtomicU32, Ordering},
        Mutex,
    },
};

static UUID: AtomicU32 = AtomicU32::new(0);
static CANCELLABLES: Lazy<Mutex<HashMap<u32, Cancellable>>> = Lazy::new(|| Mutex::new(HashMap::new()));

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
            available_units: 0,
            total_units: 0,
        });
    }

    Ok(volumes)
}

pub fn readdir<P: AsRef<Path>>(directory: P, recursive: bool, with_mime_type: bool) -> Result<Vec<Dirent>, String> {
    if !directory.as_ref().is_dir() {
        return Ok(Vec::new());
    }

    let mut entries = Vec::new();
    try_readdir(directory, &mut entries, recursive, with_mime_type).map_err(|e| e)?;

    Ok(entries)
}

fn try_readdir<P: AsRef<Path>>(dir: P, entries: &mut Vec<Dirent>, recursive: bool, with_mime_type: bool) -> Result<&mut Vec<Dirent>, String> {
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.path().is_dir() && recursive {
            try_readdir(entry.path(), entries, recursive, with_mime_type)?;
        }

        let path = entry.path();
        let mime_type = if with_mime_type {
            get_mime_type(&path)?
        } else {
            String::new()
        };

        entries.push(Dirent {
            name: entry.file_name().to_string_lossy().to_string(),
            parent_path: path.parent().unwrap_or(Path::new("")).to_string_lossy().to_string(),
            full_path: entry.path().to_string_lossy().to_string(),
            attributes: get_file_attribute(path.to_str().unwrap()).map_err(|e| e)?,
            mime_type,
        });
    }

    Ok(entries)
}

pub fn get_file_attribute<P: AsRef<Path>>(file_path: P) -> Result<FileAttribute, String> {
    let file = File::for_parse_name(file_path.as_ref().to_str().unwrap());
    let info = file.query_info("standard::*", FileQueryInfoFlags::NONE, Cancellable::NONE).unwrap();

    Ok(FileAttribute {
        is_directory: info.file_type() == FileType::Directory,
        is_read_only: false,
        is_hidden: info.is_hidden(),
        is_system: info.file_type() == FileType::Special,
        is_device: info.file_type() == FileType::Mountable,
        is_file: info.file_type() == FileType::Regular,
        is_symbolic_link: info.file_type() == FileType::SymbolicLink,
        ctime: info.creation_date_time().unwrap_or(DateTime::now_local().unwrap()).to_unix() as f64,
        mtime: info.modification_date_time().unwrap_or(DateTime::now_local().unwrap()).to_unix() as f64,
        atime: info.access_date_time().unwrap_or(DateTime::now_local().unwrap()).to_unix() as f64,
        size: info.size() as u64,
    })
}

pub fn get_mime_type<P: AsRef<Path>>(file_path: P) -> Result<String, String> {
    if !file_path.as_ref().is_file() {
        return Ok(String::new());
    }

    let (ctype, _) = gio::content_type_guess(Some(file_path.as_ref().file_name().unwrap()), &[0]);
    Ok(ctype.to_string())
}

struct BulkProgressData<'a> {
    callback: Option<&'a mut dyn FnMut(i64, i64)>,
    total: i64,
    processed: i64,
    in_process: bool,
}

pub fn reserve_cancellable() -> u32 {
    let id = UUID.fetch_add(1, Ordering::Relaxed);

    let mut tokens = CANCELLABLES.lock().unwrap();
    let token = Cancellable::new();
    tokens.insert(id, token);

    id
}

pub fn mv<P: AsRef<Path>, P2: AsRef<Path>>(source_file: P, dest_file: P2, callback: Option<&mut dyn FnMut(i64, i64)>, cancel_id: Option<u32>) -> Result<(), String> {
    let result = inner_move(source_file, dest_file, callback, cancel_id);
    clean_up(cancel_id);
    result
}

fn inner_move<P: AsRef<Path>, P2: AsRef<Path>>(source_file: P, dest_file: P2, callback: Option<&mut dyn FnMut(i64, i64)>, cancel_id: Option<u32>) -> Result<(), String> {
    let source = File::for_parse_name(source_file.as_ref().to_str().unwrap());
    let dest = File::for_parse_name(dest_file.as_ref().to_str().unwrap());

    let cancellable_token = if let Some(id) = cancel_id {
        {
            let tokens = CANCELLABLES.lock().unwrap();
            tokens.get(&id).unwrap().clone()
        }
    } else {
        Cancellable::new()
    };

    match source.copy(&dest, FileCopyFlags::from_bits(G_FILE_COPY_OVERWRITE | G_FILE_COPY_ALL_METADATA).unwrap(), Some(&cancellable_token), callback) {
        Ok(_) => after_copy(&source)?,
        Err(e) => handel_error(e, &source, &dest, true)?,
    };

    Ok(())
}

pub fn mv_all<P: AsRef<Path>, P2: AsRef<Path>>(source_files: Vec<P>, dest_dir: P2, callback: Option<&mut dyn FnMut(i64, i64)>, cancel_id: Option<u32>) -> Result<(), String> {
    let result = inner_mv_bulk(source_files, dest_dir, callback, cancel_id);
    clean_up(cancel_id);
    result
}

fn inner_mv_bulk<P: AsRef<Path>, P2: AsRef<Path>>(source_files: Vec<P>, dest_dir: P2, callback: Option<&mut dyn FnMut(i64, i64)>, cancel_id: Option<u32>) -> Result<(), String> {
    let sources: Vec<File> = source_files.iter().map(|f| File::for_parse_name(f.as_ref().to_str().unwrap())).collect();

    if dest_dir.as_ref().is_file() {
        return Err("Destination is file".to_string());
    }

    let mut total: i64 = 0;
    let mut dest_files: Vec<File> = Vec::new();

    for source_file in source_files {
        let metadata = fs::metadata(&source_file).unwrap();
        total += metadata.len() as i64;
        let name = source_file.as_ref().file_name().unwrap();
        let dest_file = dest_dir.as_ref().join(name);
        dest_files.push(File::for_parse_name(dest_file.to_str().unwrap()));
    }

    let cancellable_token = if let Some(id) = cancel_id {
        {
            let tokens = CANCELLABLES.lock().unwrap();
            tokens.get(&id).unwrap().clone()
        }
    } else {
        Cancellable::new()
    };

    let data = Box::into_raw(Box::new(BulkProgressData {
        callback,
        total,
        processed: 0,
        in_process: true,
    }));

    let flags = FileCopyFlags::from_bits(G_FILE_COPY_OVERWRITE | G_FILE_COPY_ALL_METADATA).unwrap().bits();

    for (i, source) in sources.iter().enumerate() {
        let dest = dest_files.get(i).unwrap();
        let mut error = std::ptr::null_mut();

        let is_ok = unsafe { g_file_copy(source.to_glib_none().0, dest.to_glib_none().0, flags, cancellable_token.to_glib_none().0, Some(progress_callback), data as _, &mut error) };
        debug_assert_eq!(is_ok == GFALSE, !error.is_null());

        let result: Result<(), Error> = if error.is_null() {
            Ok(())
        } else {
            Err(unsafe { from_glib_full(error) })
        };

        let done = match result {
            Ok(_) => after_copy(source)?,
            Err(e) => handel_error(e, source, dest, true)?,
        };

        if !done {
            break;
        }
    }

    Ok(())
}

fn after_copy(source: &File) -> Result<bool, String> {
    source.delete(Cancellable::NONE).map_err(|e| e.message().to_string())?;

    Ok(true)
}

fn handel_error(e: Error, source: &File, dest: &File, treat_cancel_as_error: bool) -> Result<bool, String> {
    if dest.query_exists(Cancellable::NONE) {
        dest.delete(Cancellable::NONE).map_err(|e| e.message().to_string())?;
    }

    if !e.matches(IOErrorEnum::Cancelled) {
        return Err(format!("File: {}, Message: {}", source, e.message().to_string()));
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

unsafe extern "C" fn progress_callback(current_num_bytes: i64, total_num_bytes: i64, userdata: gpointer) {
    let item_data_ptr = userdata as *mut BulkProgressData;
    let data = unsafe { &mut *item_data_ptr };

    if total_num_bytes == current_num_bytes {
        data.in_process = !data.in_process;
    }

    if data.in_process {
        let current = data.processed + current_num_bytes;

        if total_num_bytes == current_num_bytes {
            data.processed = data.processed + total_num_bytes;
        }

        if let Some(callback) = data.callback.as_mut() {
            callback(current, data.total);
        }
    }
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

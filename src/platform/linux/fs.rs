use crate::{FileAttribute, Volume};
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

pub fn open_file_property(_window_handle: isize, _file_path: String) -> Result<(), String> {
    Ok(())
}

pub fn open_path(_window_handle: isize, file_path: String) -> Result<(), String> {
    gio::AppInfo::launch_default_for_uri(&file_path, gio::AppLaunchContext::NONE).map_err(|e| e.message().to_string())
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

pub fn mv(source_file: String, dest_file: String, callback: Option<&mut dyn FnMut(i64, i64)>, cancel_id: Option<u32>) -> Result<(), String> {
    let result = inner_move(source_file, dest_file, callback, cancel_id);
    clean_up(cancel_id);
    result
}

fn inner_move(source_file: String, dest_file: String, callback: Option<&mut dyn FnMut(i64, i64)>, cancel_id: Option<u32>) -> Result<(), String> {
    let source = File::for_parse_name(&source_file);
    let dest = File::for_parse_name(&dest_file);

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

pub fn mv_bulk(source_files: Vec<String>, dest_dir: String, callback: Option<&mut dyn FnMut(i64, i64)>, cancel_id: Option<u32>) -> Result<(), String> {
    let result = inner_mv_bulk(source_files, dest_dir, callback, cancel_id);
    clean_up(cancel_id);
    result
}

fn inner_mv_bulk(source_files: Vec<String>, dest_dir: String, callback: Option<&mut dyn FnMut(i64, i64)>, cancel_id: Option<u32>) -> Result<(), String> {
    let sources: Vec<File> = source_files.iter().map(|f| File::for_parse_name(&f)).collect();
    let dest_dir_path = Path::new(&dest_dir);
    if dest_dir_path.is_file() {
        return Err("Destination is file".to_string());
    }

    let mut total: i64 = 0;
    let mut dest_files: Vec<File> = Vec::new();

    for source_file in source_files {
        let metadata = fs::metadata(&source_file).unwrap();
        total += metadata.len() as i64;
        let path = Path::new(&source_file);
        let name = path.file_name().unwrap();
        let dest_file = dest_dir_path.join(name);
        dest_files.push(File::for_parse_name(dest_file.to_string_lossy().as_ref()));
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

pub fn trash(file: String) -> Result<(), String> {
    let file = File::for_parse_name(&file);
    file.trash(Cancellable::NONE).map_err(|e| e.message().to_string())
}

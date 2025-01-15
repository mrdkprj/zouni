use super::util::{decode_wide, encode_wide, prefixed};
use crate::{Dirent, FileAttribute, Volume};
use once_cell::sync::Lazy;
use std::{
    collections::HashMap,
    ffi::c_void,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU32, Ordering},
        Mutex,
    },
};
use windows::{
    core::{Error, HRESULT, PCWSTR},
    Win32::{
        Foundation::{HANDLE, MAX_PATH},
        Storage::FileSystem::{
            DeleteFileW, FindClose, FindExInfoBasic, FindExSearchNameMatch, FindFirstFileExW, FindFirstVolumeW, FindNextFileW, FindNextVolumeW, FindVolumeClose, GetDiskFreeSpaceExW,
            GetFileAttributesW, GetVolumeInformationW, GetVolumePathNamesForVolumeNameW, MoveFileExW, MoveFileWithProgressW, FILE_ATTRIBUTE_DEVICE, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_HIDDEN,
            FILE_ATTRIBUTE_READONLY, FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_SYSTEM, FIND_FIRST_EX_FLAGS, LPPROGRESS_ROUTINE_CALLBACK_REASON, MOVEFILE_COPY_ALLOWED, MOVEFILE_REPLACE_EXISTING,
            MOVEFILE_WRITE_THROUGH, WIN32_FIND_DATAW,
        },
    },
};

static UUID: AtomicU32 = AtomicU32::new(0);
static CANCELLABLES: Lazy<Mutex<HashMap<u32, u32>>> = Lazy::new(|| Mutex::new(HashMap::new()));
const PROGRESS_CANCEL: u32 = 1;
const FILE_NO_EXISTS: u32 = 4294967295;
const CANCEL_ERROR_CODE: HRESULT = HRESULT::from_win32(1235);

pub fn list_volumes() -> Result<Vec<Volume>, String> {
    let mut volumes: Vec<Volume> = Vec::new();

    let mut volume_name = vec![0u16; MAX_PATH as usize];
    let handle = unsafe { FindFirstVolumeW(&mut volume_name).map_err(|e| e.message()) }?;

    loop {
        let mut drive_paths = vec![0u16; 261];
        let mut len = 0;
        unsafe { GetVolumePathNamesForVolumeNameW(PCWSTR::from_raw(volume_name.as_ptr()), Some(&mut drive_paths), &mut len).map_err(|e| e.message()) }?;

        let mount_point = decode_wide(&drive_paths);

        let mut volume_label_ptr = vec![0u16; 261];
        unsafe { GetVolumeInformationW(PCWSTR(volume_name.as_ptr()), Some(&mut volume_label_ptr), None, None, None, None).map_err(|e| e.message()) }?;

        let volume_label = decode_wide(&volume_label_ptr);

        if mount_point.is_empty() {
            volumes.push(Volume {
                mount_point,
                volume_label,
                available_units: 0,
                total_units: 0,
            });
        } else {
            let mut available = 0;
            let mut total = 0;
            unsafe { GetDiskFreeSpaceExW(PCWSTR::from_raw(drive_paths.as_ptr()), None, Some(&mut total), Some(&mut available)).map_err(|e| e.message()) }?;
            volumes.push(Volume {
                mount_point,
                volume_label,
                available_units: available,
                total_units: total,
            });
        }

        volume_name = vec![0u16; MAX_PATH as usize];
        let next = unsafe { FindNextVolumeW(handle, &mut volume_name) };
        if next.is_err() {
            break;
        }
    }

    unsafe { FindVolumeClose(handle).map_err(|e| e.message()) }?;

    Ok(volumes)
}

pub fn get_file_attribute<P: AsRef<Path>>(file_path: P) -> Result<FileAttribute, String> {
    let wide = encode_wide(prefixed(file_path.as_ref()));
    let path = PCWSTR::from_raw(wide.as_ptr());

    let mut data: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };
    let handle = unsafe { FindFirstFileExW(path, FindExInfoBasic, &mut data as *mut _ as _, FindExSearchNameMatch, None, FIND_FIRST_EX_FLAGS(0)).map_err(|e| e.message()) }?;
    let file_attribute = get_attributes(&data);
    unsafe { FindClose(handle).map_err(|e| e.message()) }?;

    Ok(file_attribute)
}

fn to_msecs(low: u32, high: u32) -> f64 {
    let windows_epoch = 11644473600000.0; // FILETIME epoch (1601-01-01) to Unix epoch (1970-01-01) in milliseconds
    let ticks = ((high as u64) << 32) | low as u64;
    let milliseconds = ticks as f64 / 10_000.0; // FILETIME is in 100-nanosecond intervals

    milliseconds - windows_epoch
}

#[derive(PartialEq)]
enum FileType {
    Device,
    Link,
    Dir,
    File,
}

fn get_file_type(attr: u32) -> FileType {
    if attr & FILE_ATTRIBUTE_DEVICE.0 != 0 {
        return FileType::Device;
    }
    if attr & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 {
        return FileType::Link;
    }
    if attr & FILE_ATTRIBUTE_DIRECTORY.0 != 0 {
        return FileType::Dir;
    }

    FileType::File
}

fn get_attributes(data: &WIN32_FIND_DATAW) -> FileAttribute {
    let attributes = data.dwFileAttributes;
    let file_type = get_file_type(attributes);
    FileAttribute {
        is_directory: file_type == FileType::Dir,
        is_read_only: attributes & FILE_ATTRIBUTE_READONLY.0 != 0,
        is_hidden: attributes & FILE_ATTRIBUTE_HIDDEN.0 != 0,
        is_system: attributes & FILE_ATTRIBUTE_SYSTEM.0 != 0,
        is_device: file_type == FileType::Device,
        is_file: file_type == FileType::File,
        is_symbolic_link: file_type == FileType::Link,
        ctime: to_msecs(data.ftCreationTime.dwLowDateTime, data.ftCreationTime.dwHighDateTime),
        mtime: to_msecs(data.ftLastWriteTime.dwLowDateTime, data.ftLastWriteTime.dwHighDateTime),
        atime: to_msecs(data.ftLastAccessTime.dwLowDateTime, data.ftLastAccessTime.dwHighDateTime),
        size: (data.nFileSizeLow as u64) | ((data.nFileSizeHigh as u64) << 32),
    }
}

pub fn get_mime_type<P: AsRef<Path>>(file_path: P) -> Result<String, String> {
    let content_type = match mime_guess::from_path(file_path).first() {
        Some(s) => s.essence_str().to_string(),
        None => String::new(),
    };

    Ok(content_type)
}

pub fn readdir<P: AsRef<Path>>(directory: P, recursive: bool, with_mime_type: bool) -> Result<Vec<Dirent>, String> {
    let mut entries = Vec::new();

    if !directory.as_ref().is_dir() {
        return Ok(entries);
    }

    let mut search_path = directory.as_ref().to_path_buf();
    search_path.push("*");

    let wide = encode_wide(prefixed(search_path));
    let path = PCWSTR::from_raw(wide.as_ptr());
    let mut data: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };
    let handle = unsafe { FindFirstFileExW(path, FindExInfoBasic, &mut data as *mut _ as _, FindExSearchNameMatch, None, FIND_FIRST_EX_FLAGS(0)).map_err(|e| e.message()) }?;

    if handle.is_invalid() {
        return Ok(entries);
    }

    try_readdir(handle, directory, &mut entries, recursive, with_mime_type).unwrap();

    Ok(entries)
}

fn try_readdir<P: AsRef<Path>>(handle: HANDLE, parent: P, entries: &mut Vec<Dirent>, recursive: bool, with_mime_type: bool) -> Result<&mut Vec<Dirent>, String> {
    let mut data: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };

    while unsafe { FindNextFileW(handle, &mut data) }.is_ok() {
        let name = decode_wide(&data.cFileName);
        if name == "." || name == ".." {
            continue;
        }

        let mut full_path = parent.as_ref().to_path_buf();
        full_path.push(name.clone());

        let mime_type = if with_mime_type {
            match get_mime_type(&name) {
                Ok(n) => n,
                Err(e) => {
                    println!("{:?}", e);
                    println!("{:?}", name);
                    String::new()
                }
            }
        } else {
            String::new()
        };

        entries.push(Dirent {
            name: name.clone(),
            parent_path: parent.as_ref().to_string_lossy().to_string(),
            full_path: full_path.to_string_lossy().to_string(),
            attributes: get_attributes(&data),
            mime_type,
        });

        if data.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0 && recursive {
            let mut search_path = parent.as_ref().to_path_buf();
            search_path.push(name);
            let next_parent = search_path.clone();
            search_path.push("*");
            let wide = encode_wide(prefixed(search_path));
            let path = PCWSTR::from_raw(wide.as_ptr());
            let next_handle = unsafe { FindFirstFileExW(path, FindExInfoBasic, &mut data as *mut _ as _, FindExSearchNameMatch, None, FIND_FIRST_EX_FLAGS(0)).map_err(|e| e.message()) }?;
            if !next_handle.is_invalid() {
                try_readdir(next_handle, next_parent, entries, recursive, with_mime_type)?;
            }
        }
    }

    unsafe { FindClose(handle).map_err(|e| e.message()) }?;

    Ok(entries)
}

struct ProgressData<'a> {
    cancel_id: Option<u32>,
    callback: Option<&'a mut dyn FnMut(i64, i64)>,
    total: i64,
    prev: i64,
    processed: i64,
}

pub fn reserve_cancellable() -> u32 {
    let id = UUID.fetch_add(1, Ordering::Relaxed);

    let mut tokens = CANCELLABLES.lock().unwrap();
    tokens.insert(id, 0);

    id
}

pub fn mv<P: AsRef<Path>, P2: AsRef<Path>>(source_file: P, dest_file: P2, callback: Option<&mut dyn FnMut(i64, i64)>, cancel_id: Option<u32>) -> Result<(), String> {
    let result = inner_mv(source_file, dest_file, callback, cancel_id);
    clean_up(cancel_id);
    result
}

fn inner_mv<P: AsRef<Path>, P2: AsRef<Path>>(source_file: P, dest_file: P2, callback: Option<&mut dyn FnMut(i64, i64)>, cancel_id: Option<u32>) -> Result<(), String> {
    let source_wide = encode_wide(prefixed(source_file.as_ref()));
    let dest_wide = encode_wide(prefixed(dest_file.as_ref()));
    let source_file_fallback = source_file.as_ref();
    let dest_file_fallback = dest_file.as_ref();

    if let Some(callback) = callback {
        let data = Box::into_raw(Box::new(ProgressData {
            cancel_id,
            callback: Some(callback),
            total: 0,
            prev: 0,
            processed: 0,
        }));

        match unsafe {
            MoveFileWithProgressW(
                PCWSTR::from_raw(source_wide.as_ptr()),
                PCWSTR::from_raw(dest_wide.as_ptr()),
                Some(move_progress),
                Some(data as _),
                MOVEFILE_COPY_ALLOWED | MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } {
            Ok(_) => move_fallback(source_file_fallback, dest_file_fallback)?,
            Err(e) => handel_error(e, source_file_fallback, false)?,
        };
    } else {
        match unsafe { MoveFileExW(PCWSTR::from_raw(source_wide.as_ptr()), PCWSTR::from_raw(dest_wide.as_ptr()), MOVEFILE_COPY_ALLOWED | MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH) } {
            Ok(_) => move_fallback(source_file_fallback, dest_file_fallback)?,
            Err(e) => handel_error(e, source_file_fallback, false)?,
        };
    };

    Ok(())
}

pub fn mv_all<P: AsRef<Path>>(source_files: Vec<P>, dest_dir: P, callback: Option<&mut dyn FnMut(i64, i64)>, cancel_id: Option<u32>) -> Result<(), String> {
    let result = inner_mv_bulk(source_files, dest_dir, callback, cancel_id);
    clean_up(cancel_id);
    result
}

fn inner_mv_bulk<P: AsRef<Path>>(source_files: Vec<P>, dest_dir: P, callback: Option<&mut dyn FnMut(i64, i64)>, cancel_id: Option<u32>) -> Result<(), String> {
    if dest_dir.as_ref().is_file() {
        return Err("Destination is file".to_string());
    }

    if let Some(callback) = callback {
        let mut total: i64 = 0;
        let mut dest_files: Vec<PathBuf> = Vec::new();

        for i in 0..source_files.len() {
            let source_file = source_files.get(i).unwrap();
            let metadata = fs::metadata(source_file).unwrap();
            total += metadata.len() as i64;
            let name = source_file.as_ref().file_name().unwrap();
            let dest_file = dest_dir.as_ref().join(name);
            dest_files.push(dest_file);
        }

        let data = Box::into_raw(Box::new(ProgressData {
            cancel_id,
            callback: Some(callback),
            total,
            prev: 0,
            processed: 0,
        }));

        for (i, source_file) in source_files.iter().enumerate() {
            let dest_file = dest_files.get(i).unwrap();
            let source_file_fallback = source_file;
            let dest_file_fallback = dest_file.clone();

            let done = match unsafe {
                let source_wide = encode_wide(prefixed(source_file.as_ref()));
                let dest_wide = encode_wide(prefixed(dest_file));
                MoveFileWithProgressW(
                    PCWSTR::from_raw(source_wide.as_ptr()),
                    PCWSTR::from_raw(dest_wide.as_ptr()),
                    Some(move_files_progress),
                    Some(data as _),
                    MOVEFILE_COPY_ALLOWED | MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )
            } {
                Ok(_) => move_fallback(source_file_fallback, dest_file_fallback.as_path())?,
                Err(e) => handel_error(e, source_file_fallback, true)?,
            };

            if !done {
                break;
            }
        }
    } else {
        let mut dest_files: Vec<String> = Vec::new();

        for i in 0..source_files.len() {
            let source_file = source_files.get(i).unwrap();
            let name = source_file.as_ref().file_name().unwrap();
            let dest_file = dest_dir.as_ref().join(name);
            dest_files.push(dest_file.to_string_lossy().to_string());
        }

        for (i, source_file) in source_files.iter().enumerate() {
            let dest_file = dest_files.get(i).unwrap();
            let source_file_fallback = source_file.as_ref();
            let dest_file_fallback = dest_file.clone();

            let done = match unsafe {
                let source_wide = encode_wide(prefixed(source_file.as_ref()));
                let dest_wide = encode_wide(prefixed(dest_file));
                MoveFileExW(PCWSTR::from_raw(source_wide.as_ptr()), PCWSTR::from_raw(dest_wide.as_ptr()), MOVEFILE_COPY_ALLOWED | MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)
            } {
                Ok(_) => move_fallback(source_file_fallback, dest_file_fallback)?,
                Err(e) => handel_error(e, source_file_fallback, true)?,
            };

            if !done {
                break;
            }
        }
    }

    Ok(())
}

fn move_fallback<P: AsRef<Path>, P2: AsRef<Path>>(source_file: P, dest_file: P2) -> Result<bool, String> {
    let source_wide = encode_wide(source_file.as_ref());
    let dest_wide = encode_wide(dest_file.as_ref());

    let source_file_exists = unsafe { GetFileAttributesW(PCWSTR::from_raw(source_wide.as_ptr())) } != FILE_NO_EXISTS;
    let dest_file_exists = unsafe { GetFileAttributesW(PCWSTR::from_raw(dest_wide.as_ptr())) } != FILE_NO_EXISTS;
    if source_file_exists && dest_file_exists {
        unsafe { DeleteFileW(PCWSTR::from_raw(source_wide.as_ptr())) }.map_err(|e| e.message())?;
    }

    if source_file_exists && !dest_file_exists {
        return Err("Failed to move file.".to_string());
    }

    Ok(true)
}

fn handel_error<P: AsRef<Path>>(e: Error, file: P, treat_cancel_as_error: bool) -> Result<bool, String> {
    if e.code() != CANCEL_ERROR_CODE {
        return Err(format!("File: {}, Message: {}", file.as_ref().to_string_lossy(), e.message()));
    }

    if treat_cancel_as_error && e.code() == CANCEL_ERROR_CODE {
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

unsafe extern "system" fn move_progress(
    totalfilesize: i64,
    totalbytestransferred: i64,
    _streamsize: i64,
    _streambytestransferred: i64,
    _dwstreamnumber: u32,
    _dwcallbackreason: LPPROGRESS_ROUTINE_CALLBACK_REASON,
    _hsourcefile: HANDLE,
    _hdestinationfile: HANDLE,
    lpdata: *const c_void,
) -> u32 {
    let data_ptr = lpdata as *mut ProgressData;
    let data = unsafe { &mut *data_ptr };

    if let Some(callback) = data.callback.as_mut() {
        callback(totalfilesize, totalbytestransferred);
    }

    if let Some(cancel_id) = data.cancel_id {
        if let Ok(cancellables) = CANCELLABLES.try_lock() {
            if let Some(cancellable) = cancellables.get(&cancel_id) {
                return *cancellable;
            }
        }
    }

    0
}

unsafe extern "system" fn move_files_progress(
    _totalfilesize: i64,
    totalbytestransferred: i64,
    _streamsize: i64,
    _streambytestransferred: i64,
    _dwstreamnumber: u32,
    _dwcallbackreason: LPPROGRESS_ROUTINE_CALLBACK_REASON,
    _hsourcefile: HANDLE,
    _hdestinationfile: HANDLE,
    lpdata: *const c_void,
) -> u32 {
    let data_ptr = lpdata as *mut ProgressData;
    let data = unsafe { &mut *data_ptr };

    if totalbytestransferred - data.prev > 0 {
        data.processed += totalbytestransferred - data.prev;
    }
    data.prev = totalbytestransferred;

    if let Some(callback) = data.callback.as_mut() {
        callback(data.total, data.processed);
    }

    if let Some(cancel_id) = data.cancel_id {
        if let Ok(cancellables) = CANCELLABLES.try_lock() {
            if let Some(cancellable) = cancellables.get(&cancel_id) {
                return *cancellable;
            }
        }
    }

    0
}

pub fn cancel(id: u32) -> bool {
    if let Ok(mut tokens) = CANCELLABLES.try_lock() {
        if let Some(token) = tokens.get_mut(&id) {
            *token = PROGRESS_CANCEL;
            return true;
        }
    }
    false
}

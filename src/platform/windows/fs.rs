use super::util::{decode_wide, encode_wide, prefixed};
use crate::{FileAttribute, Volume};
use once_cell::sync::Lazy;
use std::{
    collections::HashMap,
    ffi::c_void,
    fs,
    path::Path,
    sync::{
        atomic::{AtomicU32, Ordering},
        Mutex,
    },
};
use windows::{
    core::{Error, HRESULT, PCWSTR},
    Win32::{
        Foundation::{HANDLE, HWND, MAX_PATH},
        Storage::FileSystem::{
            DeleteFileW, FindClose, FindExInfoBasic, FindExSearchNameMatch, FindFirstFileExW, FindFirstVolumeW, FindNextVolumeW, FindVolumeClose, GetFileAttributesW, GetVolumeInformationW,
            GetVolumePathNamesForVolumeNameW, MoveFileExW, MoveFileWithProgressW, FILE_ATTRIBUTE_DEVICE, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_READONLY,
            FILE_ATTRIBUTE_SYSTEM, FIND_FIRST_EX_FLAGS, LPPROGRESS_ROUTINE_CALLBACK_REASON, MOVEFILE_COPY_ALLOWED, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, WIN32_FIND_DATAW,
        },
        System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED},
        UI::Shell::{FileOperation, IFileOperation, IShellItem, SHCreateItemFromParsingName, ShellExecuteExW, FOF_ALLOWUNDO, SEE_MASK_INVOKEIDLIST, SHELLEXECUTEINFOW},
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

        volumes.push(Volume {
            mount_point,
            volume_label,
        });

        volume_name = vec![0u16; MAX_PATH as usize];
        let next = unsafe { FindNextVolumeW(handle, &mut volume_name) };
        if next.is_err() {
            break;
        }
    }

    unsafe { FindVolumeClose(handle).map_err(|e| e.message()) }?;

    Ok(volumes)
}

pub fn get_file_attribute(file_path: &str) -> Result<FileAttribute, String> {
    let wide = encode_wide(prefixed(file_path));
    let path = PCWSTR::from_raw(wide.as_ptr());

    let mut data: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };
    let handle = unsafe { FindFirstFileExW(path, FindExInfoBasic, &mut data as *mut _ as _, FindExSearchNameMatch, None, FIND_FIRST_EX_FLAGS(0)).map_err(|e| e.message()) }?;
    let attributes = data.dwFileAttributes;
    unsafe { FindClose(handle).map_err(|e| e.message()) }?;

    Ok(FileAttribute {
        directory: attributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0,
        read_only: attributes & FILE_ATTRIBUTE_READONLY.0 != 0,
        hidden: attributes & FILE_ATTRIBUTE_HIDDEN.0 != 0,
        system: attributes & FILE_ATTRIBUTE_SYSTEM.0 != 0,
        device: attributes & FILE_ATTRIBUTE_DEVICE.0 != 0,
        ctime: to_msecs(data.ftCreationTime.dwLowDateTime, data.ftCreationTime.dwHighDateTime),
        mtime: to_msecs(data.ftLastWriteTime.dwLowDateTime, data.ftLastWriteTime.dwHighDateTime),
        atime: to_msecs(data.ftLastAccessTime.dwLowDateTime, data.ftLastAccessTime.dwHighDateTime),
        size: (data.nFileSizeLow as u64) | ((data.nFileSizeHigh as u64) << 32),
    })
}

fn to_msecs(low: u32, high: u32) -> f64 {
    let windows_epoch = 11644473600000.0; // FILETIME epoch (1601-01-01) to Unix epoch (1970-01-01) in milliseconds
    let ticks = ((high as u64) << 32) | low as u64;
    let milliseconds = ticks as f64 / 10_000.0; // FILETIME is in 100-nanosecond intervals

    milliseconds - windows_epoch
}

pub fn open_path(window_handle: isize, file_path: String) -> Result<(), String> {
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };

    let wide_verb = encode_wide("open");
    let wide_path = encode_wide(file_path);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        hwnd: HWND(window_handle as _),
        lpVerb: PCWSTR::from_raw(wide_verb.as_ptr()),
        fMask: SEE_MASK_INVOKEIDLIST,
        lpFile: PCWSTR::from_raw(wide_path.as_ptr()),
        ..Default::default()
    };
    unsafe { ShellExecuteExW(&mut info).map_err(|e| e.message()) }?;

    Ok(())
}

pub fn open_file_property(window_handle: isize, file_path: String) -> Result<(), String> {
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };

    let wide_verb = encode_wide("properties");
    let wide_path = encode_wide(file_path);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        hwnd: HWND(window_handle as _),
        lpVerb: PCWSTR::from_raw(wide_verb.as_ptr()),
        fMask: SEE_MASK_INVOKEIDLIST,
        lpFile: PCWSTR::from_raw(wide_path.as_ptr()),
        ..Default::default()
    };
    unsafe { ShellExecuteExW(&mut info).map_err(|e| e.message()) }?;

    Ok(())
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

pub fn mv(source_file: String, dest_file: String, callback: Option<&mut dyn FnMut(i64, i64)>, cancel_id: Option<u32>) -> Result<(), String> {
    let result = inner_mv(source_file, dest_file, callback, cancel_id);
    clean_up(cancel_id);
    result
}

fn inner_mv(source_file: String, dest_file: String, callback: Option<&mut dyn FnMut(i64, i64)>, cancel_id: Option<u32>) -> Result<(), String> {
    let source_wide = encode_wide(prefixed(&source_file));
    let dest_wide = encode_wide(prefixed(&dest_file));
    let source_file_fallback = source_file.clone();
    let dest_file_fallback = dest_file.clone();

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

pub fn mv_bulk(source_files: Vec<String>, dest_dir: String, callback: Option<&mut dyn FnMut(i64, i64)>, cancel_id: Option<u32>) -> Result<(), String> {
    let result = inner_mv_bulk(source_files, dest_dir, callback, cancel_id);
    clean_up(cancel_id);
    result
}

fn inner_mv_bulk(source_files: Vec<String>, dest_dir: String, callback: Option<&mut dyn FnMut(i64, i64)>, cancel_id: Option<u32>) -> Result<(), String> {
    let dest_dir_path = Path::new(&dest_dir);
    if dest_dir_path.is_file() {
        return Err("Destination is file".to_string());
    }

    if let Some(callback) = callback {
        let mut total: i64 = 0;
        let mut dest_files: Vec<String> = Vec::new();
        let owned_source_files = source_files.clone();

        for source_file in source_files {
            let metadata = fs::metadata(&source_file).unwrap();
            total += metadata.len() as i64;
            let path = Path::new(&source_file);
            let name = path.file_name().unwrap();
            let dest_file = dest_dir_path.join(name);
            dest_files.push(dest_file.to_string_lossy().to_string());
        }

        let data = Box::into_raw(Box::new(ProgressData {
            cancel_id,
            callback: Some(callback),
            total,
            prev: 0,
            processed: 0,
        }));

        for (i, source_file) in owned_source_files.iter().enumerate() {
            let dest_file = dest_files.get(i).unwrap();
            let source_file_fallback = source_file.clone();
            let dest_file_fallback = dest_file.clone();

            let done = match unsafe {
                let source_wide = encode_wide(prefixed(source_file));
                let dest_wide = encode_wide(prefixed(dest_file));
                MoveFileWithProgressW(
                    PCWSTR::from_raw(source_wide.as_ptr()),
                    PCWSTR::from_raw(dest_wide.as_ptr()),
                    Some(move_files_progress),
                    Some(data as _),
                    MOVEFILE_COPY_ALLOWED | MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )
            } {
                Ok(_) => move_fallback(source_file_fallback, dest_file_fallback)?,
                Err(e) => handel_error(e, source_file_fallback, true)?,
            };

            if !done {
                break;
            }
        }
    } else {
        let mut dest_files: Vec<String> = Vec::new();
        let owned_source_files = source_files.clone();

        for source_file in source_files {
            let path = Path::new(&source_file);
            let name = path.file_name().unwrap();
            let dest_file = dest_dir_path.join(name);
            dest_files.push(dest_file.to_string_lossy().to_string());
        }

        for (i, source_file) in owned_source_files.iter().enumerate() {
            let dest_file = dest_files.get(i).unwrap();
            let source_file_fallback = source_file.clone();
            let dest_file_fallback = dest_file.clone();

            let done = match unsafe {
                let source_wide = encode_wide(prefixed(source_file));
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

fn move_fallback(source_file: String, dest_file: String) -> Result<bool, String> {
    let source_wide = encode_wide(&source_file);
    let dest_wide = encode_wide(&dest_file);

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

fn handel_error(e: Error, file: String, treat_cancel_as_error: bool) -> Result<bool, String> {
    if e.code() != CANCEL_ERROR_CODE {
        return Err(format!("File: {}, Message: {}", file, e.message()));
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

pub fn trash(file: String) -> Result<(), String> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        let op: IFileOperation = CoCreateInstance(&FileOperation, None, CLSCTX_ALL).map_err(|e| e.message())?;
        op.SetOperationFlags(FOF_ALLOWUNDO).map_err(|e| e.message())?;
        let file_wide = encode_wide(prefixed(&file));
        let shell_item: IShellItem = SHCreateItemFromParsingName(PCWSTR::from_raw(file_wide.as_ptr()), None).map_err(|e| e.message())?;
        op.DeleteItem(&shell_item, None).map_err(|e| e.message())?;
        op.PerformOperations().map_err(|e| e.message())?;
    }

    Ok(())
}

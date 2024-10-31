use once_cell::sync::Lazy;
use std::os::windows::ffi::OsStrExt;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU32, Ordering},
        Mutex,
    },
};
// use windows::core::Interface;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Shell::{IFileOperation, IShellItem, SHCreateItemFromParsingName, FOF_ALLOWUNDO};
use windows::{
    core::{Error, HRESULT, PCWSTR},
    Win32::{
        Foundation::HANDLE,
        Storage::FileSystem::{DeleteFileW, GetFileAttributesW, MoveFileWithProgressW, LPPROGRESS_ROUTINE_CALLBACK_REASON, MOVEFILE_COPY_ALLOWED, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH},
    },
};

static UUID: AtomicU32 = AtomicU32::new(0);
static CANCEL_TOKEN: Lazy<Mutex<HashMap<u32, u32>>> = Lazy::new(|| Mutex::new(HashMap::new()));
const PROGRESS_CANCEL: u32 = 1;
const FILE_NO_EXISTS: u32 = 4294967295;

struct ProgressData<'a> {
    id: Option<u32>,
    callback: Option<&'a mut dyn FnMut(i64, i64)>,
}

pub(crate) fn reserve() -> u32 {
    let id = UUID.fetch_add(1, Ordering::Relaxed);

    let mut tokens = CANCEL_TOKEN.lock().unwrap();
    tokens.insert(id, 0);

    id
}

pub(crate) fn trash(file: String) -> Result<(), String> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        let op: IFileOperation = CoCreateInstance(&windows::Win32::UI::Shell::FileOperation, None, CLSCTX_ALL).map_err(|e| e.message())?;
        op.SetOperationFlags(FOF_ALLOWUNDO).map_err(|e| e.message())?;
        let shell_item: IShellItem = SHCreateItemFromParsingName(to_file_path(file), None).map_err(|e| e.message())?;
        op.DeleteItem(&shell_item, None).map_err(|e| e.message())?;
        op.PerformOperations().map_err(|e| e.message())?;

        CoUninitialize();
    }

    Ok(())
}

pub(crate) fn mv(source_file: String, dest_file: String, id: Option<u32>) -> Result<(), String> {
    cancellable_move(source_file, dest_file, None, id)
}

pub(crate) fn mv_with_progress(source_file: String, dest_file: String, handler: &mut dyn FnMut(i64, i64), id: Option<u32>) -> Result<(), String> {
    cancellable_move(source_file, dest_file, Some(handler), id)
}

fn cancellable_move(source_file: String, dest_file: String, handler: Option<&mut dyn FnMut(i64, i64)>, id: Option<u32>) -> Result<(), String> {
    let data = Box::into_raw(Box::new(ProgressData {
        id,
        callback: handler,
    }));

    let source_file_fallback = source_file.clone();
    let dest_file_fallback = dest_file.clone();

    match unsafe {
        MoveFileWithProgressW(to_file_path(source_file), to_file_path(dest_file), Some(progress), Some(data as _), MOVEFILE_COPY_ALLOWED | MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)
    } {
        Ok(_) => move_fallback(source_file_fallback, dest_file_fallback)?,
        Err(e) => handle_move_error(e)?,
    };

    Ok(())
}

// pub fn mv_files(source_files: Vec<String>, dest_dir: String) -> Result<(), String> {
//     let dest_path = std::path::Path::new(&dest_dir);
//     if dest_path.is_file() {
//         return Err("Destination is file".to_string());
//     }

//     let mut total = 0;
//     let mut dest_files = Vec::new();

//     for source_file in source_files {
//         let metadata = std::fs::metadata(&source_file).unwrap();
//         total += metadata.len();
//         let path = std::path::Path::new(&source_file);
//         let name = path.file_name().unwrap();
//         let dest_file = dest_path.join(name);
//         dest_files.push(dest_file.to_string_lossy());
//     }

//     for (i, source_file) in source_files.clone().iter().enumerate() {
//         unsafe { CopyFileExW(to_file_path_str(source_file), to_file_path_str(dest_files.get(i).unwrap()), None, None, None, 0) }.unwrap();
//     }

//     Ok(())
// }

pub(crate) fn mv_sync(source_file: String, dest_file: String) -> Result<bool, String> {
    let source_file_fallback = source_file.clone();
    let dest_file_fallback = dest_file.clone();

    match unsafe { MoveFileWithProgressW(to_file_path(source_file), to_file_path(dest_file), None, None, MOVEFILE_COPY_ALLOWED | MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH) } {
        Ok(_) => move_fallback(source_file_fallback, dest_file_fallback)?,
        Err(e) => handle_move_error(e)?,
    };

    Ok(true)
}

fn move_fallback(source_file: String, dest_file: String) -> Result<bool, String> {
    let source_file_exists = unsafe { GetFileAttributesW(to_file_path_str(&source_file)) } != FILE_NO_EXISTS;
    let dest_file_exists = unsafe { GetFileAttributesW(to_file_path_str(&dest_file)) } != FILE_NO_EXISTS;

    if source_file_exists && dest_file_exists {
        unsafe { DeleteFileW(to_file_path_str(&source_file)) }.map_err(|e| e.message())?;
    }

    if source_file_exists && !dest_file_exists {
        return Err("Did not move file.".to_string());
    }

    Ok(true)
}

fn handle_move_error(e: Error) -> Result<bool, String> {
    if e.code() != HRESULT::from_win32(1235) {
        return Err(e.message());
    }
    Ok(true)
}

fn encode_wide(string: impl AsRef<std::ffi::OsStr>) -> Vec<u16> {
    string.as_ref().encode_wide().chain(std::iter::once(0)).collect()
}

fn to_file_path_str(orig_file_path: &str) -> PCWSTR {
    PCWSTR::from_raw(encode_wide(orig_file_path).as_ptr())
}

fn to_file_path(orig_file_path: String) -> PCWSTR {
    PCWSTR::from_raw(encode_wide(orig_file_path).as_ptr())
}

unsafe extern "system" fn progress(
    totalfilesize: i64,
    totalbytestransferred: i64,
    _streamsize: i64,
    _streambytestransferred: i64,
    _dwstreamnumber: u32,
    _dwcallbackreason: LPPROGRESS_ROUTINE_CALLBACK_REASON,
    _hsourcefile: HANDLE,
    _hdestinationfile: HANDLE,
    lpdata: *const core::ffi::c_void,
) -> u32 {
    let data_ptr = lpdata as *mut ProgressData;
    let data = unsafe { &mut *data_ptr };

    if let Some(callback) = data.callback.as_mut() {
        callback(totalfilesize, totalbytestransferred);
    }

    if let Some(id) = data.id {
        if let Ok(mut tokens) = CANCEL_TOKEN.try_lock() {
            if let Some(token) = tokens.get(&id) {
                if *token != 0 {
                    let token = tokens.remove(&id).unwrap();
                    return token;
                } else {
                    return *token;
                }
            }
        }
    }

    0
}

pub(crate) fn cancel(id: u32) -> bool {
    if let Ok(mut tokens) = CANCEL_TOKEN.try_lock() {
        if let Some(token) = tokens.get_mut(&id) {
            *token = PROGRESS_CANCEL;
            return true;
        }
    }

    false
}

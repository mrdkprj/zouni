use super::util::{decode_wide, encode_wide};
use crate::{ClipboardData, Operation};
use windows::{
    core::HRESULT,
    Win32::{
        Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND},
        System::{
            DataExchange::{CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW, SetClipboardData},
            Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE},
            Ole::{CF_HDROP, CF_TEXT, DROPEFFECT_COPY, DROPEFFECT_MOVE, DROPEFFECT_NONE},
        },
        UI::Shell::{DragQueryFileW, CFSTR_PREFERREDDROPEFFECT, DROPFILES, HDROP},
    },
};

pub fn is_text_availabel() -> bool {
    unsafe { IsClipboardFormatAvailable(CF_TEXT.0 as u32).is_ok() }
}

pub fn read_text(window_handle: isize) -> Result<String, String> {
    if !is_text_availabel() {
        return Ok(String::new());
    }

    let mut text = String::new();

    unsafe { OpenClipboard(HWND(window_handle as _)).map_err(|e| e.message()) }?;

    if let Ok(handle) = unsafe { GetClipboardData(CF_TEXT.0 as u32) } {
        let hglobal = HGLOBAL(handle.0);
        let ptr = unsafe { GlobalLock(hglobal) } as *const u8;
        if !ptr.is_null() {
            // Find the null terminator to determine the string length.
            let mut len = 0;
            while unsafe { *ptr.add(len) } != 0 {
                len += 1;
            }
            let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
            text = String::from_utf8_lossy(slice).to_string();
            unsafe { GlobalUnlock(hglobal).map_err(|e| e.message()) }?;
        }
    }

    unsafe { CloseClipboard().map_err(|e| e.message()) }?;

    Ok(text)
}

pub fn write_text(window_handle: isize, text: String) -> Result<(), String> {
    unsafe { OpenClipboard(HWND(window_handle as _)).map_err(|e| e.message()) }?;

    unsafe { EmptyClipboard().map_err(|e| e.message()) }?;

    let len = text.len() + 1; // Include null terminator.
    let hglobal = unsafe { GlobalAlloc(GMEM_MOVEABLE, len).map_err(|e| e.message()) }?;

    let ptr = unsafe { GlobalLock(hglobal) } as *mut u8;
    if ptr.is_null() {
        global_free(hglobal)?;
        return Err("Failed to lock memory".to_string());
    }

    unsafe { std::ptr::copy_nonoverlapping(text.as_ptr(), ptr, text.len()) };
    unsafe { *ptr.add(text.len()) = 0 }; // Add null terminator.
    let _ = unsafe { GlobalUnlock(hglobal) };
    if unsafe { SetClipboardData(CF_TEXT.0 as u32, HANDLE(hglobal.0)).is_err() } {
        unsafe { CloseClipboard().map_err(|e| e.message()) }?;
        global_free(hglobal)?;
        return Err("Failed to write clipboard".to_string());
    }

    unsafe { CloseClipboard().map_err(|e| e.message()) }?;

    Ok(())
}

pub fn is_uris_available() -> bool {
    unsafe { IsClipboardFormatAvailable(CF_HDROP.0 as u32).is_ok() }
}

pub fn read_uris(window_handle: isize) -> Result<ClipboardData, String> {
    let mut data = ClipboardData {
        operation: Operation::None,
        urls: Vec::new(),
    };

    if !is_uris_available() {
        return Ok(data);
    }

    let mut urls = Vec::new();

    unsafe { OpenClipboard(HWND(window_handle as _)).map_err(|e| e.message()) }?;

    let operation = get_preferred_drop_effect();

    if let Ok(handle) = unsafe { GetClipboardData(CF_HDROP.0 as u32) } {
        let hdrop = HDROP(handle.0);
        let count = unsafe { DragQueryFileW(hdrop, 0xFFFFFFFF, None) };
        for i in 0..count {
            // Get the length of the file path
            let len = unsafe { DragQueryFileW(hdrop, i, None) } as usize;

            // Create a buffer to hold the file path
            let mut buffer = vec![0u16; len + 1];

            // Retrieve the file path
            unsafe { DragQueryFileW(hdrop, i, Some(&mut buffer)) };

            urls.push(decode_wide(&buffer));
        }
    }

    unsafe { CloseClipboard().map_err(|e| e.message()) }?;

    data.operation = operation;
    data.urls = urls;

    Ok(data)
}

pub fn write_uris(window_handle: isize, paths: &[String], operation: Operation) -> Result<(), String> {
    let mut file_list = paths.join("\0");
    // Append null to the last file
    file_list.push('\0');
    // Append null to the last
    file_list.push('\0');

    let mut total_size = std::mem::size_of::<u32>();
    for path in paths {
        let path_wide: Vec<u16> = encode_wide(path);
        total_size += path_wide.len() * 2;
    }
    total_size += std::mem::size_of::<DROPFILES>();
    // Double null terminator
    total_size += 2;

    // Calculate the size needed for the DROPFILES structure and file list
    let dropfiles_size = std::mem::size_of::<DROPFILES>();
    let file_list_size = file_list.len() * std::mem::size_of::<u16>();

    let hglobal = unsafe { GlobalAlloc(GMEM_MOVEABLE, total_size).map_err(|e| e.message()) }?;

    // Lock the memory to write to it
    let ptr = unsafe { GlobalLock(hglobal) } as *mut u8;
    if ptr.is_null() {
        global_free(hglobal)?;
        return Err("Failed to lock memory".to_string());
    }

    let dropfiles = DROPFILES {
        pFiles: dropfiles_size as u32,
        pt: Default::default(),
        fNC: false.into(),
        fWide: true.into(),
    };
    unsafe { std::ptr::copy_nonoverlapping(&dropfiles as *const _ as *const u8, ptr, dropfiles_size) };

    // Write the file list as wide characters (UTF-16)
    let wide_file_list: Vec<u16> = file_list.encode_utf16().collect();
    unsafe { std::ptr::copy_nonoverlapping(wide_file_list.as_ptr() as *const u8, ptr.add(dropfiles_size), file_list_size) };

    let _ = unsafe { GlobalUnlock(hglobal) };

    unsafe { OpenClipboard(HWND(window_handle as _)).map_err(|e| e.message()) }?;
    unsafe { EmptyClipboard().map_err(|e| e.message()) }?;

    if unsafe { SetClipboardData(CF_HDROP.0 as u32, HANDLE(hglobal.0)).is_err() } {
        unsafe { CloseClipboard().map_err(|e| e.message()) }?;
        global_free(hglobal)?;
        return Err("Failed to write clipboard".to_string());
    }

    let operation_value = match operation {
        Operation::Copy => DROPEFFECT_COPY.0,
        Operation::Move => DROPEFFECT_MOVE.0,
        Operation::None => DROPEFFECT_NONE.0,
    };

    let hglobal_operation = unsafe { GlobalAlloc(GMEM_MOVEABLE, std::mem::size_of::<u32>()).map_err(|e| e.message()) }?;

    let ptr_operation = unsafe { GlobalLock(hglobal_operation) } as *mut u32;
    if ptr_operation.is_null() {
        global_free(hglobal_operation)?;
        return Err("Failed to lock memory".to_string());
    }

    unsafe { *ptr_operation = operation_value };

    let _ = unsafe { GlobalUnlock(hglobal_operation) };

    let custom_format = unsafe { RegisterClipboardFormatW(CFSTR_PREFERREDDROPEFFECT) };

    if unsafe { SetClipboardData(custom_format, HANDLE(hglobal_operation.0)).is_err() } {
        unsafe { CloseClipboard().map_err(|e| e.message()) }?;
        global_free(hglobal_operation)?;
        return Err("Failed to write clipboard2".to_string());
    }

    unsafe { CloseClipboard().map_err(|e| e.message()) }?;

    Ok(())
}

fn global_free(hglobal: HGLOBAL) -> Result<(), String> {
    match unsafe { GlobalFree(hglobal) } {
        Ok(_) => Ok(()),
        Err(err) => {
            if err.code() == HRESULT(0x00000000) {
                Ok(())
            } else {
                Err(err.message())
            }
        }
    }
}

fn get_preferred_drop_effect() -> Operation {
    let cf_format = unsafe { RegisterClipboardFormatW(CFSTR_PREFERREDDROPEFFECT) };
    if cf_format == 0 {
        return Operation::None;
    }

    if unsafe { IsClipboardFormatAvailable(cf_format).is_ok() } {
        if let Ok(handle) = unsafe { GetClipboardData(cf_format) } {
            let hglobal = windows::Win32::Foundation::HGLOBAL(handle.0);
            let ptr = unsafe { GlobalLock(hglobal) } as *const u32;
            if !ptr.is_null() {
                let drop_effect = unsafe { *ptr };
                let _ = unsafe { GlobalUnlock(hglobal) };
                if (drop_effect & DROPEFFECT_COPY.0) != 0 {
                    return Operation::Copy;
                }

                if (drop_effect & DROPEFFECT_MOVE.0) != 0 {
                    return Operation::Move;
                }

                return Operation::None;
            }
        }
    }

    Operation::None
}

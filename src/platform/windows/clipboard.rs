use super::util::{decode_wide, encode_wide, GlobalMemory};
use crate::{ClipboardData, Operation};
use windows::Win32::{
    Foundation::{HANDLE, HGLOBAL, HWND},
    Graphics::Gdi::{BITMAPINFOHEADER, BI_RGB},
    System::{
        DataExchange::{CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW, SetClipboardData},
        Memory::{GlobalLock, GlobalUnlock},
        Ole::{CF_DIB, CF_HDROP, CF_TEXT, CF_UNICODETEXT, DROPEFFECT_COPY, DROPEFFECT_MOVE, DROPEFFECT_NONE},
    },
    UI::Shell::{DragQueryFileW, CFSTR_PREFERREDDROPEFFECT, DROPFILES, HDROP},
};

/// Checks if text is available
pub fn is_text_available() -> bool {
    is_ansi_text_available() || is_unicode_text_available()
}

fn is_ansi_text_available() -> bool {
    unsafe { IsClipboardFormatAvailable(CF_TEXT.0 as u32).is_ok() }
}

fn is_unicode_text_available() -> bool {
    unsafe { IsClipboardFormatAvailable(CF_UNICODETEXT.0 as u32).is_ok() }
}

/// Reads text from clipboard
pub fn read_text(window_handle: isize) -> Result<String, String> {
    if !is_text_available() {
        return Ok(String::new());
    }

    let mut text = String::new();

    unsafe { OpenClipboard(Some(HWND(window_handle as _))).map_err(|e| e.message()) }?;

    let format = if is_unicode_text_available() {
        CF_UNICODETEXT.0 as u32
    } else {
        CF_TEXT.0 as u32
    };

    if let Ok(handle) = unsafe { GetClipboardData(format) } {
        let hglobal = HGLOBAL(handle.0);
        let ptr = unsafe { GlobalLock(hglobal) } as *const u8;

        if ptr.is_null() {
            unsafe { CloseClipboard().map_err(|e| e.message()) }?;
            return Err("Failed to lock global memory.".to_string());
        }

        // Find the null terminator to determine the string length.
        let mut len = 0;
        while unsafe { *ptr.add(len) } != 0 {
            len += 1;
        }

        if format == CF_UNICODETEXT.0 as u32 {
            let slice = unsafe { std::slice::from_raw_parts(ptr as *const u16, len) };
            text = decode_wide(slice);
        } else {
            let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
            text = String::from_utf8_lossy(slice).to_string();
        }

        let _ = unsafe { GlobalUnlock(hglobal) };
    }

    unsafe { CloseClipboard().map_err(|e| e.message()) }?;

    Ok(text)
}

/// Writes text to clipboard
pub fn write_text(window_handle: isize, text: String) -> Result<(), String> {
    unsafe { OpenClipboard(Some(HWND(window_handle as _))).map_err(|e| e.message()) }?;

    unsafe { EmptyClipboard().map_err(|e| e.message()) }?;

    let utf16 = encode_wide(text);
    let size_in_bytes = utf16.len() * std::mem::size_of::<u16>();
    let hglobal = GlobalMemory::new(size_in_bytes)?;

    let ptr = hglobal.lock()?;

    unsafe { std::ptr::copy_nonoverlapping(utf16.as_ptr(), ptr as *mut u16, utf16.len()) };

    hglobal.unlock();

    if unsafe { SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(hglobal.handle().0))).is_err() } {
        unsafe { CloseClipboard().map_err(|e| e.message()) }?;
        return Err("Failed to write clipboard".to_string());
    }

    unsafe { CloseClipboard().map_err(|e| e.message()) }
}

/// Checks if URIs are available
pub fn is_uris_available() -> bool {
    unsafe { IsClipboardFormatAvailable(CF_HDROP.0 as u32).is_ok() }
}

/// Reads URIs from clipboard
pub fn read_uris(window_handle: isize) -> Result<ClipboardData, String> {
    let mut data = ClipboardData {
        operation: Operation::None,
        urls: Vec::new(),
    };

    if !is_uris_available() {
        return Ok(data);
    }

    let mut urls = Vec::new();

    unsafe { OpenClipboard(Some(HWND(window_handle as _))).map_err(|e| e.message()) }?;

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

/// Writes URIs to clipboard
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

    let hglobal = GlobalMemory::new(total_size)?;

    // Lock the memory to write to it
    let ptr = hglobal.lock()?;

    let dropfiles = DROPFILES {
        pFiles: dropfiles_size as u32,
        pt: Default::default(),
        fNC: false.into(),
        fWide: true.into(),
    };

    unsafe { std::ptr::write(ptr as *mut DROPFILES, dropfiles) };

    // Write the file list as wide characters (UTF-16)
    let wide_file_list: Vec<u16> = file_list.encode_utf16().collect();
    let dest = unsafe { ptr.add(dropfiles_size) } as *mut u16;
    let src = wide_file_list.as_ptr();
    let len = wide_file_list.len();
    unsafe {
        std::ptr::copy_nonoverlapping(src, dest, len);
    };

    hglobal.unlock();

    unsafe { OpenClipboard(Some(HWND(window_handle as _))).map_err(|e| e.message()) }?;
    unsafe { EmptyClipboard().map_err(|e| e.message()) }?;

    if unsafe { SetClipboardData(CF_HDROP.0 as u32, Some(HANDLE(hglobal.handle().0))).is_err() } {
        unsafe { CloseClipboard().map_err(|e| e.message()) }?;
        return Err("Failed to write clipboard".to_string());
    }

    let operation_value = match operation {
        Operation::Copy => DROPEFFECT_COPY.0,
        Operation::Move => DROPEFFECT_MOVE.0,
        Operation::None => DROPEFFECT_NONE.0,
    };

    let hglobal_operation = GlobalMemory::new(std::mem::size_of::<u32>())?;

    let ptr_operation = hglobal_operation.lock()?;

    unsafe { *ptr_operation = operation_value as u8 };

    hglobal_operation.unlock();

    let custom_format = unsafe { RegisterClipboardFormatW(CFSTR_PREFERREDDROPEFFECT) };

    if unsafe { SetClipboardData(custom_format, Some(HANDLE(hglobal_operation.handle().0))).is_err() } {
        unsafe { CloseClipboard().map_err(|e| e.message()) }?;
        return Err("Failed to write clipboard format".to_string());
    }

    unsafe { CloseClipboard().map_err(|e| e.message()) }
}

fn get_preferred_drop_effect() -> Operation {
    let cf_format = unsafe { RegisterClipboardFormatW(CFSTR_PREFERREDDROPEFFECT) };
    if cf_format == 0 {
        return Operation::None;
    }

    if unsafe { IsClipboardFormatAvailable(cf_format).is_ok() } {
        if let Ok(handle) = unsafe { GetClipboardData(cf_format) } {
            let hglobal = HGLOBAL(handle.0);
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

/// Copies an image buffer to the clipboard as CF_DIB.
pub fn write_image(window_handle: isize, file_path: String) -> Result<(), String> {
    let img = image::open(&file_path).map_err(|e| e.to_string())?;

    let width = img.width();
    let height = img.height();
    let rgba_img = img.to_rgba8();

    let mut bgra_data = Vec::with_capacity((width * height * 4) as usize);
    for y in (0..height).rev() {
        for x in 0..width {
            let pixel = rgba_img.get_pixel(x, y);
            bgra_data.push(pixel[2]); // Blue
            bgra_data.push(pixel[1]); // Green
            bgra_data.push(pixel[0]); // Red
            bgra_data.push(pixel[3]); // Alpha
        }
    }

    let header_size = size_of::<BITMAPINFOHEADER>() as u32;
    let image_size = bgra_data.len() as u32;
    let total_size = (header_size + image_size) as usize;

    let hglobal = GlobalMemory::new(total_size)?;

    let ptr = hglobal.lock()?;

    let header = BITMAPINFOHEADER {
        biSize: header_size,
        biWidth: width as i32,
        biHeight: height as i32, // Positive value dictates a bottom-up DIB
        biPlanes: 1,
        biBitCount: 32,          // 32-bit BGRA
        biCompression: BI_RGB.0, // BI_RGB (no compression)
        biSizeImage: image_size,
        biXPelsPerMeter: 0,
        biYPelsPerMeter: 0,
        biClrUsed: 0,
        biClrImportant: 0,
    };

    // Copy header and pixel data into the global memory pointer
    unsafe { std::ptr::copy_nonoverlapping(&header as *const BITMAPINFOHEADER as *const u8, ptr, header_size as usize) };
    unsafe { std::ptr::copy_nonoverlapping(bgra_data.as_ptr(), ptr.add(header_size as usize), image_size as usize) };

    hglobal.unlock();

    unsafe { OpenClipboard(Some(HWND(window_handle as _))).map_err(|e| e.message()) }?;
    unsafe { EmptyClipboard().map_err(|e| e.message()) }?;

    if unsafe { SetClipboardData(CF_DIB.0 as u32, Some(HANDLE(hglobal.handle().0))) }.is_err() {
        unsafe { CloseClipboard().map_err(|e| e.message()) }?;
        return Err("Failed to write clipboard".to_string());
    }

    unsafe { CloseClipboard().map_err(|e| e.message()) }
}

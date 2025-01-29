use super::util::{decode_wide, encode_wide, prefixed, ComGuard};
use crate::{AppInfo, RgbaIcon};
use std::path::Path;
use windows::{
    core::{HSTRING, PCWSTR, PWSTR},
    Management::Deployment::PackageManager,
    Win32::{
        Foundation::{HWND, MAX_PATH},
        Graphics::Gdi::{CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, SelectObject, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, HDC},
        System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_ALL},
        UI::{
            Shell::{
                ExtractIconExW, FileOperation, IFileOperation, IShellItem, SHAssocEnumHandlers, SHCreateItemFromParsingName, SHLoadIndirectString, SHOpenFolderAndSelectItems, SHParseDisplayName,
                ShellExecuteExW, ASSOC_FILTER_RECOMMENDED, FOF_ALLOWUNDO, SEE_MASK_INVOKEIDLIST, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
            },
            WindowsAndMessaging::{GetIconInfo, HICON, ICONINFO},
        },
    },
};

const SW_SHOWNORMAL: i32 = 1;

pub fn open_path<P: AsRef<Path>>(file_path: P) -> Result<(), String> {
    let _ = ComGuard::new();

    let wide_verb = encode_wide("open");
    let wide_path = encode_wide(file_path.as_ref());
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        hwnd: HWND::default(),
        lpVerb: PCWSTR::from_raw(wide_verb.as_ptr()),
        fMask: SEE_MASK_INVOKEIDLIST,
        lpFile: PCWSTR::from_raw(wide_path.as_ptr()),
        nShow: SW_SHOWNORMAL,
        ..Default::default()
    };
    unsafe { ShellExecuteExW(&mut info).map_err(|e| e.message()) }?;

    Ok(())
}

pub fn open_path_with<P1: AsRef<Path>, P2: AsRef<Path>>(file_path: P1, app_path: P2) -> Result<(), String> {
    let _ = ComGuard::new();

    let app_path = encode_wide(app_path.as_ref());
    let file_path = encode_wide(file_path.as_ref());
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        hwnd: HWND::default(),
        lpFile: PCWSTR::from_raw(app_path.as_ptr()),
        lpParameters: PCWSTR::from_raw(file_path.as_ptr()),
        fMask: SEE_MASK_NOCLOSEPROCESS,
        nShow: SW_SHOWNORMAL,
        ..Default::default()
    };
    unsafe { ShellExecuteExW(&mut info).map_err(|e| e.message()) }?;

    Ok(())
}

pub fn show_open_with_dialog<P: AsRef<Path>>(file_path: P) -> Result<(), String> {
    let _ = ComGuard::new();

    let wide_verb = encode_wide("openas");
    let wide_path = encode_wide(file_path.as_ref());
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        hwnd: HWND::default(),
        lpVerb: PCWSTR::from_raw(wide_verb.as_ptr()),
        fMask: SEE_MASK_INVOKEIDLIST,
        lpFile: PCWSTR::from_raw(wide_path.as_ptr()),
        ..Default::default()
    };
    unsafe { ShellExecuteExW(&mut info).map_err(|e| e.message()) }?;

    Ok(())
}

pub fn get_open_with<P: AsRef<Path>>(file_path: P) -> Vec<AppInfo> {
    let mut apps = Vec::new();

    if let Some(extension_name) = file_path.as_ref().extension() {
        let _ = ComGuard::new();
        let mut extension = String::from(".");
        extension.push_str(extension_name.to_str().unwrap());
        let file_extension = encode_wide(extension);

        if let Ok(enum_handlers) = unsafe { SHAssocEnumHandlers(PCWSTR::from_raw(file_extension.as_ptr()), ASSOC_FILTER_RECOMMENDED) } {
            loop {
                let mut handlers = [None; 1];
                let len = None;
                let result = unsafe { enum_handlers.Next(&mut handlers, len) };

                if result.is_err() || handlers[0].is_none() {
                    break;
                }

                if let Some(handler) = handlers[0].take() {
                    let mut path = match unsafe { handler.GetName() } {
                        Ok(path_ptr) => decode_wide(unsafe { path_ptr.as_wide() }),
                        Err(_) => String::new(),
                    };

                    let name = match unsafe { handler.GetUIName() } {
                        Ok(name_ptr) => decode_wide(unsafe { name_ptr.as_wide() }),
                        Err(_) => String::new(),
                    };

                    let mut icon_path = PWSTR::null();
                    let mut index = 0;
                    let icon_location = unsafe { handler.GetIconLocation(&mut icon_path, &mut index) };
                    let uwp = if icon_location.is_ok() {
                        is_uwp(icon_path)
                    } else {
                        false
                    };
                    let icon = if uwp {
                        get_icon_path(icon_path)
                    } else {
                        String::new()
                    };

                    let rgba_icon = if icon.is_empty() && icon_location.is_ok() {
                        to_rgba_bitmap(icon_path, index).unwrap_or_default()
                    } else {
                        RgbaIcon::default()
                    };

                    if uwp {
                        if let Some(model_id) = extract_app_user_model_id(icon_path) {
                            let manager = PackageManager::new().unwrap();
                            let pkg = manager.FindPackageByUserSecurityIdPackageFullName(&HSTRING::new(), &HSTRING::from(&model_id)).unwrap();

                            let ent = pkg.GetAppListEntries().unwrap().GetAt(0).unwrap();
                            let model_id = ent.AppUserModelId().unwrap();
                            path = format!(r#"shell:AppsFolder\{}"#, &model_id);
                        }
                    }

                    apps.push(AppInfo {
                        path,
                        name,
                        icon,
                        rgba_icon,
                    });
                }
            }
        }
    }

    apps
}

fn extract_app_user_model_id(input: PWSTR) -> Option<String> {
    let input_string = decode_wide(unsafe { input.as_wide() });
    if let Some(start) = input_string.find('{') {
        if let Some(end) = input_string.find('?') {
            return Some(input_string[start + 1..end].to_string());
        }
    }
    None
}

fn is_uwp(icon_location: PWSTR) -> bool {
    let icon_path = decode_wide(unsafe { icon_location.as_wide() });
    icon_path.starts_with("@")
}

/* Get actual icon path if path starts with "@" */
fn get_icon_path(icon_location: PWSTR) -> String {
    let icon_path = decode_wide(unsafe { icon_location.as_wide() });
    let wide_path = encode_wide(icon_path);
    let mut actual_path: [u16; MAX_PATH as _] = [0; MAX_PATH as _];
    unsafe { SHLoadIndirectString(PCWSTR(wide_path.as_ptr()), &mut actual_path, None).map_err(|e| e.message()) }.unwrap();

    decode_wide(&actual_path)
}

/* Get rgba from icon path */
fn to_rgba_bitmap(icon_path: PWSTR, icon_index: i32) -> Result<RgbaIcon, String> {
    let icon_path = decode_wide(unsafe { icon_path.as_wide() });

    if let Some(hicon) = extract_icon(&icon_path, icon_index) {
        let mut icon_info = ICONINFO::default();
        unsafe { GetIconInfo(hicon, &mut icon_info).map_err(|e| e.message()) }?;

        // Retrieve bitmap details
        let mut bitmap = BITMAP::default();
        if unsafe { GetObjectW(icon_info.hbmColor, std::mem::size_of::<BITMAP>() as i32, Some(&mut bitmap as *mut _ as *mut _)) } == 0 {
            return Err("Failed to get bitmap details".to_string());
        }

        let width = bitmap.bmWidth as u32;
        let height = bitmap.bmHeight as u32;

        let hdc = unsafe { CreateCompatibleDC(HDC::default()) };
        if hdc.is_invalid() {
            return Err("Failed to create compatible DC".to_string());
        }

        let mut bitmap_info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32), // Negative height for top-down bitmap
                biPlanes: 1,
                biBitCount: 32,
                biCompression: 0,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut pixel_data: Vec<u8> = vec![0; (width * height * 4) as usize];

        // Retrieve the RGBA pixel data
        unsafe { SelectObject(hdc, icon_info.hbmColor) };
        if unsafe { GetDIBits(hdc, icon_info.hbmColor, 0, height, Some(pixel_data.as_mut_ptr() as *mut _), &mut bitmap_info, DIB_RGB_COLORS) } == 0 {
            let _ = unsafe { DeleteDC(hdc) };
            return Err("Failed to retrieve pixel data".to_string());
        }

        let _ = unsafe { DeleteDC(hdc) };
        let _ = unsafe { DeleteObject(icon_info.hbmColor) };
        let _ = unsafe { DeleteObject(icon_info.hbmMask) };

        return Ok(RgbaIcon {
            rgba: pixel_data,
            width,
            height,
        });
    }

    Err("Not found".to_string())
}

fn extract_icon(icon_path: &str, icon_index: i32) -> Option<HICON> {
    if icon_path.is_empty() {
        return None;
    }

    let icon_path_w = encode_wide(icon_path);
    let mut small_icon: [HICON; 1] = [HICON::default()];

    let count = unsafe { ExtractIconExW(PCWSTR(icon_path_w.as_ptr()), icon_index, None, Some(small_icon.as_mut_ptr()), 1) };
    if count > 0 {
        return Some(small_icon[0]);
    }

    None
}

pub fn open_file_property<P: AsRef<Path>>(file_path: P) -> Result<(), String> {
    let _ = ComGuard::new();

    let wide_verb = encode_wide("properties");
    let wide_path = encode_wide(file_path.as_ref());
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        hwnd: HWND::default(),
        lpVerb: PCWSTR::from_raw(wide_verb.as_ptr()),
        fMask: SEE_MASK_INVOKEIDLIST,
        lpFile: PCWSTR::from_raw(wide_path.as_ptr()),
        ..Default::default()
    };
    unsafe { ShellExecuteExW(&mut info).map_err(|e| e.message()) }?;

    Ok(())
}

pub fn show_item_in_folder<P: AsRef<Path>>(file_path: P) -> Result<(), String> {
    let _ = ComGuard::new();

    let wide_path = encode_wide(file_path.as_ref());
    let mut idlist = std::ptr::null_mut();
    unsafe { SHParseDisplayName(PCWSTR::from_raw(wide_path.as_ptr()), None, &mut idlist, 0, None).map_err(|e| e.message()) }?;
    if !idlist.is_null() {
        let _ = unsafe { SHOpenFolderAndSelectItems(idlist, None, 0) };
        unsafe { CoTaskMemFree(Some(idlist as _)) };
    }

    Ok(())
}

pub fn trash<P: AsRef<Path>>(file_path: P) -> Result<(), String> {
    let _ = ComGuard::new();

    let op: IFileOperation = unsafe { CoCreateInstance(&FileOperation, None, CLSCTX_ALL).map_err(|e| e.message()) }?;
    unsafe { op.SetOperationFlags(FOF_ALLOWUNDO).map_err(|e| e.message()) }?;
    let file_wide = encode_wide(prefixed(file_path.as_ref()));
    let shell_item: IShellItem = unsafe { SHCreateItemFromParsingName(PCWSTR::from_raw(file_wide.as_ptr()), None).map_err(|e| e.message()) }?;
    unsafe { op.DeleteItem(&shell_item, None).map_err(|e| e.message()) }?;
    unsafe { op.PerformOperations().map_err(|e| e.message()) }?;

    Ok(())
}

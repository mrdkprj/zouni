use super::util::{decode_wide, encode_wide, prefixed, ComGuard};
use crate::AppInfo;
use std::path::Path;
use windows::{
    core::{PCWSTR, PWSTR},
    Win32::{
        Foundation::HWND,
        System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_ALL},
        UI::Shell::{
            FileOperation, IFileOperation, IShellItem, SHAssocEnumHandlers, SHCreateItemFromParsingName, SHOpenFolderAndSelectItems, SHParseDisplayName, ShellExecuteExW, ASSOC_FILTER_RECOMMENDED,
            FOF_ALLOWUNDO, SEE_MASK_INVOKEIDLIST, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
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
                    let path_ptr = unsafe { handler.GetName().unwrap_or(PWSTR::null()) };
                    let path = decode_wide(unsafe { path_ptr.as_wide() });
                    let name_ptr = unsafe { handler.GetUIName().unwrap_or(PWSTR::null()) };
                    let name = decode_wide(unsafe { name_ptr.as_wide() });
                    let mut icon_path = PWSTR::null();
                    let mut index = 0;
                    let icon_location = unsafe { handler.GetIconLocation(&mut icon_path, &mut index) };
                    let icon = if icon_location.is_ok() {
                        decode_wide(unsafe { icon_path.as_wide() })
                    } else {
                        String::new()
                    };
                    apps.push(AppInfo {
                        path,
                        name,
                        icon,
                    });
                }
            }
        }
    }

    apps
}

/*
fn extract_icon(icon_path: &str, icon_index: i32) -> Option<HICON> {
    let icon_path_w: Vec<u16> = icon_path.encode_utf16().chain(Some(0)).collect();
    let mut large_icon: [HICON; 1] = [HICON::default()];
    let mut small_icon: [HICON; 1] = [HICON::default()];

    unsafe {
        let count = ExtractIconExW(PCWSTR(icon_path_w.as_ptr()), icon_index, large_icon.as_mut_ptr(), small_icon.as_mut_ptr(), 1);
        if count > 0 {
            return Some(large_icon[0]); // Return the large icon
        }
    }
    None
}
 */

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

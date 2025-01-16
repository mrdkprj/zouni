use super::util::{encode_wide, prefixed, ComGuard};
use std::path::Path;
use windows::Win32::{
    Foundation::HWND,
    System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_ALL},
    UI::Shell::{
        FileOperation, IFileOperation, IShellItem, SHCreateItemFromParsingName, SHOpenFolderAndSelectItems, SHParseDisplayName, ShellExecuteExW, FOF_ALLOWUNDO, SEE_MASK_INVOKEIDLIST,
        SHELLEXECUTEINFOW,
    },
};
use windows_core::PCWSTR;

pub fn open_path<P: AsRef<Path>>(window_handle: isize, file_path: P) -> Result<(), String> {
    let _ = ComGuard::new();

    let wide_verb = encode_wide("open");
    let wide_path = encode_wide(file_path.as_ref());
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

pub fn open_path_with<P: AsRef<Path>>(window_handle: isize, file_path: P) -> Result<(), String> {
    let _ = ComGuard::new();

    let wide_verb = encode_wide("openas");
    let wide_path = encode_wide(file_path.as_ref());
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

pub fn open_file_property<P: AsRef<Path>>(window_handle: isize, file_path: P) -> Result<(), String> {
    let _ = ComGuard::new();

    let wide_verb = encode_wide("properties");
    let wide_path = encode_wide(file_path.as_ref());
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

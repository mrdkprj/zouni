use super::util::{decode_wide, encode_wide, ComGuard};
use crate::{AppInfo, RgbaIcon, ThumbButton};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::OnceLock,
};
use windows::{
    core::{HSTRING, PCWSTR, PWSTR},
    Management::Deployment::PackageManager,
    Win32::{
        Foundation::{GENERIC_READ, HWND, LPARAM, LRESULT, MAX_PATH, WPARAM},
        Graphics::{
            Gdi::{CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDIBits, GetObjectW, SelectObject, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, HDC},
            Imaging::{CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICImagingFactory, WICBitmapDitherTypeNone, WICBitmapPaletteTypeCustom, WICDecodeMetadataCacheOnDemand},
        },
        System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_INPROC_SERVER},
        UI::{
            Shell::{
                DefSubclassProc, ExtractIconExW, ITaskbarList3, RemoveWindowSubclass, SHAssocEnumHandlers, SHLoadIndirectString, SHOpenFolderAndSelectItems, SHParseDisplayName, SetWindowSubclass,
                ShellExecuteExW, TaskbarList, ASSOC_FILTER_RECOMMENDED, SEE_MASK_INVOKEIDLIST, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, THBF_ENABLED, THBF_HIDDEN, THBN_CLICKED, THB_FLAGS,
                THB_ICON, THB_TOOLTIP, THUMBBUTTON,
            },
            WindowsAndMessaging::{CreateIconIndirect, DestroyIcon, GetIconInfo, HICON, ICONINFO, WM_COMMAND, WM_DESTROY},
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
    unsafe { ShellExecuteExW(&mut info).map_err(|e| e.message()) }
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
    unsafe { ShellExecuteExW(&mut info).map_err(|e| e.message()) }
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
    unsafe { ShellExecuteExW(&mut info).map_err(|e| e.message()) }
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

        let mut alpha = 0;
        for i in (0..(width * height * 4)).step_by(4) {
            alpha += pixel_data[(i + 3) as usize];
        }

        // If transparent
        if alpha == 0 {
            for y in 0..height {
                for x in 0..width {
                    let i = (y * width + x) * 4;
                    if pixel_data[(i + 3) as usize] == 0 {
                        // Set fully opaque if alpha was 0
                        pixel_data[(i + 3) as usize] = 255;
                    }
                }
            }
        }

        let _ = unsafe { DeleteDC(hdc) };
        let _ = unsafe { DeleteObject(icon_info.hbmColor) };
        let _ = unsafe { DeleteObject(icon_info.hbmMask) };
        let _ = unsafe { DestroyIcon(hicon) };

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
    unsafe { ShellExecuteExW(&mut info).map_err(|e| e.message()) }
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

struct InnerThumbButtons {
    callback: Box<dyn Fn(String)>,
    id_map: HashMap<u32, String>,
}

static BUTTONS_ADDED: OnceLock<bool> = OnceLock::new();

pub fn set_thumbar_buttons<F: Fn(String) + 'static>(window_handle: isize, buttons: &[ThumbButton], callback: F) -> Result<(), String> {
    let hwnd = HWND(window_handle as _);

    let _ = ComGuard::new();

    let mut thumb_buttons: Vec<THUMBBUTTON> = Vec::new();
    let mut id_map = HashMap::new();

    for i in 0..7 {
        // Set hidden buttons to the limit(7 buttons) so that new buttons can replace the existing buttons
        if i >= buttons.len() {
            thumb_buttons.push(THUMBBUTTON {
                iId: i as _,
                dwFlags: THBF_HIDDEN,
                dwMask: THB_FLAGS,
                ..Default::default()
            });
            continue;
        }

        let button = buttons.get(i).unwrap();
        id_map.insert(i as _, button.id.clone());

        let hicon = create_hicon(&button.icon)?;

        let mut thumb_button = THUMBBUTTON {
            iId: i as _,
            iBitmap: 0,
            hIcon: hicon,
            szTip: [0; 260],
            dwMask: THB_FLAGS | THB_ICON | THB_TOOLTIP,
            dwFlags: THBF_ENABLED,
        };

        // Set tooltip
        if let Some(tooltip) = &button.tool_tip {
            let tooltip_wide = encode_wide(tooltip);
            thumb_button.szTip[..tooltip_wide.len()].copy_from_slice(&tooltip_wide);
        }

        thumb_buttons.push(thumb_button);
    }

    let taskbar: ITaskbarList3 = unsafe { CoCreateInstance(&TaskbarList, None, CLSCTX_INPROC_SERVER).map_err(|e| e.message()) }?;

    unsafe { taskbar.HrInit().map_err(|e| e.message()) }?;

    if BUTTONS_ADDED.get().is_none() {
        unsafe { taskbar.ThumbBarAddButtons(hwnd, &thumb_buttons).map_err(|e| e.message()) }?;
        BUTTONS_ADDED.set(true).unwrap();
    } else {
        unsafe { taskbar.ThumbBarUpdateButtons(hwnd, &thumb_buttons).map_err(|e| e.message()) }?;
    }

    let inner = InnerThumbButtons {
        callback: Box::new(callback),
        id_map,
    };

    unsafe {
        let _ = SetWindowSubclass(hwnd, Some(subclass_proc), 200, Box::into_raw(Box::new(inner)) as _);
    }

    Ok(())
}

fn create_hicon(file_path: &PathBuf) -> Result<HICON, String> {
    let imaging_factory: IWICImagingFactory = unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).map_err(|e| e.message()) }?;

    let wide = encode_wide(file_path);
    let decoder = unsafe { imaging_factory.CreateDecoderFromFilename(PCWSTR::from_raw(wide.as_ptr()), None, GENERIC_READ, WICDecodeMetadataCacheOnDemand).map_err(|e| e.message()) }?;

    let frame = unsafe { decoder.GetFrame(0).unwrap() };

    let converter = unsafe { imaging_factory.CreateFormatConverter().unwrap() };
    unsafe { converter.Initialize(&frame, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom).map_err(|e| e.message()) }?;

    let mut width = 0;
    let mut height = 0;
    unsafe { converter.GetSize(&mut width, &mut height).map_err(|e| e.message()) }?;

    let stride = (width * 4) as usize;

    let buffer_size = stride * height as usize;
    let mut pixel_data = vec![0u8; buffer_size];

    // Copy WIC bitmap to HBITMAP
    unsafe { converter.CopyPixels(std::ptr::null(), width * 4, &mut pixel_data).map_err(|e| e.message()) }?;

    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: 0,
            biSizeImage: 0,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        },
        ..Default::default()
    };

    let hdc = unsafe { CreateCompatibleDC(None) };
    let mut bits_ptr: *mut u8 = std::ptr::null_mut();
    let hbitmap = unsafe { CreateDIBSection(hdc, &bmi, DIB_RGB_COLORS, &mut bits_ptr as *mut *mut u8 as *mut *mut _, None, 0).map_err(|e| e.message()) }?;

    if hbitmap.is_invalid() || pixel_data.is_empty() {
        let _ = unsafe { DeleteDC(hdc) };
        return Ok(HICON(0 as _));
    }

    // Copy pixel data into the HBITMAP memory
    unsafe { std::ptr::copy_nonoverlapping(pixel_data.as_ptr(), bits_ptr, buffer_size) };

    let _ = unsafe { DeleteDC(hdc) };

    let icon_info = ICONINFO {
        fIcon: true.into(),
        xHotspot: 0,
        yHotspot: 0,
        hbmMask: hbitmap,
        hbmColor: hbitmap,
    };

    let hicon = unsafe { CreateIconIndirect(&icon_info).map_err(|e| e.message()) }?;

    let _ = unsafe { DeleteObject(hbitmap) };

    Ok(hicon)
}

unsafe extern "system" fn subclass_proc(window: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, _uidsubclass: usize, dwrefdata: usize) -> LRESULT {
    match msg {
        WM_COMMAND => {
            let hiword = HIWORD(wparam.0 as _);

            if hiword == THBN_CLICKED as _ {
                let button_in = LOWORD(wparam.0 as _) as u32;
                let inner = unsafe { &mut *(dwrefdata as *mut InnerThumbButtons) };
                if let Some(id) = inner.id_map.get(&button_in) {
                    (inner.callback)(id.to_string());
                }

                return LRESULT(0);
            }

            DefSubclassProc(window, msg, wparam, lparam)
        }

        WM_DESTROY => {
            let _ = RemoveWindowSubclass(window, Some(subclass_proc), 200);
            DefSubclassProc(window, msg, wparam, lparam)
        }

        _ => DefSubclassProc(window, msg, wparam, lparam),
    }
}

#[allow(non_snake_case)]
fn LOWORD(dword: u32) -> u16 {
    (dword & 0xFFFF) as u16
}

#[allow(non_snake_case)]
fn HIWORD(dword: u32) -> u16 {
    ((dword & 0xFFFF_0000) >> 16) as u16
}

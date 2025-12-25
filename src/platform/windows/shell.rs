use super::util::{decode_wide, encode_wide, ComGuard};
use crate::{AppInfo, RgbaIcon, Size, ThumbButton};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::OnceLock,
};
use windows::{
    core::{Interface, HSTRING, PCWSTR, PWSTR},
    Management::Deployment::PackageManager,
    Win32::{
        Foundation::{GENERIC_READ, HWND, LPARAM, LRESULT, MAX_PATH, PROPERTYKEY, SIZE, WPARAM},
        Globalization::{GetLocaleInfoEx, LOCALE_SNAME},
        Graphics::{
            Gdi::{CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, HPALETTE},
            Imaging::{
                CLSID_WICImagingFactory, GUID_ContainerFormatPng, GUID_WICPixelFormat32bppPBGRA, IWICBitmapFrameEncode, IWICImagingFactory, WICBitmapDitherTypeNone, WICBitmapEncoderNoCache,
                WICBitmapPaletteTypeCustom, WICBitmapUsePremultipliedAlpha, WICDecodeMetadataCacheOnDemand,
            },
        },
        System::Com::{CoCreateInstance, CoTaskMemFree, StructuredStorage::IPropertyBag2, CLSCTX_INPROC_SERVER, STATFLAG_NONAME, STATSTG, STREAM_SEEK_SET},
        UI::{
            Shell::{
                DefSubclassProc, IShellItem, IShellItemImageFactory, ITaskbarList3,
                PropertiesSystem::{IPropertyStore, PSGetNameFromPropertyKey, SHGetPropertyStoreFromParsingName, GPS_DEFAULT},
                RemoveWindowSubclass, SHAssocEnumHandlers, SHCreateItemFromParsingName, SHLoadIndirectString, SHOpenFolderAndSelectItems, SHParseDisplayName, SetWindowSubclass, ShellExecuteExW,
                TaskbarList, ASSOC_FILTER_RECOMMENDED, SEE_MASK_INVOKEIDLIST, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, SIIGBF_ICONONLY, THBF_ENABLED, THBF_HIDDEN, THBN_CLICKED, THB_FLAGS,
                THB_ICON, THB_TOOLTIP, THUMBBUTTON,
            },
            WindowsAndMessaging::{CreateIconIndirect, HICON, ICONINFO, WM_COMMAND, WM_DESTROY},
        },
    },
};

static BUTTONS_ADDED: OnceLock<bool> = OnceLock::new();
const SW_SHOWNORMAL: i32 = 1;

/// Opens the file with the default/associated application
pub fn open_path<P: AsRef<Path>>(file_path: P) -> Result<(), String> {
    let _guard = ComGuard::new();

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

/// Opens the file with the specified application
pub fn open_path_with<P1: AsRef<Path>, P2: AsRef<Path>>(file_path: P1, app_path: P2) -> Result<(), String> {
    let _guard = ComGuard::new();

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

pub fn execute<P1: AsRef<Path>, P2: AsRef<Path>>(file_path: P1, app_path: P2) -> Result<(), String> {
    let _guard = ComGuard::new();

    let app_path = encode_wide(app_path.as_ref());
    let file_path = encode_wide(file_path.as_ref());
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        hwnd: HWND::default(),
        lpFile: PCWSTR::from_raw(app_path.as_ptr()),
        lpParameters: PCWSTR::from_raw(file_path.as_ptr()),
        fMask: SEE_MASK_NOCLOSEPROCESS,
        ..Default::default()
    };
    unsafe { ShellExecuteExW(&mut info).map_err(|e| e.message()) }
}

pub fn execute_as<P1: AsRef<Path>, P2: AsRef<Path>>(file_path: P1, app_path: P2) -> Result<(), String> {
    let _guard = ComGuard::new();

    let wide_verb = encode_wide("runas");
    let app_path = encode_wide(app_path.as_ref());
    let file_path = encode_wide(file_path.as_ref());
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        hwnd: HWND::default(),
        lpVerb: PCWSTR::from_raw(wide_verb.as_ptr()),
        lpFile: PCWSTR::from_raw(app_path.as_ptr()),
        lpParameters: PCWSTR::from_raw(file_path.as_ptr()),
        fMask: SEE_MASK_NOCLOSEPROCESS,
        ..Default::default()
    };
    unsafe { ShellExecuteExW(&mut info).map_err(|e| e.message()) }
}

/// Shows the application chooser dialog
pub fn show_open_with_dialog<P: AsRef<Path>>(file_path: P) -> Result<(), String> {
    let _guard = ComGuard::new();

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

/// Lists the applications that can open the file
pub fn get_open_with<P: AsRef<Path>>(file_path: P) -> Vec<AppInfo> {
    let mut apps = Vec::new();

    if let Some(extension_name) = file_path.as_ref().extension() {
        let _guard = ComGuard::new();
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

                    let mut raw_icon_path = PWSTR::null();
                    let mut index = 0;
                    let icon_location = unsafe { handler.GetIconLocation(&mut raw_icon_path, &mut index) };

                    let uwp = if icon_location.is_ok() {
                        is_uwp(raw_icon_path)
                    } else {
                        false
                    };
                    let icon_path = if uwp {
                        get_icon_path(raw_icon_path)
                    } else {
                        decode_wide(unsafe { raw_icon_path.as_wide() })
                    };

                    if uwp {
                        if let Some(model_id) = extract_app_user_model_id(raw_icon_path) {
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
                        icon_path,
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

/// Extracts an icon from executable/icon file or an icon stored in a file's associated executable file
pub fn extract_icon<P: AsRef<Path>>(path: P, size: Size) -> Result<RgbaIcon, String> {
    let _guard = ComGuard::new();

    let wide = encode_wide(path.as_ref());
    let item: IShellItem = unsafe { SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None) }.map_err(|e| e.message())?;
    let image_factory: IShellItemImageFactory = item.cast().map_err(|e| e.message())?;

    let (width, height) = (size.width, size.height);

    let size = SIZE {
        cx: width as _,
        cy: height as _,
    };

    let hbitmap = unsafe { image_factory.GetImage(size, SIIGBF_ICONONLY) }.map_err(|e| e.message())?;

    let factory: IWICImagingFactory = unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER) }.map_err(|e| e.message())?;
    let wic_bitmap = unsafe { factory.CreateBitmapFromHBITMAP(hbitmap, HPALETTE(std::ptr::null_mut()), WICBitmapUsePremultipliedAlpha) }.map_err(|e| e.message())?;
    let mut format = unsafe { wic_bitmap.GetPixelFormat() }.map_err(|e| e.message())?;
    let converter = unsafe { factory.CreateFormatConverter() }.map_err(|e| e.message())?;
    unsafe {
        converter.Initialize(&wic_bitmap, &format, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom).map_err(|e| e.message())?;
    }

    let stride = width * 4;
    let buffer_size = stride * height;
    let mut raw_pixels = vec![0u8; buffer_size as usize];

    unsafe { converter.CopyPixels(std::ptr::null(), stride, &mut raw_pixels) }.map_err(|e| e.message())?;

    let _ = unsafe { DeleteObject(hbitmap.into()) };

    let pixels = raw_pixels.clone();
    let bitmap = unsafe { factory.CreateBitmapFromMemory(width, height, &format, width * 4, &pixels) }.map_err(|e| e.message())?;

    let stream = unsafe { factory.CreateStream() }.map_err(|e| e.message())?;
    unsafe { stream.InitializeFromMemory(&pixels) }.map_err(|e| e.message())?;

    let encoder = unsafe { factory.CreateEncoder(&GUID_ContainerFormatPng, std::ptr::null()) }.map_err(|e| e.message())?;
    unsafe { encoder.Initialize(&stream, WICBitmapEncoderNoCache) }.map_err(|e| e.message())?;

    let mut frame: Option<IWICBitmapFrameEncode> = None;
    let mut bag: Option<IPropertyBag2> = None;
    unsafe { encoder.CreateNewFrame(&mut frame, &mut bag) }.map_err(|e| e.message())?;
    if let Some(frame) = frame {
        unsafe { frame.Initialize(None) }.map_err(|e| e.message())?;
        unsafe { frame.SetSize(width, height) }.map_err(|e| e.message())?;

        unsafe { frame.SetPixelFormat(&mut format) }.map_err(|e| e.message())?;

        unsafe { frame.WriteSource(&bitmap, std::ptr::null()) }.map_err(|e| e.message())?;
        unsafe { frame.Commit() }.map_err(|e| e.message())?;
        unsafe { encoder.Commit() }.map_err(|e| e.message())?;

        let mut stat = STATSTG::default();
        unsafe { stream.Stat(&mut stat, STATFLAG_NONAME) }.map_err(|e| e.message())?;

        let mut png = vec![0u8; stat.cbSize as usize];
        unsafe { stream.Seek(0, STREAM_SEEK_SET, None) }.map_err(|e| e.message())?;
        let _ = unsafe { stream.Read(png.as_mut_ptr() as _, stat.cbSize as _, None) };

        Ok(RgbaIcon {
            raw_pixels,
            png,
            width,
            height,
        })
    } else {
        Ok(RgbaIcon {
            raw_pixels,
            png: Vec::new(),
            width,
            height,
        })
    }
}

/// Shows the file/directory property dialog
pub fn open_file_property<P: AsRef<Path>>(file_path: P) -> Result<(), String> {
    let _guard = ComGuard::new();

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
    let _guard = ComGuard::new();

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

/// Adds a thumbnail toolbar with specified buttons to a taskbar layout of an application window
pub fn set_thumbar_buttons<F: Fn(String) + 'static>(window_handle: isize, buttons: &[ThumbButton], callback: F) -> Result<(), String> {
    let hwnd = HWND(window_handle as _);

    let _guard = ComGuard::new();

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
    let hbitmap = unsafe { CreateDIBSection(Some(hdc), &bmi, DIB_RGB_COLORS, &mut bits_ptr as *mut *mut u8 as *mut *mut _, None, 0).map_err(|e| e.message()) }?;

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

    let _ = unsafe { DeleteObject(hbitmap.into()) };

    Ok(hicon)
}

unsafe extern "system" fn subclass_proc(window: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, _uidsubclass: usize, dwrefdata: usize) -> LRESULT {
    match msg {
        WM_COMMAND => {
            let hiword = HIWORD(wparam.0 as _);

            if hiword == THBN_CLICKED as u16 {
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

pub(crate) fn read_properties<P: AsRef<Path>>(file_path: P) -> HashMap<String, String> {
    let _guard = ComGuard::new();

    let mut result = HashMap::new();
    let wide = encode_wide(file_path.as_ref());
    let store: IPropertyStore = unsafe { SHGetPropertyStoreFromParsingName(PCWSTR::from_raw(wide.as_ptr()), None, GPS_DEFAULT).unwrap() };

    let count = unsafe { store.GetCount().unwrap() };
    for i in 0..count {
        let mut propkey = PROPERTYKEY::default();

        if unsafe { store.GetAt(i, &mut propkey).is_ok() } {
            if let Ok(propvalue) = unsafe { store.GetValue(&propkey) } {
                if let Ok(keyname) = unsafe { PSGetNameFromPropertyKey(&propkey) } {
                    let key = unsafe { keyname.to_string().unwrap().replace("System", "").replace('.', "") };
                    let value = propvalue.to_string();
                    result.insert(key, value.to_string());
                };
            }
        }
    }

    result
}

pub fn get_locale() -> String {
    let size = unsafe { GetLocaleInfoEx(PCWSTR::null(), LOCALE_SNAME, None) };
    let mut locale = vec![0u16; size as _];
    let _ = unsafe { GetLocaleInfoEx(PCWSTR::null(), LOCALE_SNAME, Some(&mut locale)) };
    decode_wide(locale.as_slice())
}

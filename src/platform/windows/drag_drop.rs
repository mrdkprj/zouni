use super::util::{encode_wide, global_free};
use crate::Operation;
use std::mem::ManuallyDrop;
use windows::{
    core::{implement, HRESULT, PCWSTR},
    Win32::{
        Foundation::*,
        System::{
            Com::{CoInitializeEx, CoUninitialize, IDataObject, COINIT_APARTMENTTHREADED, FORMATETC, STGMEDIUM, STGMEDIUM_0, TYMED_HGLOBAL},
            Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE},
            Ole::{DoDragDrop, IDropSource, IDropSource_Impl, CF_HDROP, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_MOVE, DROPEFFECT_NONE},
            SystemServices::{MK_LBUTTON, MODIFIERKEYS_FLAGS},
        },
        UI::Shell::{Common::ITEMIDLIST, SHCreateDataObject, SHParseDisplayName, DROPFILES},
    },
};

pub fn start_drag(file_paths: Vec<String>, operation: Operation) -> Result<(), String> {
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };

    let pidls: Vec<*const ITEMIDLIST> = file_paths
        .iter()
        .map(|path| {
            let mut pidl = std::ptr::null_mut();
            let wide_str = encode_wide(path);
            unsafe { SHParseDisplayName(PCWSTR::from_raw(wide_str.as_ptr()), None, &mut pidl, 0, None) }?;
            Ok(pidl as *const _)
        })
        .collect::<windows::core::Result<_>>()
        .unwrap();

    let data_object: IDataObject = unsafe { SHCreateDataObject(None, Some(&pidls), None).map_err(|e| e.message()) }?;

    let mut file_list = file_paths.join("\0");
    // Append null to the last file
    file_list.push('\0');
    // Append null to the last
    file_list.push('\0');

    let mut total_size = std::mem::size_of::<u32>();
    for path in file_paths {
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
        return Err("Failed to free memory".to_string());
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

    // Set the data in the IDataObject
    let format_etc = FORMATETC {
        cfFormat: CF_HDROP.0,
        ptd: std::ptr::null_mut(),
        dwAspect: windows::Win32::System::Com::DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as _,
    };

    let stg_medium = STGMEDIUM {
        tymed: TYMED_HGLOBAL.0 as _,
        u: STGMEDIUM_0 {
            hGlobal: hglobal,
        },
        pUnkForRelease: ManuallyDrop::new(None),
    };

    unsafe { data_object.SetData(&format_etc, &stg_medium, true).map_err(|e| e.message()) }?;

    let drop_source: IDropSource = DragDropTarget.into();
    let mut effects = match operation {
        Operation::Copy => DROPEFFECT_COPY,
        Operation::Move => DROPEFFECT_MOVE,
        Operation::None => DROPEFFECT_NONE,
    };

    let _ = unsafe { DoDragDrop(&data_object, &drop_source, effects, &mut effects) };

    unsafe { CoUninitialize() };

    Ok(())
}

#[implement(IDropSource)]
pub struct DragDropTarget;

#[allow(non_snake_case)]
impl IDropSource_Impl for DragDropTarget_Impl {
    fn QueryContinueDrag(&self, escape_pressed: BOOL, keystate: MODIFIERKEYS_FLAGS) -> HRESULT {
        if escape_pressed.as_bool() {
            return DRAGDROP_S_CANCEL;
        }

        if !keystate.contains(MK_LBUTTON) {
            return DRAGDROP_S_DROP;
        }

        S_OK
    }

    fn GiveFeedback(&self, _dweffect: DROPEFFECT) -> windows_core::HRESULT {
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}

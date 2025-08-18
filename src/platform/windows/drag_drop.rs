use super::util::{encode_wide, ComGuard, GlobalMemory};
use crate::Operation;
use std::mem::ManuallyDrop;
use windows::{
    core::{implement, Ref, BOOL, HRESULT, PCWSTR},
    Win32::{
        Foundation::*,
        System::{
            Com::{CoTaskMemFree, IDataObject, DVASPECT_CONTENT, FORMATETC, STGMEDIUM, STGMEDIUM_0, TYMED_HGLOBAL},
            Ole::{
                DoDragDrop, IDropSource, IDropSource_Impl, IDropTarget, IDropTarget_Impl, RegisterDragDrop, ReleaseStgMedium, RevokeDragDrop, CF_HDROP, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_MOVE,
                DROPEFFECT_NONE,
            },
            SystemServices::{MK_LBUTTON, MODIFIERKEYS_FLAGS},
        },
        UI::Shell::{Common::ITEMIDLIST, SHCreateDataObject, SHParseDisplayName, DROPFILES},
    },
};

pub fn start_drag(file_paths: Vec<String>, operation: Operation) -> Result<(), String> {
    let _guard = ComGuard::new();

    let pidls: Vec<*const ITEMIDLIST> = file_paths
        .iter()
        .map(|path| {
            let mut pidl = std::ptr::null_mut();
            let wide_str = encode_wide(path);
            unsafe { SHParseDisplayName(PCWSTR::from_raw(wide_str.as_ptr()), None, &mut pidl, 0, None) }?;
            Ok(pidl as *const _)
        })
        .collect::<windows::core::Result<_>>()
        .map_err(|e| e.message())?;

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

    // Set the data in the IDataObject
    let format_etc = FORMATETC {
        cfFormat: CF_HDROP.0,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as _,
    };

    let mut stg_medium = STGMEDIUM {
        tymed: TYMED_HGLOBAL.0 as _,
        u: STGMEDIUM_0 {
            hGlobal: hglobal.handle(),
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

    for pidl in &pidls {
        unsafe { CoTaskMemFree(Some(*pidl as *mut _)) };
    }

    unsafe { ReleaseStgMedium(&mut stg_medium) };

    Ok(())
}

#[implement(IDropSource)]
struct DragDropTarget;

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

    fn GiveFeedback(&self, _dweffect: DROPEFFECT) -> HRESULT {
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}

pub fn register(window_handle: isize) -> Result<(), String> {
    let _ = unregister(window_handle);
    let drag_drop_target: IDropTarget = DropTarget.into();
    unsafe { RegisterDragDrop(HWND(window_handle as _), &drag_drop_target).map_err(|e| e.message()) }
}

pub fn unregister(window_handle: isize) -> Result<(), String> {
    unsafe { RevokeDragDrop(HWND(window_handle as _)).map_err(|e| e.message()) }
}

#[implement(IDropTarget)]
struct DropTarget;

#[allow(non_snake_case)]
impl IDropTarget_Impl for DropTarget_Impl {
    fn DragEnter(&self, _pDataObj: Ref<IDataObject>, _grfKeyState: MODIFIERKEYS_FLAGS, _pt: &POINTL, _pdwEffect: *mut DROPEFFECT) -> windows::core::Result<()> {
        Ok(())
    }

    fn DragOver(&self, _grfKeyState: MODIFIERKEYS_FLAGS, _pt: &POINTL, _pdwEffect: *mut DROPEFFECT) -> windows::core::Result<()> {
        Ok(())
    }

    fn DragLeave(&self) -> windows::core::Result<()> {
        Ok(())
    }

    fn Drop(&self, _pDataObj: Ref<IDataObject>, _grfKeyState: MODIFIERKEYS_FLAGS, _pt: &POINTL, _pdwEffect: *mut DROPEFFECT) -> windows::core::Result<()> {
        Ok(())
    }
}

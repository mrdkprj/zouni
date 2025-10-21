use super::{
    shell,
    util::{decode_wide, encode_wide, prefixed, ComGuard},
};
use crate::{Dirent, FileAttribute, RecycleBinItem, UndeleteRequest, Volume};
use std::{collections::HashMap, path::Path};
use windows::{
    core::{Interface, PCSTR, PCWSTR},
    Win32::{
        Foundation::{CloseHandle, FILETIME, HANDLE, HWND, MAX_PATH, PROPERTYKEY, S_OK},
        Storage::FileSystem::{
            CreateFileW, FindClose, FindExInfoBasic, FindExSearchNameMatch, FindFirstFileExW, FindFirstVolumeW, FindNextFileW, FindNextVolumeW, FindVolumeClose, GetDiskFreeSpaceExW, GetDriveTypeW,
            GetVolumeInformationW, GetVolumePathNamesForVolumeNameW, SetFileTime, FILE_ATTRIBUTE_DEVICE, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_READONLY,
            FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_SYSTEM, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_WRITE_ATTRIBUTES,
            FIND_FIRST_EX_FLAGS, OPEN_EXISTING, WIN32_FIND_DATAW,
        },
        System::{
            Com::{CoCreateInstance, CoTaskMemFree, CreateBindCtx, IPersistFile, CLSCTX_ALL, CLSCTX_INPROC_SERVER, STGM_READ},
            Variant::{VariantChangeType, VariantClear, VariantGetStringElem, VariantToFileTime, PSTIME_FLAGS, VARIANT, VAR_CHANGE_FLAGS, VT_BSTR, VT_DATE},
        },
        UI::Shell::{
            Common::{ITEMIDLIST, STRRET},
            FMTID_Storage, FOLDERID_RecycleBinFolder, FileOperation, IContextMenu, IEnumIDList, IFileOperation, IShellFolder, IShellFolder2, IShellItem, IShellItemArray, IShellLinkW,
            SHCreateItemFromParsingName, SHCreateShellItemArrayFromIDLists, SHEmptyRecycleBinW, SHGetDataFromIDListW, SHGetDesktopFolder, SHGetKnownFolderIDList, SHParseDisplayName, ShellLink,
            CMINVOKECOMMANDINFO, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_RENAMEONCOLLISION, KF_FLAG_DEFAULT, PID_DISPLACED_DATE, PSGUID_DISPLACED, SHCONTF_FOLDERS, SHCONTF_NONFOLDERS,
            SHGDFIL_FINDDATA, SHGDN_NORMAL, SLGP_UNCPRIORITY,
        },
    },
};

/// Lists volumes
pub fn list_volumes() -> Result<Vec<Volume>, String> {
    let mut volumes: Vec<Volume> = Vec::new();

    let mut volume_path_guid = vec![0u16; MAX_PATH as usize];
    let handle = unsafe { FindFirstVolumeW(&mut volume_path_guid).map_err(|e| e.message()) }?;

    loop {
        let mut drive_paths = vec![0u16; (MAX_PATH + 1) as usize];
        let mut len = 0;
        unsafe { GetVolumePathNamesForVolumeNameW(PCWSTR::from_raw(volume_path_guid.as_ptr()), Some(&mut drive_paths), &mut len).map_err(|e| e.message()) }?;

        let mount_point = decode_wide(&drive_paths);

        let mut volume_label_ptr = vec![0u16; (MAX_PATH + 1) as usize];
        unsafe { GetVolumeInformationW(PCWSTR(volume_path_guid.as_ptr()), Some(&mut volume_label_ptr), None, None, None, None).map_err(|e| e.message()) }?;

        let mut volume_label = decode_wide(&volume_label_ptr);

        if volume_label.is_empty() {
            volume_label = match unsafe { GetDriveTypeW(PCWSTR::from_raw(drive_paths.as_ptr())) } {
                2 => "Removable Drive".to_string(),
                3 => "Disk Drive".to_string(),
                4 => "Network Drive".to_string(),
                _ => "Unknown".to_string(),
            }
        }

        if mount_point.is_empty() {
            volumes.push(Volume {
                mount_point,
                volume_label,
                available_units: 0,
                total_units: 0,
            });
        } else {
            let mut available = 0;
            let mut total = 0;
            unsafe { GetDiskFreeSpaceExW(PCWSTR::from_raw(drive_paths.as_ptr()), None, Some(&mut total), Some(&mut available)).map_err(|e| e.message()) }?;
            volumes.push(Volume {
                mount_point,
                volume_label,
                available_units: available,
                total_units: total,
            });
        }

        volume_path_guid = vec![0u16; MAX_PATH as usize];
        let next = unsafe { FindNextVolumeW(handle, &mut volume_path_guid) };
        if next.is_err() {
            break;
        }
    }

    unsafe { FindVolumeClose(handle).map_err(|e| e.message()) }?;

    Ok(volumes)
}

/// Lists all files/directories under the specified directory
pub fn readdir<P: AsRef<Path>>(directory: P, recursive: bool, with_mime_type: bool) -> Result<Vec<Dirent>, String> {
    let mut entries = Vec::new();

    if !directory.as_ref().is_dir() {
        return Ok(entries);
    }

    let mut search_path = directory.as_ref().to_path_buf();
    search_path.push("*");

    let wide = encode_wide(prefixed(search_path));
    let path = PCWSTR::from_raw(wide.as_ptr());
    let mut data: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };
    let handle = unsafe { FindFirstFileExW(path, FindExInfoBasic, &mut data as *mut _ as _, FindExSearchNameMatch, None, FIND_FIRST_EX_FLAGS(0)).map_err(|e| e.message()) }?;

    if handle.is_invalid() {
        return Ok(entries);
    }

    try_readdir(handle, directory, &mut entries, recursive, with_mime_type)?;

    Ok(entries)
}

fn try_readdir<P: AsRef<Path>>(handle: HANDLE, parent: P, entries: &mut Vec<Dirent>, recursive: bool, with_mime_type: bool) -> Result<&mut Vec<Dirent>, String> {
    let mut data: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };

    while unsafe { FindNextFileW(handle, &mut data) }.is_ok() {
        let name = decode_wide(&data.cFileName);
        if name == "." || name == ".." {
            continue;
        }

        let mut full_path = parent.as_ref().to_path_buf();

        if full_path.to_str().unwrap().ends_with(":") {
            full_path.push(std::path::MAIN_SEPARATOR_STR);
        }
        full_path.push(name.clone());

        let attributes = get_attribute(&full_path, &data)?;

        let mime_type = if with_mime_type {
            get_mime_type(if attributes.is_symbolic_link {
                &attributes.link_path
            } else {
                &name
            })
        } else {
            String::new()
        };

        entries.push(Dirent {
            name: name.clone(),
            parent_path: parent.as_ref().to_string_lossy().to_string(),
            full_path: full_path.to_string_lossy().to_string(),
            attributes,
            mime_type,
        });

        if data.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0 && recursive {
            let mut search_path = parent.as_ref().to_path_buf();
            search_path.push(name);
            let next_parent = search_path.clone();
            search_path.push("*");
            let wide = encode_wide(prefixed(search_path));
            let path = PCWSTR::from_raw(wide.as_ptr());
            let next_handle = unsafe { FindFirstFileExW(path, FindExInfoBasic, &mut data as *mut _ as _, FindExSearchNameMatch, None, FIND_FIRST_EX_FLAGS(0)).map_err(|e| e.message()) }?;
            if !next_handle.is_invalid() {
                try_readdir(next_handle, next_parent, entries, recursive, with_mime_type)?;
            }
        }
    }

    unsafe { FindClose(handle).map_err(|e| e.message()) }?;

    Ok(entries)
}

/// Gets file/directory attributes
pub fn stat<P: AsRef<Path>>(file_path: P) -> Result<FileAttribute, String> {
    let wide = encode_wide(prefixed(file_path.as_ref()));
    let path = PCWSTR::from_raw(wide.as_ptr());

    let mut data: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };
    let handle = unsafe { FindFirstFileExW(path, FindExInfoBasic, &mut data as *mut _ as _, FindExSearchNameMatch, None, FIND_FIRST_EX_FLAGS(0)).map_err(|e| e.message()) }?;
    let file_attributes = get_attribute(&file_path, &data)?;
    unsafe { FindClose(handle).map_err(|e| e.message()) }?;

    Ok(file_attributes)
}

fn get_attribute<P: AsRef<Path>>(file_path: &P, data: &WIN32_FIND_DATAW) -> Result<FileAttribute, String> {
    let attributes = data.dwFileAttributes;
    let possible_file_type = get_file_type(&file_path, attributes);
    let (file_type, is_symbolic_link, link_path) = if possible_file_type == FileType::Link {
        get_link_path(file_path.as_ref())?
    } else {
        (possible_file_type, false, String::new())
    };

    Ok(FileAttribute {
        is_directory: file_type == FileType::Dir,
        is_read_only: attributes & FILE_ATTRIBUTE_READONLY.0 != 0,
        is_hidden: attributes & FILE_ATTRIBUTE_HIDDEN.0 != 0,
        is_system: attributes & FILE_ATTRIBUTE_SYSTEM.0 != 0,
        is_device: file_type == FileType::Device,
        is_file: file_type == FileType::File,
        is_symbolic_link,
        ctime_ms: 0,
        mtime_ms: to_msecs_from_file_time(data.ftLastWriteTime.dwLowDateTime, data.ftLastWriteTime.dwHighDateTime),
        atime_ms: to_msecs_from_file_time(data.ftLastAccessTime.dwLowDateTime, data.ftLastAccessTime.dwHighDateTime),
        birthtime_ms: to_msecs_from_file_time(data.ftCreationTime.dwLowDateTime, data.ftCreationTime.dwHighDateTime),
        size: (data.nFileSizeLow as u64) | ((data.nFileSizeHigh as u64) << 32),
        link_path,
    })
}

#[derive(PartialEq, Debug)]
enum FileType {
    Device,
    Link,
    Dir,
    File,
}

fn get_file_type<P: AsRef<Path>>(file_path: &P, attr: u32) -> FileType {
    if attr & FILE_ATTRIBUTE_DEVICE.0 != 0 {
        return FileType::Device;
    }

    if attr & FILE_ATTRIBUTE_DIRECTORY.0 != 0 {
        return FileType::Dir;
    }

    // Shortcut/file/archive are all FILE_ATTRIBUTE_ARCHIVE
    // So determine type by extension
    if attr & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 || file_path.as_ref().extension().unwrap_or_default() == "lnk" {
        return FileType::Link;
    }

    FileType::File
}

fn get_link_path<P: AsRef<Path>>(full_path: P) -> Result<(FileType, bool, String), String> {
    let _guard = ComGuard::new();

    let shell_link: IShellLinkW = unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).map_err(|e| e.message()) }?;
    let persist_file: IPersistFile = shell_link.cast().map_err(|e| e.message())?;
    let wide = encode_wide(prefixed(full_path.as_ref()));
    let path = PCWSTR::from_raw(wide.as_ptr());
    if unsafe { persist_file.Load(path, STGM_READ).is_err() } {
        return Ok((FileType::File, false, String::new()));
    }

    let mut data: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };
    let mut link_path = vec![0u16; (MAX_PATH + 1) as usize];
    unsafe { shell_link.GetPath(&mut link_path, &mut data, SLGP_UNCPRIORITY.0 as _).map_err(|e| e.message()) }?;
    let mut working_directory = vec![0u16; (MAX_PATH + 1) as usize];
    unsafe { shell_link.GetWorkingDirectory(&mut working_directory).map_err(|e| e.message()) }?;
    let link_path_str = decode_wide(&link_path);
    let working_directory_str = decode_wide(&working_directory);
    if working_directory_str.is_empty() {
        Ok((FileType::Dir, true, link_path_str))
    } else {
        Ok((FileType::File, true, link_path_str))
    }
}

/// Create shortcut
pub fn create_symlink<P1: AsRef<Path>, P2: AsRef<Path>>(full_path: P1, link_path: P2) -> Result<(), String> {
    let _guard = ComGuard::new();

    let shell_link: IShellLinkW = unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).map_err(|e| e.message()) }?;
    if link_path.as_ref().is_file() {
        if let Some(directory) = link_path.as_ref().parent() {
            let wide = encode_wide(prefixed(directory));
            let working_directory = PCWSTR::from_raw(wide.as_ptr());
            unsafe { shell_link.SetWorkingDirectory(working_directory) }.map_err(|e| e.message())?;
        }
    }

    let wide = encode_wide(prefixed(link_path.as_ref()));
    let link_path = PCWSTR::from_raw(wide.as_ptr());
    unsafe { shell_link.SetPath(link_path) }.map_err(|e| e.message())?;

    let persist_file: IPersistFile = shell_link.cast().map_err(|e| e.message())?;
    let mut symlink = full_path.as_ref().to_string_lossy().to_string();
    symlink.push_str(".lnk");
    let wide = encode_wide(prefixed(symlink));
    let path = PCWSTR::from_raw(wide.as_ptr());
    unsafe { persist_file.Save(path, true) }.map_err(|e| e.message())?;

    Ok(())
}

/// Gets mime type of the file
pub fn get_mime_type<P: AsRef<Path>>(file_path: P) -> String {
    match mime_guess::from_path(file_path).first() {
        Some(s) => s.essence_str().to_string(),
        None => String::new(),
    }
}

#[allow(dead_code)]
fn get_mime_type_fallback<P: AsRef<Path>>(file_path: P) -> String {
    let props = shell::read_properties(file_path);
    if props.contains_key("MIMEType") {
        props.get("MIMEType").unwrap().to_string()
    } else {
        String::new()
    }
}

/// Moves an item
pub fn mv<P1: AsRef<Path>, P2: AsRef<Path>>(from: P1, to: P2) -> Result<(), String> {
    let _guard = ComGuard::new();

    let from_wide = encode_wide(from.as_ref());
    let to_wide = encode_wide(to.as_ref());
    let from_item: IShellItem = unsafe { SHCreateItemFromParsingName(PCWSTR::from_raw(from_wide.as_ptr()), None).map_err(|e| e.message()) }?;
    let to_item: IShellItem = unsafe { SHCreateItemFromParsingName(PCWSTR::from_raw(to_wide.as_ptr()), None).map_err(|e| e.message()) }?;

    let op: IFileOperation = unsafe { CoCreateInstance(&FileOperation, None, CLSCTX_ALL).map_err(|e| e.message()) }?;
    unsafe { op.SetOperationFlags(FOF_ALLOWUNDO).map_err(|e| e.message()) }?;
    unsafe { op.MoveItem(&from_item, &to_item, None, None).map_err(|e| e.message()) }?;
    execute(op)
}

/// Moves multiple items
pub fn mv_all<P1: AsRef<Path>, P2: AsRef<Path>>(from: &[P1], to: P2) -> Result<(), String> {
    let _guard = ComGuard::new();

    let from_item_array = get_id_lists(from)?;
    let to_wide = encode_wide(to.as_ref());
    let to_item: IShellItem = unsafe { SHCreateItemFromParsingName(PCWSTR::from_raw(to_wide.as_ptr()), None).map_err(|e| e.message()) }?;

    let op: IFileOperation = unsafe { CoCreateInstance(&FileOperation, None, CLSCTX_ALL).map_err(|e| e.message()) }?;
    unsafe { op.SetOperationFlags(FOF_ALLOWUNDO).map_err(|e| e.message()) }?;
    unsafe { op.MoveItems(&from_item_array, &to_item).map_err(|e| e.message()) }?;
    execute(op)
}

/// Copies an item
pub fn copy<P1: AsRef<Path>, P2: AsRef<Path>>(from: P1, to: P2) -> Result<(), String> {
    let _guard = ComGuard::new();

    let from_wide = encode_wide(from.as_ref());
    let to_wide = encode_wide(to.as_ref());
    let from_item: IShellItem = unsafe { SHCreateItemFromParsingName(PCWSTR::from_raw(from_wide.as_ptr()), None).map_err(|e| e.message()) }?;
    let to_item: IShellItem = unsafe { SHCreateItemFromParsingName(PCWSTR::from_raw(to_wide.as_ptr()), None).map_err(|e| e.message()) }?;

    let op: IFileOperation = unsafe { CoCreateInstance(&FileOperation, None, CLSCTX_ALL).map_err(|e| e.message()) }?;
    if from.as_ref().parent().unwrap() == to.as_ref() {
        unsafe { op.SetOperationFlags(FOF_ALLOWUNDO | FOF_RENAMEONCOLLISION).map_err(|e| e.message()) }?;
    } else {
        unsafe { op.SetOperationFlags(FOF_ALLOWUNDO).map_err(|e| e.message()) }?;
    }
    unsafe { op.CopyItem(&from_item, &to_item, None, None).map_err(|e| e.message()) }?;
    execute(op)
}

/// Copies multiple items
pub fn copy_all<P1: AsRef<Path>, P2: AsRef<Path>>(from: &[P1], to: P2) -> Result<(), String> {
    let _guard = ComGuard::new();

    let from_item_array = get_id_lists(from)?;
    let to_wide = encode_wide(to.as_ref());
    let to_item: IShellItem = unsafe { SHCreateItemFromParsingName(PCWSTR::from_raw(to_wide.as_ptr()), None).map_err(|e| e.message()) }?;

    let op: IFileOperation = unsafe { CoCreateInstance(&FileOperation, None, CLSCTX_ALL).map_err(|e| e.message()) }?;
    let from_sample = from.first().unwrap();
    if from_sample.as_ref().parent().unwrap() == to.as_ref() {
        unsafe { op.SetOperationFlags(FOF_ALLOWUNDO | FOF_RENAMEONCOLLISION).map_err(|e| e.message()) }?;
    } else {
        unsafe { op.SetOperationFlags(FOF_ALLOWUNDO).map_err(|e| e.message()) }?;
    }
    unsafe { op.CopyItems(&from_item_array, &to_item).map_err(|e| e.message()) }?;
    execute(op)
}

/// Deletes an item
pub fn delete<P: AsRef<Path>>(file_path: P) -> Result<(), String> {
    let _guard = ComGuard::new();

    let file_wide = encode_wide(file_path.as_ref());
    let shell_item: IShellItem = unsafe { SHCreateItemFromParsingName(PCWSTR::from_raw(file_wide.as_ptr()), None).map_err(|e| e.message()) }?;

    let op: IFileOperation = unsafe { CoCreateInstance(&FileOperation, None, CLSCTX_ALL).map_err(|e| e.message()) }?;
    unsafe { op.SetOperationFlags(FOF_NOCONFIRMATION).map_err(|e| e.message()) }?;
    unsafe { op.DeleteItem(&shell_item, None).map_err(|e| e.message()) }?;
    execute(op)
}

/// Deletes multiple items
pub fn delete_all<P: AsRef<Path>>(file_paths: &[P]) -> Result<(), String> {
    let _guard = ComGuard::new();

    let item_array = get_id_lists(file_paths)?;

    let op: IFileOperation = unsafe { CoCreateInstance(&FileOperation, None, CLSCTX_ALL).map_err(|e| e.message()) }?;
    unsafe { op.SetOperationFlags(FOF_NOCONFIRMATION).map_err(|e| e.message()) }?;
    unsafe { op.DeleteItems(&item_array).map_err(|e| e.message()) }?;
    execute(op)
}

/// Moves an item to the OS-specific trash location
pub fn trash<P: AsRef<Path>>(file_path: P) -> Result<(), String> {
    let _guard = ComGuard::new();

    let file_wide = encode_wide(file_path.as_ref());
    let shell_item: IShellItem = unsafe { SHCreateItemFromParsingName(PCWSTR::from_raw(file_wide.as_ptr()), None).map_err(|e| e.message()) }?;

    let op: IFileOperation = unsafe { CoCreateInstance(&FileOperation, None, CLSCTX_ALL).map_err(|e| e.message()) }?;
    unsafe { op.SetOperationFlags(FOF_ALLOWUNDO).map_err(|e| e.message()) }?;
    unsafe { op.DeleteItem(&shell_item, None).map_err(|e| e.message()) }?;
    execute(op)
}

/// Moves multiple items to the OS-specific trash location
pub fn trash_all<P: AsRef<Path>>(file_paths: &[P]) -> Result<(), String> {
    let _guard = ComGuard::new();

    let item_array = get_id_lists(file_paths)?;
    let op: IFileOperation = unsafe { CoCreateInstance(&FileOperation, None, CLSCTX_ALL).map_err(|e| e.message()) }?;
    unsafe { op.SetOperationFlags(FOF_ALLOWUNDO).map_err(|e| e.message()) }?;
    unsafe { op.DeleteItems(&item_array).map_err(|e| e.message()) }?;
    execute(op)
}

fn get_id_lists<P: AsRef<Path>>(from: &[P]) -> Result<IShellItemArray, String> {
    let items: Vec<*const ITEMIDLIST> = from
        .iter()
        .map(|path| {
            let mut item = std::ptr::null_mut();
            let wide_str = encode_wide(path.as_ref());
            unsafe { SHParseDisplayName(PCWSTR::from_raw(wide_str.as_ptr()), None, &mut item, 0, None) }?;
            Ok(item as *const _)
        })
        .collect::<windows::core::Result<_>>()
        .map_err(|e| e.message())?;

    let array = unsafe { SHCreateShellItemArrayFromIDLists(&items).map_err(|e| e.message()) };

    for item in items {
        unsafe { CoTaskMemFree(Some(item as _)) };
    }
    array
}

fn execute(op: IFileOperation) -> Result<(), String> {
    let result = unsafe { op.PerformOperations() };

    if result.is_err() {
        if unsafe { op.GetAnyOperationsAborted().map_err(|e| e.message()) }?.as_bool() {
            return Ok(());
        } else {
            return result.map_err(|e| e.message());
        }
    }

    Ok(())
}

const PKEY_SIZE: PROPERTYKEY = PROPERTYKEY {
    fmtid: FMTID_Storage,
    pid: 12,
};
const PKEY_DELETED_DATE: PROPERTYKEY = PROPERTYKEY {
    fmtid: PSGUID_DISPLACED,
    pid: PID_DISPLACED_DATE,
};

fn get_recycle_bin() -> Result<IShellFolder2, String> {
    let recycle_bin_item: *mut ITEMIDLIST = unsafe { SHGetKnownFolderIDList(&FOLDERID_RecycleBinFolder, KF_FLAG_DEFAULT.0 as _, None).map_err(|e| e.message()) }?;
    let desktop: IShellFolder = unsafe { SHGetDesktopFolder().map_err(|e| e.message()) }?;
    let pbc = unsafe { CreateBindCtx(0).map_err(|e| e.message()) }?;
    let recycle_bin: IShellFolder2 = unsafe { desktop.BindToObject(recycle_bin_item, &pbc).map_err(|e| e.message()) }?;
    unsafe { CoTaskMemFree(Some(recycle_bin_item as _)) };
    Ok(recycle_bin)
}

/// Gets items in recycle bin
pub fn read_recycle_bin() -> Result<Vec<RecycleBinItem>, String> {
    let _guard = ComGuard::new();

    let recycle_bin = get_recycle_bin()?;
    let mut enum_list: Option<IEnumIDList> = None;
    let _ = unsafe { recycle_bin.EnumObjects(HWND::default(), (SHCONTF_FOLDERS.0 | SHCONTF_NONFOLDERS.0) as _, &mut enum_list) };

    if enum_list.is_none() {
        return Ok(Vec::new());
    }

    let list = enum_list.unwrap();
    let mut rgelt: Vec<*mut ITEMIDLIST> = vec![std::ptr::null_mut()];
    let cnt: Option<*mut u32> = None;

    let mut result = Vec::new();

    while unsafe { list.Next(&mut rgelt, cnt) } == S_OK {
        if rgelt.is_empty() {
            continue;
        }

        let item = *(rgelt.first().unwrap());

        let original_path = to_original_path(&recycle_bin, item)?;
        let name = Path::new(&original_path).file_name().unwrap_or_default().to_string_lossy().to_string();
        let deleted_date_ms = to_time_ms_from_variant(&recycle_bin, item, &PKEY_DELETED_DATE)?;

        let mut src = unsafe { recycle_bin.GetDetailsEx(item, &PKEY_SIZE).map_err(|e| e.message()) }?;
        let mut variant = VARIANT::default();
        unsafe { VariantChangeType(&mut variant, &src, VAR_CHANGE_FLAGS(0), VT_BSTR).map_err(|e| e.message()) }.unwrap();
        let size_ptr = unsafe { VariantGetStringElem(&variant, 0).map_err(|e| e.message()) }?;
        let size: u64 = if let Ok(size) = unsafe { size_ptr.to_string() } {
            size.parse().unwrap()
        } else {
            0
        };
        unsafe { VariantClear(&mut variant).map_err(|e| e.message()) }?;
        unsafe { VariantClear(&mut src).map_err(|e| e.message()) }?;

        let mime_type = get_mime_type(&original_path);

        let mut data: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };
        unsafe { SHGetDataFromIDListW(&recycle_bin, item, SHGDFIL_FINDDATA, &mut data as *mut _ as _, size_of::<WIN32_FIND_DATAW>() as _).unwrap() };
        let mut attributes = get_attribute(&original_path, &data)?;
        attributes.size = size;

        let bin_item = RecycleBinItem {
            name,
            original_path,
            deleted_date_ms,
            attributes,
            mime_type,
        };
        result.push(bin_item);

        unsafe { CoTaskMemFree(Some(item as _)) };

        rgelt = vec![std::ptr::null_mut()];
    }

    Ok(result)
}

struct ItemData {
    deleted_date_ms: u64,
    item: *mut ITEMIDLIST,
}
/// Undos a trash operation
pub fn undelete<P: AsRef<Path>>(file_paths: &[P]) -> Result<(), String> {
    let _guard = ComGuard::new();

    let file_paths: Vec<String> = file_paths.iter().map(|f| f.as_ref().to_string_lossy().to_string()).collect();
    let recycle_bin = get_recycle_bin()?;
    let mut enum_list: Option<IEnumIDList> = None;
    let _ = unsafe { recycle_bin.EnumObjects(HWND::default(), (SHCONTF_FOLDERS.0 | SHCONTF_NONFOLDERS.0) as _, &mut enum_list) };

    if enum_list.is_none() {
        return Ok(());
    }

    let list = enum_list.unwrap();
    let mut rgelt: Vec<*mut ITEMIDLIST> = vec![std::ptr::null_mut()];
    let cnt: Option<*mut u32> = None;

    let mut map: HashMap<String, ItemData> = HashMap::new();

    while unsafe { list.Next(&mut rgelt, cnt) } == S_OK {
        if rgelt.is_empty() {
            continue;
        }

        let item = *(rgelt.first().unwrap());

        let old_path = to_original_path(&recycle_bin, item)?;
        let deleted_date_ms = to_time_ms_from_variant(&recycle_bin, item, &PKEY_DELETED_DATE)?;

        if file_paths.contains(&old_path) {
            let data = ItemData {
                deleted_date_ms,
                item,
            };

            if map.contains_key(&old_path) {
                let old = map.get(&old_path).unwrap();
                if old.deleted_date_ms < deleted_date_ms {
                    let old = map.insert(old_path, data).unwrap();
                    unsafe { CoTaskMemFree(Some(old.item as _)) };
                }
            } else {
                map.insert(old_path, data);
            }
        } else {
            unsafe { CoTaskMemFree(Some(item as _)) };
        }

        rgelt = vec![std::ptr::null_mut()];
    }

    let items: Vec<*const ITEMIDLIST> = map.values().map(|a| a.item as _).collect();

    if !items.is_empty() {
        let menu: IContextMenu = unsafe { recycle_bin.GetUIObjectOf(HWND::default(), &items, None).map_err(|e| e.message()) }?;
        let invoke = CMINVOKECOMMANDINFO {
            cbSize: std::mem::size_of::<CMINVOKECOMMANDINFO>() as u32,
            lpVerb: PCSTR(b"undelete\0".as_ptr()),
            ..Default::default()
        };

        match unsafe { menu.InvokeCommand(&invoke) } {
            Ok(_) => {
                for item in items {
                    unsafe { CoTaskMemFree(Some(item as _)) };
                }
            }
            Err(_) => {
                for item in items {
                    unsafe { CoTaskMemFree(Some(item as _)) };
                }
            }
        }
    }

    Ok(())
}

/// Undos a trash operation by deleted time
pub fn undelete_by_time(targets: &[UndeleteRequest]) -> Result<(), String> {
    let _guard = ComGuard::new();

    let args: HashMap<String, u64> = targets.iter().map(|target| (target.file_path.clone(), target.deleted_time_ms)).collect();
    let recycle_bin = get_recycle_bin()?;
    let mut enum_list: Option<IEnumIDList> = None;
    let _ = unsafe { recycle_bin.EnumObjects(HWND::default(), (SHCONTF_FOLDERS.0 | SHCONTF_NONFOLDERS.0) as _, &mut enum_list) };

    if enum_list.is_none() {
        return Ok(());
    }

    let list = enum_list.unwrap();
    let mut rgelt: Vec<*mut ITEMIDLIST> = vec![std::ptr::null_mut()];
    let cnt: Option<*mut u32> = None;

    let mut items: Vec<*const ITEMIDLIST> = Vec::new();

    while unsafe { list.Next(&mut rgelt, cnt) } == S_OK {
        if rgelt.is_empty() {
            continue;
        }

        let item = *(rgelt.first().unwrap());

        let old_path = to_original_path(&recycle_bin, item)?;
        let deleted_date_ms = to_time_ms_from_variant(&recycle_bin, item, &PKEY_DELETED_DATE)?;

        if args.contains_key(&old_path) && *args.get(&old_path).unwrap() == deleted_date_ms {
            items.push(item);
        } else {
            unsafe { CoTaskMemFree(Some(item as _)) };
        }

        rgelt = vec![std::ptr::null_mut()];
    }

    if !items.is_empty() {
        let menu: IContextMenu = unsafe { recycle_bin.GetUIObjectOf(HWND::default(), &items, None).map_err(|e| e.message()) }?;
        let invoke = CMINVOKECOMMANDINFO {
            cbSize: std::mem::size_of::<CMINVOKECOMMANDINFO>() as u32,
            lpVerb: PCSTR(b"undelete\0".as_ptr()),
            ..Default::default()
        };

        match unsafe { menu.InvokeCommand(&invoke) } {
            Ok(_) => {
                for item in items {
                    unsafe { CoTaskMemFree(Some(item as _)) };
                }
            }
            Err(_) => {
                for item in items {
                    unsafe { CoTaskMemFree(Some(item as _)) };
                }
            }
        }
    }

    Ok(())
}

fn to_original_path(recycle_bin: &IShellFolder2, item: *const ITEMIDLIST) -> Result<String, String> {
    let mut street: STRRET = STRRET::default();
    unsafe { recycle_bin.GetDisplayNameOf(item, SHGDN_NORMAL, &mut street).map_err(|e| e.message()) }?;
    let original_path = decode_wide(unsafe { street.Anonymous.pOleStr.as_wide() });
    Ok(original_path)
}

fn to_time_ms_from_variant(recycle_bin: &IShellFolder2, item: *const ITEMIDLIST, key: &PROPERTYKEY) -> Result<u64, String> {
    let mut src = unsafe { recycle_bin.GetDetailsEx(item, key).map_err(|e| e.message()) }?;
    let mut variant = VARIANT::default();
    unsafe { VariantChangeType(&mut variant, &src, VAR_CHANGE_FLAGS(0), VT_DATE).map_err(|e| e.message()) }?;
    let file_time = unsafe { VariantToFileTime(&variant, PSTIME_FLAGS(0)) }.unwrap();
    let time_ms = to_msecs_from_file_time(file_time.dwLowDateTime, file_time.dwHighDateTime);
    unsafe { VariantClear(&mut variant).map_err(|e| e.message()) }?;
    unsafe { VariantClear(&mut src).map_err(|e| e.message()) }?;
    Ok(time_ms)
}

/// Empty Recycle Bin
pub fn empty_recycle_bin(root: Option<String>) -> Result<(), String> {
    let drive = if let Some(root) = root {
        PCWSTR::from_raw(encode_wide(root).as_ptr())
    } else {
        PCWSTR::null()
    };
    unsafe { SHEmptyRecycleBinW(None, drive, 0).map_err(|e| e.to_string()) }?;

    Ok(())
}

/// Changes the modification and access timestamps of a file
pub fn utimes<P: AsRef<Path>>(file: P, atime_ms: u64, mtime_ms: u64) -> Result<(), String> {
    let wide = encode_wide(file.as_ref());
    let handle = unsafe {
        CreateFileW(
            PCWSTR::from_raw(wide.as_ptr()),
            FILE_WRITE_ATTRIBUTES.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            None,
        )
        .map_err(|e| e.message())?
    };

    if handle.is_invalid() {
        return Err(format!("Failed to write file:{}", file.as_ref().to_string_lossy()));
    }

    unsafe { SetFileTime(handle, None, Some(&to_file_time(atime_ms)), Some(&to_file_time(mtime_ms))).map_err(|e| e.message()) }?;

    unsafe { CloseHandle(handle).map_err(|e| e.message()) }?;

    Ok(())
}

fn to_file_time(time: u64) -> FILETIME {
    // milliseconds to 100-nanosecond
    const EPOCH_DIFFERENCE: u64 = 11644473600000;
    let intervals = (time + EPOCH_DIFFERENCE) * 10_000;
    FILETIME {
        dwLowDateTime: intervals as u32,
        dwHighDateTime: (intervals >> 32) as u32,
    }
}

fn to_msecs_from_file_time(low: u32, high: u32) -> u64 {
    // FILETIME epoch (1601-01-01) to Unix epoch (1970-01-01) in milliseconds
    let windows_epoch = 11644473600000;
    let ticks = ((high as u64) << 32) | low as u64;
    // FILETIME is in 100-nanosecond intervals
    let milliseconds = ticks / 10_000;

    milliseconds - windows_epoch
}

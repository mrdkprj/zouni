use super::util::{decode_wide, encode_wide, prefixed, ComGuard};
use crate::{Dirent, FileAttribute, Volume};
use std::path::Path;
use windows::{
    core::{PCWSTR, PWSTR},
    Win32::{
        Foundation::{HANDLE, MAX_PATH},
        Storage::FileSystem::{
            FindClose, FindExInfoBasic, FindExSearchNameMatch, FindFirstFileExW, FindFirstVolumeW, FindNextFileW, FindNextVolumeW, FindVolumeClose, GetDiskFreeSpaceExW, GetVolumeInformationW,
            GetVolumePathNamesForVolumeNameW, FILE_ATTRIBUTE_DEVICE, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_READONLY, FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_SYSTEM,
            FIND_FIRST_EX_FLAGS, WIN32_FIND_DATAW,
        },
        System::Com::{CoCreateInstance, CLSCTX_ALL, CLSCTX_INPROC_SERVER},
        UI::Shell::{
            CLSID_QueryAssociations, Common::ITEMIDLIST, FileOperation, IFileOperation, IQueryAssociations, IShellItem, IShellItemArray, SHCreateItemFromParsingName,
            SHCreateShellItemArrayFromIDLists, SHParseDisplayName, ASSOCF_NONE, ASSOCSTR_CONTENTTYPE,
        },
    },
};

pub fn list_volumes() -> Result<Vec<Volume>, String> {
    let mut volumes: Vec<Volume> = Vec::new();

    let mut volume_name = vec![0u16; MAX_PATH as usize];
    let handle = unsafe { FindFirstVolumeW(&mut volume_name).map_err(|e| e.message()) }?;

    loop {
        let mut drive_paths = vec![0u16; (MAX_PATH + 1) as usize];
        let mut len = 0;
        unsafe { GetVolumePathNamesForVolumeNameW(PCWSTR::from_raw(volume_name.as_ptr()), Some(&mut drive_paths), &mut len).map_err(|e| e.message()) }?;

        let mount_point = decode_wide(&drive_paths);

        let mut volume_label_ptr = vec![0u16; (MAX_PATH + 1) as usize];
        unsafe { GetVolumeInformationW(PCWSTR(volume_name.as_ptr()), Some(&mut volume_label_ptr), None, None, None, None).map_err(|e| e.message()) }?;

        let volume_label = decode_wide(&volume_label_ptr);

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

        volume_name = vec![0u16; MAX_PATH as usize];
        let next = unsafe { FindNextVolumeW(handle, &mut volume_name) };
        if next.is_err() {
            break;
        }
    }

    unsafe { FindVolumeClose(handle).map_err(|e| e.message()) }?;

    Ok(volumes)
}

pub fn stat<P: AsRef<Path>>(file_path: P) -> Result<FileAttribute, String> {
    let wide = encode_wide(prefixed(file_path.as_ref()));
    let path = PCWSTR::from_raw(wide.as_ptr());

    let mut data: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };
    let handle = unsafe { FindFirstFileExW(path, FindExInfoBasic, &mut data as *mut _ as _, FindExSearchNameMatch, None, FIND_FIRST_EX_FLAGS(0)).map_err(|e| e.message()) }?;
    let file_attributes = get_attribute(&data);
    unsafe { FindClose(handle).map_err(|e| e.message()) }?;

    Ok(file_attributes)
}

#[derive(PartialEq)]
enum FileType {
    Device,
    Link,
    Dir,
    File,
}

fn get_file_type(attr: u32) -> FileType {
    if attr & FILE_ATTRIBUTE_DEVICE.0 != 0 {
        return FileType::Device;
    }
    if attr & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 {
        return FileType::Link;
    }
    if attr & FILE_ATTRIBUTE_DIRECTORY.0 != 0 {
        return FileType::Dir;
    }

    FileType::File
}

fn get_attribute(data: &WIN32_FIND_DATAW) -> FileAttribute {
    let attributes = data.dwFileAttributes;
    let file_type = get_file_type(attributes);

    FileAttribute {
        is_directory: file_type == FileType::Dir,
        is_read_only: attributes & FILE_ATTRIBUTE_READONLY.0 != 0,
        is_hidden: attributes & FILE_ATTRIBUTE_HIDDEN.0 != 0,
        is_system: attributes & FILE_ATTRIBUTE_SYSTEM.0 != 0,
        is_device: file_type == FileType::Device,
        is_file: file_type == FileType::File,
        is_symbolic_link: file_type == FileType::Link,
        ctime_ms: 0.0,
        mtime_ms: to_msecs_from_file_time(data.ftLastWriteTime.dwLowDateTime, data.ftLastWriteTime.dwHighDateTime),
        atime_ms: to_msecs_from_file_time(data.ftLastAccessTime.dwLowDateTime, data.ftLastAccessTime.dwHighDateTime),
        birthtime_ms: to_msecs_from_file_time(data.ftCreationTime.dwLowDateTime, data.ftCreationTime.dwHighDateTime),
        size: (data.nFileSizeLow as u64) | ((data.nFileSizeHigh as u64) << 32),
    }
}

fn to_msecs_from_file_time(low: u32, high: u32) -> f64 {
    // FILETIME epoch (1601-01-01) to Unix epoch (1970-01-01) in milliseconds
    let windows_epoch = 11644473600000.0;
    let ticks = ((high as u64) << 32) | low as u64;
    // FILETIME is in 100-nanosecond intervals
    let milliseconds = ticks as f64 / 10_000.0;

    milliseconds - windows_epoch
}

pub fn get_mime_type<P: AsRef<Path>>(file_path: P) -> String {
    match mime_guess::from_path(file_path).first() {
        Some(s) => s.essence_str().to_string(),
        None => String::new(),
    }
}

#[allow(dead_code)]
fn get_mime_type_fallback<P: AsRef<Path>>(file_path: P) -> Result<String, String> {
    let _ = ComGuard::new();

    let query_associations: IQueryAssociations = unsafe { CoCreateInstance(&CLSID_QueryAssociations, None, CLSCTX_INPROC_SERVER).map_err(|e| e.message()) }?;

    let wide = encode_wide(file_path.as_ref());
    unsafe { query_associations.Init(ASSOCF_NONE, PCWSTR::from_raw(wide.as_ptr()), None, None).map_err(|e| e.message()) }?;
    let mut mime_type_ptr = [0u16; 256];
    let mime_type = PWSTR::from_raw(mime_type_ptr.as_mut_ptr());
    let mut mime_len = unsafe { mime_type.len() } as u32;
    unsafe { query_associations.GetString(ASSOCF_NONE, ASSOCSTR_CONTENTTYPE, None, mime_type, &mut mime_len).map_err(|e| e.message()) }?;
    let content_type = decode_wide(unsafe { mime_type.as_wide() });
    Ok(content_type)
}

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

        let mime_type = if with_mime_type {
            get_mime_type(&name)
        } else {
            String::new()
        };

        entries.push(Dirent {
            name: name.clone(),
            parent_path: parent.as_ref().to_string_lossy().to_string(),
            full_path: full_path.to_string_lossy().to_string(),
            attributes: get_attribute(&data),
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

pub fn mv<P1: AsRef<Path>, P2: AsRef<Path>>(from: P1, to: P2) -> Result<(), String> {
    let _ = ComGuard::new();

    let from_wide = encode_wide(prefixed(from.as_ref()));
    let to_wide = encode_wide(prefixed(to.as_ref()));
    let from_item: IShellItem = unsafe { SHCreateItemFromParsingName(PCWSTR::from_raw(from_wide.as_ptr()), None).map_err(|e| e.message()) }?;
    let to_item: IShellItem = unsafe { SHCreateItemFromParsingName(PCWSTR::from_raw(to_wide.as_ptr()), None).map_err(|e| e.message()) }?;

    let op: IFileOperation = unsafe { CoCreateInstance(&FileOperation, None, CLSCTX_ALL).map_err(|e| e.message()) }?;
    unsafe { op.MoveItem(&from_item, &to_item, None, None).map_err(|e| e.message()) }?;
    execute(op)
}

pub fn mv_all<P1: AsRef<Path>, P2: AsRef<Path>>(from: &[P1], to: P2) -> Result<(), String> {
    let _ = ComGuard::new();

    let from_item_array = get_id_lists(from)?;
    let to_wide = encode_wide(prefixed(to.as_ref()));
    let to_item: IShellItem = unsafe { SHCreateItemFromParsingName(PCWSTR::from_raw(to_wide.as_ptr()), None).map_err(|e| e.message()) }?;

    let op: IFileOperation = unsafe { CoCreateInstance(&FileOperation, None, CLSCTX_ALL).map_err(|e| e.message()) }?;
    unsafe { op.MoveItems(&from_item_array, &to_item).map_err(|e| e.message()) }?;
    execute(op)
}

pub fn copy<P1: AsRef<Path>, P2: AsRef<Path>>(from: P1, to: P2) -> Result<(), String> {
    let _ = ComGuard::new();

    let from_wide = encode_wide(prefixed(from.as_ref()));
    let to_wide = encode_wide(prefixed(to.as_ref()));
    let from_item: IShellItem = unsafe { SHCreateItemFromParsingName(PCWSTR::from_raw(from_wide.as_ptr()), None).map_err(|e| e.message()) }?;
    let to_item: IShellItem = unsafe { SHCreateItemFromParsingName(PCWSTR::from_raw(to_wide.as_ptr()), None).map_err(|e| e.message()) }?;

    let op: IFileOperation = unsafe { CoCreateInstance(&FileOperation, None, CLSCTX_ALL).map_err(|e| e.message()) }?;
    unsafe { op.CopyItem(&from_item, &to_item, None, None).map_err(|e| e.message()) }?;
    execute(op)
}

pub fn copy_all<P1: AsRef<Path>, P2: AsRef<Path>>(from: &[P1], to: P2) -> Result<(), String> {
    let _ = ComGuard::new();

    let from_item_array = get_id_lists(from)?;
    let to_wide = encode_wide(prefixed(to.as_ref()));
    let to_item: IShellItem = unsafe { SHCreateItemFromParsingName(PCWSTR::from_raw(to_wide.as_ptr()), None).map_err(|e| e.message()) }?;

    let op: IFileOperation = unsafe { CoCreateInstance(&FileOperation, None, CLSCTX_ALL).map_err(|e| e.message()) }?;
    unsafe { op.CopyItems(&from_item_array, &to_item).map_err(|e| e.message()) }?;
    execute(op)
}

pub fn delete<P: AsRef<Path>>(file_path: P) -> Result<(), String> {
    let _ = ComGuard::new();

    let file_wide = encode_wide(prefixed(file_path.as_ref()));
    let shell_item: IShellItem = unsafe { SHCreateItemFromParsingName(PCWSTR::from_raw(file_wide.as_ptr()), None).map_err(|e| e.message()) }?;

    let op: IFileOperation = unsafe { CoCreateInstance(&FileOperation, None, CLSCTX_ALL).map_err(|e| e.message()) }?;
    unsafe { op.DeleteItem(&shell_item, None).map_err(|e| e.message()) }?;
    execute(op)
}

pub fn delete_all<P: AsRef<Path>>(file_paths: &[P]) -> Result<(), String> {
    let _ = ComGuard::new();

    let item_array = get_id_lists(file_paths)?;

    let op: IFileOperation = unsafe { CoCreateInstance(&FileOperation, None, CLSCTX_ALL).map_err(|e| e.message()) }?;
    unsafe { op.DeleteItems(&item_array).map_err(|e| e.message()) }?;
    execute(op)
}

fn get_id_lists<P: AsRef<Path>>(from: &[P]) -> Result<IShellItemArray, String> {
    let pidls: Vec<*const ITEMIDLIST> = from
        .iter()
        .map(|path| {
            let mut pidl = std::ptr::null_mut();
            let wide_str = encode_wide(path.as_ref());
            unsafe { SHParseDisplayName(PCWSTR::from_raw(wide_str.as_ptr()), None, &mut pidl, 0, None) }?;
            Ok(pidl as *const _)
        })
        .collect::<windows::core::Result<_>>()
        .map_err(|e| e.message())?;

    unsafe { SHCreateShellItemArrayFromIDLists(&pidls).map_err(|e| e.message()) }
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

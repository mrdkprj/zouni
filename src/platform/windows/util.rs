use std::os::windows::ffi::OsStrExt;
use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::{GlobalFree, HGLOBAL, MAX_PATH},
        Globalization::lstrlenW,
        System::{
            Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED},
            Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE},
        },
    },
};
use windows_core::HRESULT;

pub(crate) fn decode_wide(wide: &[u16]) -> String {
    let len = unsafe { lstrlenW(PCWSTR::from_raw(wide.as_ptr())) } as usize;
    let w_str_slice = unsafe { std::slice::from_raw_parts(wide.as_ptr(), len) };
    String::from_utf16_lossy(w_str_slice)
}

pub(crate) fn encode_wide(string: impl AsRef<std::ffi::OsStr>) -> Vec<u16> {
    string.as_ref().encode_wide().chain(std::iter::once(0)).collect()
}

pub(crate) fn prefixed(path: impl AsRef<std::ffi::OsStr>) -> String {
    if path.as_ref().len() >= MAX_PATH as usize {
        if let Some(stripped) = path.as_ref().to_str().unwrap().strip_prefix("\\\\") {
            format!("\\\\?\\UNC\\{}", stripped)
        } else {
            format!("\\\\?\\{}", path.as_ref().to_str().unwrap())
        }
    } else {
        path.as_ref().to_string_lossy().to_string()
    }
}

pub(crate) struct ComGuard;

impl ComGuard {
    pub fn new() -> Self {
        let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        Self
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

pub(crate) struct GlobalMemory {
    handle: HGLOBAL,
}

impl GlobalMemory {
    pub fn new(size: usize) -> Result<Self, String> {
        match unsafe { GlobalAlloc(GMEM_MOVEABLE, size) } {
            Ok(handle) => Ok(Self {
                handle,
            }),
            Err(_) => Err("Failed to allocate global memory".to_string()),
        }
    }

    pub fn lock(&self) -> Result<*mut u8, String> {
        let ptr = unsafe { GlobalLock(self.handle) } as *mut u8;
        if ptr.is_null() {
            Err("Failed to lock global memory".to_string())
        } else {
            Ok(ptr)
        }
    }

    pub fn unlock(&self) {
        let _ = unsafe { GlobalUnlock(self.handle) };
    }

    pub fn handle(&self) -> HGLOBAL {
        self.handle
    }
}

impl Drop for GlobalMemory {
    fn drop(&mut self) {
        if !self.handle.is_invalid() {
            match unsafe { GlobalFree(self.handle) } {
                Ok(_) => {}
                Err(e) => {
                    if e.code() != HRESULT(0x00000000) {
                        eprintln!("Error freeing global memory: {:?}", e);
                    }
                }
            }
        }
    }
}

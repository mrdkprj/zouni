use super::util::decode_wide;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use windows::{
    core::{Error, GUID, PCWSTR},
    Win32::{
        Devices::DeviceAndDriverInstallation::{
            CM_Register_Notification, CM_Unregister_Notification, SetupDiEnumDeviceInfo, SetupDiGetClassDevsW, SetupDiGetDeviceRegistryPropertyW, SetupDiOpenDeviceInterfaceW, CM_NOTIFY_ACTION,
            CM_NOTIFY_ACTION_DEVICEINTERFACEARRIVAL, CM_NOTIFY_ACTION_DEVICEINTERFACEREMOVAL, CM_NOTIFY_EVENT_DATA, CM_NOTIFY_FILTER, CM_NOTIFY_FILTER_FLAG_ALL_INTERFACE_CLASSES,
            CM_NOTIFY_FILTER_TYPE_DEVICEINTERFACE, CR_SUCCESS, DIGCF_DEVICEINTERFACE, DIODI_NO_ADD, HCMNOTIFICATION, SPDRP_CLASS, SP_DEVICE_INTERFACE_DATA, SP_DEVINFO_DATA,
        },
        Foundation::{ERROR_SUCCESS, MAX_PATH},
    },
};

static CONFIG: Lazy<Mutex<isize>> = Lazy::new(|| Mutex::new(0));

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceEvent {
    name: String,
    event: String,
}

pub fn listen<F: FnMut(DeviceEvent) + 'static>(callback: F) -> bool {
    let notify_type = CM_NOTIFY_FILTER {
        cbSize: size_of::<CM_NOTIFY_FILTER>() as _,
        FilterType: CM_NOTIFY_FILTER_TYPE_DEVICEINTERFACE,
        Flags: CM_NOTIFY_FILTER_FLAG_ALL_INTERFACE_CLASSES,
        ..Default::default()
    };
    let mut config = HCMNOTIFICATION::default();
    let result = unsafe { CM_Register_Notification(&notify_type, Some(Box::into_raw(Box::new(callback)) as _), Some(on_notify::<F>), &mut config) };
    if result.0 == CR_SUCCESS.0 {
        unlisten();
        *CONFIG.lock().unwrap() = config.0 as _;
        true
    } else {
        false
    }
}

unsafe extern "system" fn on_notify<F: FnMut(DeviceEvent)>(
    _hnotify: HCMNOTIFICATION,
    context: *const core::ffi::c_void,
    action: CM_NOTIFY_ACTION,
    eventdata: *const CM_NOTIFY_EVENT_DATA,
    _eventdatasize: u32,
) -> u32 {
    match action {
        CM_NOTIFY_ACTION_DEVICEINTERFACEARRIVAL | CM_NOTIFY_ACTION_DEVICEINTERFACEREMOVAL => {
            let data = &*eventdata;
            if data.FilterType != CM_NOTIFY_FILTER_TYPE_DEVICEINTERFACE {
                return 0;
            }
            let callback = &mut *(context as *mut F);
            let name = get_device_type(data.u.DeviceInterface.ClassGuid, data.u.DeviceInterface.SymbolicLink.as_ptr()).unwrap_or_default();
            callback(DeviceEvent {
                name,
                event: if action == CM_NOTIFY_ACTION_DEVICEINTERFACEARRIVAL {
                    "Added".to_string()
                } else {
                    "Removed".to_string()
                },
            })
        }
        _ => {}
    };
    ERROR_SUCCESS.0
}

fn get_device_type(guid: GUID, symbolic_link: *const u16) -> Result<String, Error> {
    if let Ok(info) = unsafe { SetupDiGetClassDevsW(Some(&guid), None, None, DIGCF_DEVICEINTERFACE) } {
        let mut device_interface_data = SP_DEVICE_INTERFACE_DATA {
            cbSize: size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
            ..Default::default()
        };

        unsafe { SetupDiOpenDeviceInterfaceW(info, Some(&PCWSTR::from_raw(symbolic_link)), DIODI_NO_ADD, Some(&mut device_interface_data)) }?;

        let mut data = SP_DEVINFO_DATA {
            cbSize: size_of::<SP_DEVINFO_DATA>() as _,
            ..Default::default()
        };
        unsafe { SetupDiEnumDeviceInfo(info, 0, &mut data) }?;

        let mut buffer = vec![0u8; MAX_PATH as _];
        let mut property_size = 0u32;
        unsafe { SetupDiGetDeviceRegistryPropertyW(info, &data, SPDRP_CLASS, None, Some(&mut buffer), Some(&mut property_size)) }?;

        let wchar_count = property_size as usize / 2;
        let wide_buffer = &buffer[..property_size as usize];
        let wide_buffer = unsafe { std::slice::from_raw_parts(wide_buffer.as_ptr() as *const u16, wchar_count) };

        let device_type = decode_wide(wide_buffer);

        return Ok(device_type);
    }

    Ok(String::new())
}

pub fn unlisten() {
    if let Ok(config) = CONFIG.try_lock() {
        if *config != 0 {
            let _ = unsafe { CM_Unregister_Notification(HCMNOTIFICATION(*config as _)) };
        }
    }
}

pub fn is_listening() -> bool {
    if let Ok(config) = CONFIG.try_lock() {
        *config != 0
    } else {
        false
    }
}

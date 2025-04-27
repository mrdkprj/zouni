use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use windows::Win32::{
    Devices::DeviceAndDriverInstallation::{
        CM_Register_Notification, CM_Unregister_Notification, CM_NOTIFY_ACTION, CM_NOTIFY_ACTION_DEVICEINTERFACEARRIVAL, CM_NOTIFY_ACTION_DEVICEINTERFACEREMOVAL, CM_NOTIFY_EVENT_DATA,
        CM_NOTIFY_FILTER, CM_NOTIFY_FILTER_FLAG_ALL_INTERFACE_CLASSES, CM_NOTIFY_FILTER_TYPE_DEVICEINTERFACE, CR_SUCCESS, HCMNOTIFICATION,
    },
    Foundation::ERROR_SUCCESS,
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
        CM_NOTIFY_ACTION_DEVICEINTERFACEARRIVAL => {
            let callback = &mut *(context as *mut F);
            callback(DeviceEvent {
                name: { *eventdata }.u.DeviceInterface.ClassGuid.to_u128().to_string(),
                event: "Added".to_string(),
            })
        }
        CM_NOTIFY_ACTION_DEVICEINTERFACEREMOVAL => {
            let callback = &mut *(context as *mut F);
            callback(DeviceEvent {
                name: { *eventdata }.u.DeviceInterface.ClassGuid.to_u128().to_string(),
                event: "Removed".to_string(),
            })
        }
        _ => {}
    };
    ERROR_SUCCESS.0
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

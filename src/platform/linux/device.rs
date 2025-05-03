use once_cell::sync::Lazy;
use rusb::{Context, Device, Interfaces, Registration, UsbContext};
use serde::{Deserialize, Serialize};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

static LISTENER: Lazy<Mutex<Listener>> = Lazy::new(|| Mutex::new(Listener::default()));
static WATCHING: Lazy<Arc<AtomicBool>> = Lazy::new(|| Arc::new(AtomicBool::new(false)));

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceEvent {
    name: String,
    event: String,
}

struct HotPlugHandler {
    callback: Box<dyn FnMut(DeviceEvent) + 'static + Send>,
}

fn get_class_name(interfaces: Interfaces) -> String {
    let mut class_name = String::new();

    for interface in interfaces {
        for descriptor in interface.descriptors() {
            class_name = match descriptor.class_code() {
                1 => "Audio",
                2 => "COMM",
                3 => "HID",
                5 => "Physical",
                6 => "PTP",
                7 => "Printer",
                8 => "MassStorage",
                9 => "Hub",
                10 => "Data",
                _ => "Unknown",
            }
            .to_string();
        }
    }
    class_name
}

impl<T: UsbContext> rusb::Hotplug<T> for HotPlugHandler {
    fn device_arrived(&mut self, device: Device<T>) {
        (self.callback)(DeviceEvent {
            name: get_class_name(device.active_config_descriptor().unwrap().interfaces()),
            event: "Added".to_string(),
        });
    }

    fn device_left(&mut self, device: Device<T>) {
        (self.callback)(DeviceEvent {
            name: get_class_name(device.config_descriptor(0).unwrap().interfaces()),
            event: "Removed".to_string(),
        });
    }
}

#[derive(Default)]
struct Listener {
    context: Option<Context>,
    registration: Option<Registration<Context>>,
}

pub fn listen<F: FnMut(DeviceEvent) + 'static + Send>(callback: F) -> bool {
    if !rusb::has_hotplug() {
        return false;
    }

    if let Ok(context) = Context::new() {
        let callback = Box::new(callback);
        if let Ok(registration) = rusb::HotplugBuilder::new().register(
            &context,
            Box::new(HotPlugHandler {
                callback,
            }),
        ) {
            unlisten();

            *LISTENER.lock().unwrap() = Listener {
                context: Some(context),
                registration: Some(registration),
            };

            WATCHING.store(true, Ordering::SeqCst);

            std::thread::spawn(|| loop {
                if !WATCHING.load(Ordering::SeqCst) {
                    drop_context();
                    break;
                }

                if let Ok(listener) = LISTENER.try_lock() {
                    if let Some(context) = &listener.context {
                        context.handle_events(Some(Duration::from_millis(10))).unwrap();
                    }
                }
            });

            return true;
        }
    }

    false
}

fn drop_context() {
    if let Ok(mut con) = LISTENER.lock() {
        let context = con.context.take().unwrap();
        let registration = con.registration.take().unwrap();
        context.unregister_callback(registration);
    }
}

pub fn unlisten() {
    WATCHING.store(false, Ordering::SeqCst);
}

pub fn is_listening() -> bool {
    WATCHING.load(Ordering::SeqCst)
}

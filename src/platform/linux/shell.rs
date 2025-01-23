use crate::AppInfo;
use gio::{
    glib::{Cast, GString, ToVariant},
    prelude::{AppInfoExt, FileExt},
    AppInfoCreateFlags, AppLaunchContext, Cancellable, DBusCallFlags, DBusConnectionFlags, File, FileIcon, ThemedIcon,
};
use gtk::{prelude::WidgetExt, DialogFlags};
use std::path::Path;

use super::{fs::get_mime_type, util::init};

pub fn open_file_property<P: AsRef<Path>>(_file_path: P) -> Result<(), String> {
    Ok(())
}

pub fn open_path<P: AsRef<Path>>(file_path: P) -> Result<(), String> {
    let uri = format!("file://{}", file_path.as_ref().to_str().unwrap());
    gio::AppInfo::launch_default_for_uri(&uri, AppLaunchContext::NONE).map_err(|e| e.message().to_string())
}

pub fn open_path_with<P1: AsRef<Path>, P2: AsRef<Path>>(_file_path: P1, _app_path: P2) -> Result<(), String> {
    let info = gio::AppInfo::create_from_commandline(_app_path.as_ref(), None, AppInfoCreateFlags::NONE).map_err(|e| e.message().to_string())?;
    info.launch(&[File::for_path(_file_path)], AppLaunchContext::NONE).map_err(|e| e.message().to_string())
}

pub fn show_open_with_dialog<P: AsRef<Path>>(file_path: P) -> Result<(), String> {
    init();
    let file = File::for_path(file_path.as_ref().to_str().unwrap());
    let dialog = gtk::AppChooserDialog::new(gtk::Window::NONE, DialogFlags::DESTROY_WITH_PARENT, &file);
    dialog.show_all();
    Ok(())
}

pub fn get_open_with<P: AsRef<Path>>(file_path: P) -> Vec<AppInfo> {
    let mut apps = Vec::new();
    let content_type = get_mime_type(file_path);

    for app_info in gio::AppInfo::all_for_type(&content_type) {
        let name = app_info.display_name().to_string();
        let path = app_info.commandline().unwrap_or_default().to_string_lossy().to_string();
        let icon = if let Some(icon) = app_info.icon() {
            if let Some(themed_icon) = icon.downcast_ref::<ThemedIcon>() {
                themed_icon.names().first().unwrap_or(&GString::new()).to_string()
            } else if let Some(file_icon) = icon.downcast_ref::<FileIcon>() {
                file_icon.file().path().unwrap_or_default().to_string_lossy().to_string()
            } else {
                String::new()
            }
        } else {
            String::new()
        };
        apps.push(AppInfo {
            path,
            name,
            icon,
        });
    }
    println!("{:?}", apps);
    apps
}

pub fn show_item_in_folder<P: AsRef<Path>>(file_path: P) -> Result<(), String> {
    let bus = gio::bus_get_sync(gio::BusType::Session, Cancellable::NONE).unwrap();
    let conn = gio::DBusConnection::new_sync(&bus.stream(), None, DBusConnectionFlags::NONE, None, Cancellable::NONE).unwrap();
    let t = ("ss".to_string(), file_path.as_ref().to_string_lossy().to_string()).to_variant();
    let parameters = t;
    conn.call_sync(
        Some("org.freedesktop.FileManager1"),
        "/org/freedesktop/FileManager1",
        "org.freedesktop.FileManager1",
        "ShowItems",
        Some(&parameters),
        None,
        DBusCallFlags::NONE,
        -1,
        Cancellable::NONE,
    )
    .unwrap();

    Ok(())
}

pub fn trash<P: AsRef<Path>>(file: P) -> Result<(), String> {
    let file = File::for_parse_name(file.as_ref().to_str().unwrap());
    file.trash(Cancellable::NONE).map_err(|e| e.message().to_string())
}

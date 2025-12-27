use super::{
    fs::{self, get_mime_type},
    util::init,
};
use crate::{
    dialog::{self, MessageDialogOptions},
    AppInfo, Icon, Size, ThumbButton,
};
use gtk::{
    gio::{
        glib::{Cast, GString, ToVariant},
        prelude::{AppInfoExt, FileExt},
        AppInfoCreateFlags, AppLaunchContext, Cancellable, DBusCallFlags, DBusConnectionFlags, File, FileIcon, ThemedIcon,
    },
    glib::DateTime,
    prelude::{AppChooserExt, IconThemeExt, WidgetExt},
    traits::{AppChooserDialogExt, DialogExt, GtkWindowExt},
    AppChooserDialog, DialogFlags, IconLookupFlags, IconSize, IconTheme, ResponseType,
};
use std::path::Path;

/// Opens the file with the default/associated application
pub fn open_path<P: AsRef<Path>>(file_path: P) -> Result<(), String> {
    let uri = format!("file://{}", file_path.as_ref().to_str().unwrap());
    gtk::gio::AppInfo::launch_default_for_uri(&uri, AppLaunchContext::NONE).map_err(|e| e.message().to_string())
}

/// Opens the file with the specified application
pub fn open_path_with<P1: AsRef<Path>, P2: AsRef<Path>>(file_path: P1, app_path: P2) -> Result<(), String> {
    let info = gtk::gio::AppInfo::create_from_commandline(app_path.as_ref(), None, AppInfoCreateFlags::NONE).map_err(|e| e.message().to_string())?;
    info.launch(&[File::for_path(file_path)], AppLaunchContext::NONE).map_err(|e| e.message().to_string())
}

pub fn execute<P1: AsRef<Path>, P2: AsRef<Path>>(file_path: P1, app_path: P2) -> Result<(), String> {
    let info = gtk::gio::AppInfo::create_from_commandline(app_path.as_ref(), None, AppInfoCreateFlags::NEEDS_TERMINAL).map_err(|e| e.message().to_string())?;
    info.launch(&[File::for_path(file_path)], AppLaunchContext::NONE).map_err(|e| e.message().to_string())
}

pub fn execute_as<P1: AsRef<Path>, P2: AsRef<Path>>(file_path: P1, app_path: P2) -> Result<(), String> {
    execute(file_path, app_path)
}

/// Shows the application chooser dialog
pub fn show_open_with_dialog<P: AsRef<Path>>(file_path: P) -> Result<(), String> {
    init();
    let file = File::for_path(file_path.as_ref().to_str().unwrap());
    let dialog = AppChooserDialog::new(gtk::Window::NONE, DialogFlags::DESTROY_WITH_PARENT, &file);

    dialog.connect_response(move |dialog, response_type| {
        if response_type == ResponseType::Ok {
            if let Some(app_info) = dialog.app_info() {
                let _ = app_info.launch(&[dialog.gfile().unwrap()], AppLaunchContext::NONE).map_err(|e| e.message().to_string());
            }
        }

        dialog.close();
    });

    dialog.show();

    Ok(())
}

fn resolve_themed_icon(icon_name: &str, size: Option<i32>) -> String {
    init();
    let theme = IconTheme::default().unwrap();
    let icon_size = if let Some(size) = size {
        size
    } else {
        IconSize::Dialog.into()
    };
    if let Some(path) = theme.lookup_icon(icon_name, icon_size, IconLookupFlags::empty()) {
        return path.filename().unwrap_or_default().to_string_lossy().to_string();
    }
    String::new()
}

/// Lists the applications that can open the file
pub fn get_open_with<P: AsRef<Path>>(file_path: P) -> Vec<AppInfo> {
    let mut apps = Vec::new();
    let content_type = get_mime_type(file_path);

    for app_info in gtk::gio::AppInfo::all_for_type(&content_type) {
        let name = app_info.display_name().to_string();
        let path = app_info.commandline().unwrap_or_default().to_string_lossy().to_string();
        let icon_path = if let Some(icon) = app_info.icon() {
            if let Some(themed_icon) = icon.downcast_ref::<ThemedIcon>() {
                resolve_themed_icon(themed_icon.names().first().unwrap_or(&GString::new()).as_str(), None)
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
            icon_path,
        });
    }
    apps
}

/// Extracts an icon from executable/icon file or an icon stored in a file's associated executable file
pub fn extract_icon<P: AsRef<Path>>(path_or_name: P, size: Size) -> Result<Icon, String> {
    init();

    let content_type = get_mime_type(path_or_name);
    let size: i32 = size.width.max(size.height) as _;

    if let Some(info) = gtk::gio::AppInfo::default_for_type(&content_type, false) {
        if let Some(icon) = info.icon() {
            let icon_path = if let Some(themed_icon) = icon.downcast_ref::<ThemedIcon>() {
                resolve_themed_icon(themed_icon.names().first().unwrap_or(&GString::new()).as_str(), Some(size))
            } else if let Some(file_icon) = icon.downcast_ref::<FileIcon>() {
                file_icon.file().path().unwrap_or_default().to_string_lossy().to_string()
            } else {
                return Err("No icon found".to_string());
            };
            return Ok(Icon {
                file: icon_path,
            });
        }
    }

    Err("No icon found".to_string())
}

/// Shows the file/directory property dialog
pub fn open_file_property<P: AsRef<Path>>(file_path: P) -> Result<(), String> {
    let info = fs::stat_inner(file_path.as_ref())?;
    let message = vec![
        format!("Name: {}", file_path.as_ref().file_name().unwrap_or_default().to_string_lossy()),
        format!("File Type: {}", file_path.as_ref().extension().unwrap_or_default().to_string_lossy()),
        format!("Location: {}", file_path.as_ref().parent().unwrap_or(Path::new("")).to_string_lossy()),
        format!("Size: {}", info.size()),
        format!("Created Date: {:?}", DateTime::from_unix_local(info.attribute_uint64("time::created") as _).unwrap().format_iso8601().unwrap()),
        format!("Last Modified Date: {:?}", DateTime::from_unix_local(info.attribute_uint64("time::modified") as _).unwrap().format_iso8601().unwrap()),
        format!("Last Access Date: {:?}", DateTime::from_unix_local(info.attribute_uint64("time::access") as _).unwrap().format_iso8601().unwrap()),
        format!("Readonly: {}", info.boolean("filesystem::readonly")),
        format!("Hidden: {}", info.is_hidden()),
    ]
    .join("\r\n");
    let options = MessageDialogOptions {
        title: Some(file_path.as_ref().file_name().unwrap().to_string_lossy().to_string()),
        kind: Some(dialog::MessageDialogKind::Info),
        buttons: Vec::new(),
        message,
        cancel_id: None,
    };

    smol::block_on(async move {
        dialog::message(options).await;
    });

    Ok(())
}

pub fn show_item_in_folder<P: AsRef<Path>>(file_path: P) -> Result<(), String> {
    let bus = gtk::gio::bus_get_sync(gtk::gio::BusType::Session, Cancellable::NONE).unwrap();
    let conn = gtk::gio::DBusConnection::new_sync(&bus.stream(), None, DBusConnectionFlags::NONE, None, Cancellable::NONE).unwrap();
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

#[allow(unused_variables)]
/// Does nothing on Linux
pub fn set_thumbar_buttons<F: Fn(String) + 'static>(window_handle: isize, buttons: &[ThumbButton], callback: F) -> Result<(), String> {
    Ok(())
}

pub fn get_locale() -> String {
    if let Some(language) = gtk::default_language() {
        language.to_string()
    } else {
        String::new()
    }
}

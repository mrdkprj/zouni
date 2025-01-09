use gio::{glib::ToVariant, prelude::FileExt, Cancellable, DBusCallFlags, DBusConnectionFlags, File};
use gtk::{prelude::WidgetExt, DialogFlags};

pub fn open_file_property(_window_handle: isize, _file_path: String) -> Result<(), String> {
    Ok(())
}

pub fn open_path(_window_handle: isize, file_path: String) -> Result<(), String> {
    gio::AppInfo::launch_default_for_uri(&file_path, gio::AppLaunchContext::NONE).map_err(|e| e.message().to_string())
}

pub fn open_path_with(_window_handle: isize, file_path: String) -> Result<(), String> {
    let file = File::for_parse_name(&file_path);
    let dialog = gtk::AppChooserDialog::new(gtk::Window::NONE, DialogFlags::DESTROY_WITH_PARENT, &file);
    dialog.show_all();
    Ok(())
}

pub fn show_item_in_folder(file_path: String) -> Result<(), String> {
    let bus = gio::bus_get_sync(gio::BusType::Session, Cancellable::NONE).unwrap();
    let conn = gio::DBusConnection::new_sync(&bus.stream(), None, DBusConnectionFlags::NONE, None, Cancellable::NONE).unwrap();
    let t = ("ss".to_string(), file_path).to_variant();
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

pub fn trash(file: String) -> Result<(), String> {
    let file = File::for_parse_name(&file);
    file.trash(Cancellable::NONE).map_err(|e| e.message().to_string())
}

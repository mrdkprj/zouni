use super::{
    fs::{self, get_mime_type},
    util::init,
};
use crate::{AppInfo, Icon, Size, ThumbButton};
use gio::{FileInfo, FileQueryInfoFlags, FileType};
use gtk::{
    gio::{
        glib::{Cast, GString},
        prelude::{AppInfoExt, FileExt},
        AppInfoCreateFlags, AppLaunchContext, Cancellable, File, FileIcon, ThemedIcon,
    },
    glib::DateTime,
    prelude::{AppChooserExt, IconThemeExt, WidgetExt},
    traits::{AppChooserDialogExt, BoxExt, CssProviderExt, DialogExt, GtkWindowExt, HeaderBarExt, LabelExt, OrientableExt, StyleContextExt, ToggleButtonExt},
    Align, AppChooserDialog, CssProvider, Dialog, DialogFlags, IconLookupFlags, IconSize, IconTheme, Label, Orientation, ResponseType, STYLE_PROVIDER_PRIORITY_APPLICATION,
};
use std::{
    path::Path,
    process::{Command, Stdio},
};

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

fn to_path_from_gicon(icon: Option<gio::Icon>, size: Option<i32>) -> String {
    init();
    if let Some(icon) = icon {
        if let Some(themed_icon) = icon.downcast_ref::<ThemedIcon>() {
            resolve_themed_icon(&themed_icon.names(), size)
        } else if let Some(file_icon) = icon.downcast_ref::<FileIcon>() {
            file_icon.file().path().unwrap_or_default().to_string_lossy().to_string()
        } else {
            String::new()
        }
    } else {
        String::new()
    }
}

fn resolve_themed_icon(icon_names: &[GString], size: Option<i32>) -> String {
    let theme = IconTheme::default().unwrap();
    let icon_size = if let Some(size) = size {
        size
    } else {
        IconSize::Dialog.into()
    };

    for icon_name in icon_names {
        if let Some(path) = theme.lookup_icon(icon_name, icon_size, IconLookupFlags::empty()) {
            return path.filename().unwrap_or_default().to_string_lossy().to_string();
        }
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
        let icon_path = to_path_from_gicon(app_info.icon(), None);
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
        let icon_path = to_path_from_gicon(info.icon(), Some(size));
        if icon_path.is_empty() {
            return Err("No icon found".to_string());
        } else {
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
    let dialog = create_progress_dialog(file_path, info);
    dialog.set_position(gtk::WindowPosition::Mouse);
    dialog.show_all();

    Ok(())
}

fn count_items<P: AsRef<Path>>(file_path: P, item_count: &mut u32, dir_count: &mut u32) {
    let dir = gio::File::for_parse_name(file_path.as_ref().to_str().unwrap());
    for info in dir.enumerate_children("standard::name,standard::type", FileQueryInfoFlags::NOFOLLOW_SYMLINKS, Cancellable::NONE).unwrap().flatten() {
        if info.file_type() == FileType::Directory {
            *dir_count += 1;
            let mut full_path = dir.path().unwrap().to_path_buf();
            full_path.push(info.name());
            count_items(full_path, item_count, dir_count);
        } else {
            *item_count += 1;
        }
    }
}

fn create_progress_dialog<P: AsRef<Path>>(file_path: P, info: FileInfo) -> Dialog {
    init();

    let mut gp1 = vec![
        ("File Type", info.attribute_string("standard::content-type").unwrap_or_default().to_string()),
        ("Location", file_path.as_ref().parent().unwrap().to_string_lossy().to_string()),
        ("Size", format!("{:?}KB", info.size())),
    ];
    if file_path.as_ref().is_dir() {
        let mut item_count = 0;
        let mut dir_count = 0;
        count_items(file_path.as_ref(), &mut item_count, &mut dir_count);
        gp1.push(("Content", format!("Files:{:?}, Directories:{:?}", item_count, dir_count)));
    }
    let gp2 = [
        ("Modified Date", DateTime::from_unix_local(info.attribute_uint64("time::modified") as _).unwrap().format_iso8601().unwrap().to_string()),
        ("Created Date", DateTime::from_unix_local(info.attribute_uint64("time::created") as _).unwrap().format_iso8601().unwrap().to_string()),
        ("Accessed Date", DateTime::from_unix_local(info.attribute_uint64("time::access") as _).unwrap().format_iso8601().unwrap().to_string()),
    ];
    let gp3 = [("Readonly", info.boolean("filesystem::readonly")), ("Hidden", info.is_hidden())];

    let icon_path = if let Ok(icon) = extract_icon(
        file_path.as_ref(),
        Size {
            width: 50,
            height: 50,
        },
    ) {
        icon.file
    } else {
        to_path_from_gicon(info.icon(), Some(50))
    };

    // Dialog
    let dialog = Dialog::new();
    dialog.set_width_request(500);

    // HeaderBar
    let header = gtk::HeaderBar::new();
    header.set_show_close_button(true);
    let css_provider = CssProvider::new();
    let css = r#"
        headerbar entry,
        headerbar spinbutton,
        headerbar button,
        headerbar separator {
            margin-top: 0px; /* same as headerbar side padding for nicer proportions */
            margin-bottom: 0px;
        }

        headerbar {
            min-height: 0px;
            padding-left: 2px; /* same as childrens vertical margins for nicer proportions */
            padding-right: 2px;
            margin: 0px; /* same as headerbar side padding for nicer proportions */
            padding: 0px;
        }
    "#;
    css_provider.load_from_data(css.as_bytes()).unwrap();
    header.style_context().add_provider(&css_provider, STYLE_PROVIDER_PRIORITY_APPLICATION);
    dialog.set_titlebar(Some(&header));
    dialog.set_title(&format!("Property:{}", file_path.as_ref().file_name().unwrap().to_string_lossy()));

    let content_area = dialog.content_area();
    content_area.set_orientation(Orientation::Vertical);
    content_area.set_halign(Align::Fill);
    content_area.set_hexpand(true);
    content_area.set_expand(true);

    // css
    let cont_css_prov = CssProvider::new();
    let cont_css = r#"
        dialog box {
        background-color:#fff;
            border: 1px solid #ccc;
            border-radius:5px;
            margin:10px;
        }
    "#;
    cont_css_prov.load_from_data(cont_css.as_bytes()).unwrap();
    let header_css_prov = CssProvider::new();
    let header_css = r#"
        dialog label {
            color:rgb(99, 107, 116);
        }
    "#;
    header_css_prov.load_from_data(header_css.as_bytes()).unwrap();
    let label_css_prov = CssProvider::new();
    let label_css = r#"
        dialog label {
            color:#000000;
        }
    "#;
    label_css_prov.load_from_data(label_css.as_bytes()).unwrap();
    let box_prov = CssProvider::new();
    let box_css = r#"
        dialog box {
            padding: 5px 20px;
        }
    "#;
    box_prov.load_from_data(box_css.as_bytes()).unwrap();

    // Icon
    let container = gtk::Box::new(Orientation::Horizontal, 5);
    let img = gtk::Image::from_file(&icon_path);
    img.set_height_request(50);
    let label = Label::new(Some(&file_path.as_ref().file_name().unwrap().to_string_lossy()));
    label.set_xalign(0.0);
    container.pack_start(&img, true, false, 0);
    container.pack_start(&label, true, true, 0);
    content_area.pack_start(&container, true, true, 0);

    // Group1
    let container = gtk::Box::new(Orientation::Vertical, 5);
    container.style_context().add_provider(&cont_css_prov, STYLE_PROVIDER_PRIORITY_APPLICATION);
    for (i, data) in gp1.iter().enumerate() {
        let label_box = gtk::Box::new(Orientation::Vertical, 5);
        label_box.style_context().add_provider(&box_prov, STYLE_PROVIDER_PRIORITY_APPLICATION);
        let label = Label::new(Some(data.0));
        label.set_xalign(0.0);
        let data = Label::new(Some(&data.1));
        data.set_xalign(0.0);
        label.style_context().add_provider(&header_css_prov, STYLE_PROVIDER_PRIORITY_APPLICATION);
        data.style_context().add_provider(&label_css_prov, STYLE_PROVIDER_PRIORITY_APPLICATION);
        label_box.pack_start(&label, true, true, 0);
        label_box.pack_start(&data, true, true, 0);
        container.pack_start(&label_box, true, true, 0);
        if i + 1 != gp1.len() {
            container.pack_start(&gtk::Separator::new(Orientation::Horizontal), true, true, 0);
        }
    }
    content_area.pack_start(&container, true, true, 0);

    // Group2
    let container = gtk::Box::new(Orientation::Vertical, 5);
    container.style_context().add_provider(&cont_css_prov, STYLE_PROVIDER_PRIORITY_APPLICATION);
    for (i, data) in gp2.iter().enumerate() {
        let label_box = gtk::Box::new(Orientation::Vertical, 5);
        label_box.style_context().add_provider(&box_prov, STYLE_PROVIDER_PRIORITY_APPLICATION);
        let label = Label::new(Some(data.0));
        label.set_xalign(0.0);
        let data = Label::new(Some(&data.1));
        data.set_xalign(0.0);
        label.style_context().add_provider(&header_css_prov, STYLE_PROVIDER_PRIORITY_APPLICATION);
        data.style_context().add_provider(&label_css_prov, STYLE_PROVIDER_PRIORITY_APPLICATION);
        label_box.pack_start(&label, true, true, 0);
        label_box.pack_start(&data, true, true, 0);
        container.pack_start(&label_box, true, true, 0);
        if i + 1 != gp2.len() {
            container.pack_start(&gtk::Separator::new(Orientation::Horizontal), true, true, 0);
        }
    }
    content_area.pack_start(&container, true, true, 0);

    // Group3
    let container = gtk::Box::new(Orientation::Horizontal, 5);
    for data in gp3 {
        let label = Label::new(Some(data.0));
        label.set_xalign(0.0);
        let chk = gtk::CheckButton::new();
        chk.set_margin_start(10);
        chk.set_sensitive(false);
        chk.set_active(data.1);
        container.pack_start(&chk, false, false, 0);
        container.pack_start(&label, false, false, 0);
    }
    container.set_margin_top(5);
    container.set_margin_bottom(20);
    content_area.pack_start(&container, true, true, 0);

    dialog.connect_destroy(|dialog| {
        dialog.close();
    });

    dialog.connect_close(|dialog| {
        // The default binding for this signal is the Escape key
        dialog.close();
    });

    dialog.connect_response(|dialog, response| {
        if response == ResponseType::Cancel || response == ResponseType::Close {
            dialog.close();
        }
    });

    dialog
}

pub fn show_item_in_folder<P: AsRef<Path>>(file_path: P) -> Result<(), String> {
    use zbus::blocking::Connection;
    // Cannot use opener crate due to its dependency
    if is_wsl() {
        return reveal_in_windows_explorer(file_path);
    }
    let connection = Connection::session().map_err(|e| e.to_string())?;
    let path = file_path.as_ref().canonicalize().map_err(|e| e.to_string())?;
    let uri = format!("file://{}", path.display());
    let proxy = FileManager1Proxy::new(&connection).map_err(|e| e.to_string())?;
    proxy.show_items(&[&uri], "").map_err(|e| e.to_string())
}

/// # D-Bus interface proxy for `org.freedesktop.FileManager1` interface.
#[zbus::proxy(gen_async = false, interface = "org.freedesktop.FileManager1", default_service = "org.freedesktop.FileManager1", default_path = "/org/freedesktop/FileManager1")]
trait FileManager1 {
    /// ShowItems method
    fn show_items(&self, uris: &[&str], startup_id: &str) -> zbus::Result<()>;
}

fn reveal_in_windows_explorer<P: AsRef<Path>>(file_path: P) -> Result<(), String> {
    let path = file_path.as_ref().to_path_buf();
    let converted_path = wsl_to_windows_path(path.as_os_str());
    let converted_path = converted_path.as_deref();
    let path = match converted_path {
        None => path,
        Some(x) => std::path::PathBuf::from(x),
    };
    Command::new("explorer.exe").arg("/select,").arg(path).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map_err(|e| e.to_string())?;

    Ok(())
}

fn wsl_to_windows_path(path: &std::ffi::OsStr) -> Option<std::ffi::OsString> {
    use bstr::ByteSlice;
    use std::os::unix::ffi::OsStringExt;

    let output = Command::new("wslpath").arg("-w").arg(path).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).output().ok()?;

    if !output.status.success() {
        return None;
    }

    Some(std::ffi::OsString::from_vec(output.stdout.trim_end().to_vec()))
}

fn is_wsl() -> bool {
    if let Ok(true) = std::fs::read_to_string("/proc/sys/kernel/osrelease").map(|osrelease| osrelease.to_ascii_lowercase().contains("microsoft")) {
        return true;
    }

    if let Ok(true) = std::fs::read_to_string("/proc/version").map(|version| version.to_ascii_lowercase().contains("microsoft")) {
        return true;
    }

    false
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

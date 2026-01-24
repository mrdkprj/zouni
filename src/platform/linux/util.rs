use std::{
    collections::HashMap,
    fs::File,
    os::fd::AsFd,
    path::{Path, PathBuf},
};
use url::Url;
use zbus::blocking::Connection;

pub(crate) fn init() {
    if !gtk::is_initialized() {
        let _ = gtk::init();
    }
}

// We should prefer the OpenURI interface, because it correctly handles runtimes such as Flatpak.
// However, OpenURI was broken in the original version of the interface (it did not highlight the items).
// This version is still in use by some distributions, which would result in degraded functionality for some users.
// That's why we're first trying to use the FileManager1 interface, falling back to the OpenURI interface.
// Source: https://chromium-review.googlesource.com/c/chromium/src/+/3009959
pub(crate) fn reveal_with_dbus<P: AsRef<Path>>(path: P) -> Result<(), String> {
    let connection = Connection::session().map_err(|e| e.to_string())?;
    reveal_with_filemanager1(path.as_ref().to_path_buf(), &connection).or_else(|_| reveal_with_open_uri_portal(path.as_ref().to_path_buf(), &connection))
}

pub(crate) fn show_item_properties<P: AsRef<Path>>(path: P) -> Result<(), String> {
    let connection = Connection::session().map_err(|e| e.to_string())?;
    let uri = path_to_uri(path.as_ref().to_path_buf())?;
    let proxy = FileManager1Proxy::new(&connection).map_err(|e| e.to_string())?;
    proxy.show_item_properties(&[uri], "").map_err(|e| e.to_string())
}

fn reveal_with_filemanager1(path: PathBuf, connection: &Connection) -> Result<(), String> {
    let uri = path_to_uri(path)?;
    let proxy = FileManager1Proxy::new(connection).map_err(|e| e.to_string())?;
    proxy.show_items(&[uri], "").map_err(|e| e.to_string())
}

fn reveal_with_open_uri_portal(path: PathBuf, connection: &Connection) -> Result<(), String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let proxy = OpenURIProxy::new(connection).map_err(|e| e.to_string())?;
    proxy.open_directory("", file.as_fd().into(), HashMap::new()).map_err(|e| e.to_string())?;
    Ok(())
}

fn path_to_uri(path: PathBuf) -> Result<Url, String> {
    let path = path.canonicalize().map_err(|e| e.to_string())?;
    Ok(Url::from_file_path(path).unwrap())
}

/// # D-Bus interface proxy for `org.freedesktop.FileManager1` interface.
// https://www.freedesktop.org/wiki/Specifications/file-manager-interface/
#[zbus::proxy(gen_async = false, interface = "org.freedesktop.FileManager1", default_service = "org.freedesktop.FileManager1", default_path = "/org/freedesktop/FileManager1")]
trait FileManager1 {
    fn show_items(&self, uris: &[Url], startup_id: &str) -> zbus::Result<()>;
    fn show_item_properties(&self, uris: &[Url], startup_id: &str) -> zbus::Result<()>;
}

/// # D-Bus interface proxy for: `org.freedesktop.portal.OpenURI`
#[zbus::proxy(gen_async = false, interface = "org.freedesktop.portal.OpenURI", default_service = "org.freedesktop.portal.Desktop", default_path = "/org/freedesktop/portal/desktop")]
pub trait OpenURI {
    fn open_directory(&self, parent_window: &str, fd: zbus::zvariant::Fd<'_>, options: HashMap<&str, &zbus::zvariant::Value<'_>>) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;
}

use crate::{ClipboardData, Operation};
use gtk::{
    gdk::{SELECTION_CLIPBOARD, SELECTION_PRIMARY},
    TargetEntry, TargetFlags,
};

use super::util::init;

pub fn is_text_available() -> bool {
    init();

    let clipboard = gtk::Clipboard::get(&SELECTION_PRIMARY);
    clipboard.wait_is_text_available()
}

pub fn read_text(_window_handle: isize) -> Result<String, String> {
    init();

    if is_text_available() {
        return Ok(String::new());
    }

    let clipboard = gtk::Clipboard::get(&SELECTION_CLIPBOARD);
    Ok(clipboard.wait_for_text().unwrap_or_default().to_string())
}

pub fn write_text(_window_handle: isize, text: String) -> Result<(), String> {
    init();

    let clipboard = gtk::Clipboard::get(&SELECTION_CLIPBOARD);
    clipboard.set_text(&text);

    Ok(())
}

pub fn is_uris_available() -> bool {
    init();

    let clipboard = gtk::Clipboard::get(&SELECTION_PRIMARY);
    clipboard.wait_is_uris_available()
}

pub fn read_uris(_window_handle: isize) -> Result<ClipboardData, String> {
    init();
    let data = ClipboardData {
        operation: Operation::None,
        urls: Vec::new(),
    };

    if !is_uris_available() {
        return Ok(data);
    }

    let clipboard = gtk::Clipboard::get(&SELECTION_CLIPBOARD);

    let urls: Vec<String> = clipboard.wait_for_uris().iter().map(|gs| gs.to_string()).collect();

    return Ok(ClipboardData {
        operation: Operation::None,
        urls,
    });
}

pub fn write_uris(_window_handle: isize, paths: &[String], _operation: Operation) -> Result<(), String> {
    init();

    let clipboard = gtk::Clipboard::get(&SELECTION_CLIPBOARD);

    let targets = &[TargetEntry::new("text/uri-list", TargetFlags::OTHER_APP, 0)];
    let urls = paths.to_vec();

    let _ = clipboard.set_with_data(targets, move |_, selection, _| {
        let uri_list: Vec<&str> = urls.iter().map(|s| s.as_str()).collect();
        let _ = selection.set_uris(uri_list.as_slice());
    });

    Ok(())
}

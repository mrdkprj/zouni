use super::util::init;
use crate::{platform::linux::util::path_to_uri, ClipboardData, Operation};
use gtk::{gdk::SELECTION_CLIPBOARD, TargetEntry, TargetFlags};

/// Checks if text is available
pub fn is_text_available() -> bool {
    init();

    let clipboard = gtk::Clipboard::get(&SELECTION_CLIPBOARD);
    clipboard.wait_is_text_available()
}

/// Reads text from clipboard
///
/// `window_handle` is ignored
pub fn read_text(_window_handle: isize) -> Result<String, String> {
    init();

    if is_text_available() {
        return Ok(String::new());
    }

    let clipboard = gtk::Clipboard::get(&SELECTION_CLIPBOARD);
    Ok(clipboard.wait_for_text().unwrap_or_default().to_string())
}

/// Writes text to clipboard
///
/// `window_handle` is ignored
pub fn write_text(_window_handle: isize, text: String) -> Result<(), String> {
    init();

    let clipboard = gtk::Clipboard::get(&SELECTION_CLIPBOARD);
    clipboard.set_text(&text);

    Ok(())
}

/// Checks if URIs are available
pub fn is_uris_available() -> bool {
    init();

    let clipboard = gtk::Clipboard::get(&SELECTION_CLIPBOARD);
    clipboard.wait_is_uris_available()
}

/// Reads URIs from clipboard
///
/// `window_handle` is ignored
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

    Ok(ClipboardData {
        operation: Operation::None,
        urls,
    })
}

/// Writes URIs to clipboard
///
/// `window_handle` and `operation` are ignored
pub fn write_uris(_window_handle: isize, paths: &[String], operation: Operation) -> Result<(), String> {
    init();

    let clipboard = gtk::Clipboard::get(&SELECTION_CLIPBOARD);

    let targets: Vec<TargetEntry> =
        ["text/uri-list", "x-special/gnome-copied-files", "application/x-kde-cutselection"].iter().map(|target| TargetEntry::new(target, TargetFlags::empty(), 0)).collect();

    let paths_vec = paths.to_vec();
    let uris = paths_vec.iter().filter_map(|path| path_to_uri(path).ok()).collect::<Vec<_>>();
    let uris_ref = uris.iter().map(|uri| uri.to_string()).collect::<Vec<_>>();
    let mut payloads = if operation == Operation::Move {
        vec!["cut"]
    } else {
        vec!["copy"]
    };
    for uri in &uris_ref {
        payloads.push(uri);
    }
    let payload = payloads.join("\n");

    let _ = clipboard.set_with_data(&targets, move |_, selection, _| match selection.target().name().as_str() {
        "x-special/gnome-copied-files" => {
            println!("1");
            let _ = selection.set(&selection.target(), 8, payload.as_bytes());
        }
        "application/x-kde-cutselection" => {
            let _ = selection.set(
                &selection.target(),
                8,
                if operation == Operation::Move {
                    b"1"
                } else {
                    b"0"
                },
            );
        }
        _ => {
            let uris: Vec<&str> = payload.lines().skip(1).collect();
            let _ = selection.set_uris(&uris);
        }
    });

    Ok(())
}

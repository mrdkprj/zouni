use crate::{ClipboardData, Operation};
use gtk::{gdk::Display, Clipboard, TargetEntry, TargetFlags};

pub fn is_text_available() -> bool {
    if let Some(clipboard) = Clipboard::default(&Display::default().unwrap()) {
        return clipboard.wait_is_text_available();
    }

    false
}

pub fn read_text(_window_handle: isize) -> Result<String, String> {
    if let Some(clipboard) = Clipboard::default(&Display::default().unwrap()) {
        if clipboard.wait_is_uris_available() {
            return Ok(clipboard.wait_for_text().unwrap_or_default().to_string());
        }
    }

    Ok(String::new())
}

pub fn write_text(_window_handle: isize, text: String) -> Result<(), String> {
    if let Some(clipboard) = Clipboard::default(&Display::default().unwrap()) {
        clipboard.set_text(&text);
    }

    Ok(())
}

pub fn is_uris_available() -> bool {
    if let Some(clipboard) = Clipboard::default(&Display::default().unwrap()) {
        return clipboard.wait_is_uris_available();
    }

    false
}

pub fn read_uris(_window_handle: isize) -> Result<ClipboardData, String> {
    let data = ClipboardData {
        operation: Operation::None,
        urls: Vec::new(),
    };

    if let Some(clipboard) = Clipboard::default(&Display::default().unwrap()) {
        if clipboard.wait_is_uris_available() {
            let urls: Vec<String> = clipboard.wait_for_uris().iter().map(|gs| gs.to_string()).collect();

            return Ok(ClipboardData {
                operation: Operation::None,
                urls,
            });
        }
    }

    Ok(data)
}

pub fn write_uris(_window_handle: isize, paths: &[String], _operation: Operation) -> Result<(), String> {
    if let Some(clipboard) = Clipboard::default(&Display::default().unwrap()) {
        let targets = &[TargetEntry::new("text/uri-list", TargetFlags::OTHER_APP, 0)];
        let urls = paths.to_vec();
        let _ = clipboard.set_with_data(targets, move |_, selection, _| {
            let uri_list: Vec<&str> = urls.iter().map(|s| s.as_str()).collect();
            selection.set_uris(uri_list.as_slice());
        });
    }
    Ok(())
}

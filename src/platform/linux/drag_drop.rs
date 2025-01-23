#![allow(unused_imports)]
use crate::{platform::linux::util::init, Operation};
use gio::glib::translate::FromGlibPtrNone;
use gtk::{ffi::GtkWidget, prelude::WidgetExt, TargetEntry, Widget};

pub fn start_drag(_: isize, file_paths: Vec<String>, _operation: Operation) -> Result<(), String> {
    println!("1");
    init();

    let widgets = gtk::Window::list_toplevels();
    if widgets.is_empty() {
        return Ok(());
    }
    let widget = widgets.get(0).unwrap();
    println!("2");
    let targets = gtk::TargetList::new(&[TargetEntry::new("text/uri-list", gtk::TargetFlags::OTHER_APP, 0)]);

    widget.drag_begin_with_coordinates(&targets, gtk::gdk::DragAction::COPY, 1, None, -1, -1);
    println!("3");
    widget.connect_drag_data_get(move |_, _context, selection_data, info, _time| {
        println!("4");
        if info == 0 {
            let uris = file_paths.iter().map(|path| format!("file://{}", path)).collect::<Vec<_>>();
            let uris_ref: Vec<&str> = uris.iter().map(|uri| uri.as_str()).collect();
            // Set the URIs as the data
            selection_data.set_uris(&uris_ref);
        }
    });

    widget.connect_drag_end(move |_widget, _context| {
        println!("Drag operation ended");
    });

    Ok(())
}

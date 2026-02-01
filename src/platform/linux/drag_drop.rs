use crate::{
    platform::linux::util::{init, path_to_uri},
    Operation,
};
use gtk::{gdk::DragAction, prelude::WidgetExt, TargetEntry, TargetFlags};

/// Starts dragging
pub fn start_drag(file_paths: Vec<String>, operation: Operation) -> Result<(), String> {
    init();

    let widgets = gtk::Window::list_toplevels();
    if widgets.is_empty() {
        return Ok(());
    }
    let widget = widgets.first().unwrap();

    let targets = gtk::TargetList::new(&[TargetEntry::new("text/uri-list", TargetFlags::OTHER_APP, 0)]);

    let action = match operation {
        Operation::Copy => DragAction::COPY,
        Operation::Move => DragAction::MOVE,
        Operation::None => DragAction::DEFAULT,
    };
    widget.drag_begin_with_coordinates(&targets, action, 1, None, -1, -1);

    widget.connect_drag_data_get(move |_, _context, selection_data, info, _time| {
        if info == 0 {
            let uris = file_paths.iter().filter_map(|path| path_to_uri(path).ok()).map(|url| url.to_string()).collect::<Vec<_>>();
            let uris_ref: Vec<&str> = uris.iter().map(|uri| uri.as_str()).collect();
            selection_data.set_uris(&uris_ref);
        }
    });

    Ok(())
}

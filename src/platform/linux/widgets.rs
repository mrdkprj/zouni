use crate::fs::try_cancel;
use crate::platform::linux::fs_ext::FileOperation;
use gtk::{
    gdk_pixbuf::{traits::PixbufLoaderExt, InterpType, PixbufLoader},
    glib::{self, clone, ObjectExt},
    prelude::DialogExtManual,
    traits::{BoxExt, ButtonExt, CssProviderExt, DialogExt, GtkWindowExt, HeaderBarExt, LabelExt, OrientableExt, ProgressBarExt, StyleContextExt, ToggleButtonExt, WidgetExt},
    Align, CssProvider, Dialog, Label, Orientation, ProgressBar, ResponseType, STYLE_PROVIDER_PRIORITY_APPLICATION,
};
use smol::channel::Sender;
use std::path::PathBuf;

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct FileOperationDialog {
    dialog: Dialog,
    progress_bar: ProgressBar,
    message: Label,
    from_name: Option<Label>,
}

#[allow(dead_code)]
impl FileOperationDialog {
    pub(crate) fn show(&self) {
        self.dialog.show_all();
    }

    pub(crate) fn close(&self) {
        self.dialog.close();
    }

    pub(crate) fn set_title(&self, title: &str) {
        self.dialog.set_title(title)
    }

    pub(crate) fn set_message(&self, message: &str) {
        self.message.set_label(message);
    }

    pub(crate) fn set_from_name(&self, name: &str) {
        if let Some(label) = &self.from_name {
            label.set_text(name);
            label.set_tooltip_text(Some(name))
        }
    }

    pub(crate) fn progress(&self, fraction: f64) {
        self.progress_bar.set_fraction(fraction)
    }
}

pub(crate) fn create_progress_dialog(operation: &FileOperation, message: &str, to_item: &str, cancel_id: u32, pause_tx: Sender<bool>) -> FileOperationDialog {
    let dialog = Dialog::new();
    dialog.set_destroy_with_parent(true);

    // CSS
    let css_provider = CssProvider::new();
    let css = r#"
        headerbar entry,
        headerbar spinbutton,
        headerbar button,
        headerbar separator {
            margin-top: 0px; /* same as headerbar side padding for nicer proportions */
            margin-bottom: 0px;
            font-size: 14px;
        }

        headerbar {
            min-height: 0px;
            padding: 0px 2px;
            margin: 0px; /* same as headerbar side padding for nicer proportions */
        }

        label#entrylabel {
            color:blue;
            font-size:12px;
        }

        label#message {
            font-size:14px;
        }

        label {
            font-size:12px;
        }

        #progress_container {
            margin-bottom:10px;
        }

        progressbar{
            min-height:5px;
        }
        progressbar trough{
            min-width: 400px;
            min-height:5px;
        }
        progressbar trough progress{
            min-height:5px;
        }

        #progress-button{
            border:none;
            min-width:16px;
        }
    "#;
    css_provider.load_from_data(css.as_bytes()).unwrap();

    // HeaderBar
    let header = gtk::HeaderBar::new();
    header.set_show_close_button(true);
    header.style_context().add_provider(&css_provider, STYLE_PROVIDER_PRIORITY_APPLICATION);
    dialog.set_titlebar(Some(&header));
    dialog.set_title("0% complete");

    let content_area = dialog.content_area();
    content_area.set_orientation(Orientation::Vertical);
    content_area.set_halign(Align::Start);
    content_area.set_hexpand(false);

    // Message Label
    let message_label_container = gtk::Box::new(Orientation::Vertical, 5);
    let messge_label = Label::new(Some(&message));
    messge_label.set_xalign(0.0);
    messge_label.set_margin_start(10);
    messge_label.set_widget_name("message");
    messge_label.style_context().add_provider(&css_provider, STYLE_PROVIDER_PRIORITY_APPLICATION);
    message_label_container.pack_start(&messge_label, false, false, 0);

    let from_name = if *operation == FileOperation::Copy || *operation == FileOperation::Move {
        // From/To Label
        let progress_label_container = gtk::Box::new(Orientation::Horizontal, 0);
        let from_label = Label::new(Some("From "));
        from_label.set_xalign(0.0);
        from_label.style_context().add_provider(&css_provider, STYLE_PROVIDER_PRIORITY_APPLICATION);
        let from = Label::new(Some("..."));
        from.set_xalign(0.0);
        from.set_widget_name("entrylabel");
        from.set_max_width_chars(20);
        from.set_ellipsize(gtk::pango::EllipsizeMode::End);
        from.set_tooltip_text(Some(""));
        from.style_context().add_provider(&css_provider, STYLE_PROVIDER_PRIORITY_APPLICATION);
        let to_label = Label::new(Some(" to "));
        to_label.set_xalign(0.0);
        to_label.style_context().add_provider(&css_provider, STYLE_PROVIDER_PRIORITY_APPLICATION);
        let to = Label::new(Some(to_item));
        to.set_xalign(0.0);
        to.set_widget_name("entrylabel");
        to.set_max_width_chars(20);
        to.set_ellipsize(gtk::pango::EllipsizeMode::End);
        to.set_tooltip_text(Some(to_item));
        to.style_context().add_provider(&css_provider, STYLE_PROVIDER_PRIORITY_APPLICATION);

        progress_label_container.pack_start(&from_label, false, false, 0);
        progress_label_container.pack_start(&from, false, false, 0);
        progress_label_container.pack_start(&to_label, false, false, 0);
        progress_label_container.pack_start(&to, false, false, 0);
        progress_label_container.set_margin_start(10);
        progress_label_container.set_width_request(100);
        message_label_container.pack_start(&progress_label_container, false, false, 0);
        content_area.pack_start(&message_label_container, false, false, 5);

        Some(from)
    } else {
        None
    };

    // ProgressBar
    let progress_container = gtk::Box::new(Orientation::Horizontal, 5);
    progress_container.set_widget_name("progress_container");
    progress_container.style_context().add_provider(&css_provider, STYLE_PROVIDER_PRIORITY_APPLICATION);
    let progress_bar = ProgressBar::new();
    progress_bar.set_height_request(5);
    progress_bar.style_context().add_provider(&css_provider, STYLE_PROVIDER_PRIORITY_APPLICATION);
    progress_bar.set_fraction(0.0);
    progress_bar.set_valign(Align::Center);
    // Pause buttons
    let pause_button = gtk::Button::new();
    pause_button.set_widget_name("progress-button");
    pause_button.style_context().add_provider(&css_provider, STYLE_PROVIDER_PRIORITY_APPLICATION);
    let pause = create_image(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="currentColor" class="bi bi-pause-fill" viewBox="0 0 16 16">
            <path d="M5.5 3.5A1.5 1.5 0 0 1 7 5v6a1.5 1.5 0 0 1-3 0V5a1.5 1.5 0 0 1 1.5-1.5m5 0A1.5 1.5 0 0 1 12 5v6a1.5 1.5 0 0 1-3 0V5a1.5 1.5 0 0 1 1.5-1.5"/>
        </svg>"#,
        16,
        16,
    );
    let resume = create_image(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="currentColor" class="bi bi-caret-right-fill" viewBox="0 0 16 16">
            <path d="m12.14 8.753-5.482 4.796c-.646.566-1.658.106-1.658-.753V3.204a1 1 0 0 1 1.659-.753l5.48 4.796a1 1 0 0 1 0 1.506z"/>
        </svg>"#,
        16,
        16,
    );
    pause_button.set_image(Some(&pause));
    pause_button.set_size_request(1, 1);
    pause_button.set_can_focus(false);
    pause_button.set_relief(gtk::ReliefStyle::None);
    pause_button.set_focus_on_click(false);
    unsafe { pause_button.set_data("paused", false) };
    pause_button.connect_button_release_event(clone!(@strong pause, @strong resume => @default-return gio::glib::Propagation::Proceed, move |pause_button, _| {
        let paused = unsafe { pause_button.data::<bool>("paused") .unwrap().as_mut() };
        if *paused {
            pause_button.set_image(Some(&pause));
        }else {
            pause_button.set_image(Some(&resume));
        }
        *paused = !*paused;
        let _ = pause_tx.try_send(*paused);

        gio::glib::Propagation::Proceed
    }));

    // Stop button
    let stop_button = gtk::Button::new();
    stop_button.set_widget_name("progress-button");
    stop_button.style_context().add_provider(&css_provider, STYLE_PROVIDER_PRIORITY_APPLICATION);
    let stop = create_image(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="currentColor" class="bi bi-stop-fill" viewBox="0 0 16 16">
            <path d="M5 3.5h6A1.5 1.5 0 0 1 12.5 5v6a1.5 1.5 0 0 1-1.5 1.5H5A1.5 1.5 0 0 1 3.5 11V5A1.5 1.5 0 0 1 5 3.5"/>
        </svg>"#,
        16,
        16,
    );
    stop_button.set_image(Some(&stop));
    stop_button.set_size_request(1, 1);
    stop_button.set_can_focus(false);
    stop_button.set_relief(gtk::ReliefStyle::None);
    stop_button.set_focus_on_click(false);
    stop_button.set_margin_end(5);
    stop_button.connect_button_release_event(clone!(@weak dialog => @default-return gio::glib::Propagation::Proceed, move |_, _| {
        dialog.response(ResponseType::Cancel);
        gio::glib::Propagation::Proceed
    }));

    progress_container.pack_start(&progress_bar, true, true, 5);
    progress_container.pack_start(&pause_button, false, false, 5);
    progress_container.pack_start(&stop_button, false, false, 0);
    content_area.pack_start(&progress_container, true, true, 5);

    unsafe { dialog.set_data("cancel_id", cancel_id) };

    dialog.connect_destroy(|dialog| {
        try_cancel(dialog);
    });

    dialog.connect_close(|dialog| {
        // The default binding for this signal is the Escape key
        try_cancel(dialog);
    });

    dialog.connect_response(|dialog, response| {
        if response == ResponseType::Cancel || response == ResponseType::Close {
            try_cancel(dialog);
        }
    });

    FileOperationDialog {
        dialog,
        progress_bar,
        message: messge_label,
        from_name,
    }
}

fn create_image(svg: &str, width: i32, height: i32) -> gtk::Image {
    let loader = PixbufLoader::new();
    let result = loader.write_bytes(&gtk::glib::Bytes::from(svg.as_bytes()));
    match result {
        Ok(_) => {
            let _ = loader.close();
            if let Some(pixbuf) = loader.pixbuf() {
                let scaled = pixbuf.scale_simple(width, height, InterpType::Nearest).unwrap_or(pixbuf);
                gtk::Image::from_pixbuf(Some(&scaled))
            } else {
                gtk::Image::new()
            }
        }
        Err(_) => gtk::Image::new(),
    }
}

#[derive(Debug, Clone)]
pub(crate) struct FileReplaceDialog {
    message: gtk::Dialog,
    file_name: Label,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ReplaceOrSkip {
    Replace,
    ReplaceAll,
    Skip,
    SkipAll,
}

const REPLACE: u16 = 0;
const REPLACE_ALL: u16 = 1;
const SKIP: u16 = 2;
const SKIP_ALL: u16 = 3;
fn response_to_enum(response: &ResponseType) -> ReplaceOrSkip {
    match response {
        ResponseType::Other(value) => match *value {
            REPLACE => ReplaceOrSkip::Replace,
            REPLACE_ALL => ReplaceOrSkip::ReplaceAll,
            SKIP => ReplaceOrSkip::Skip,
            SKIP_ALL => ReplaceOrSkip::SkipAll,
            _ => ReplaceOrSkip::Skip,
        },
        _ => ReplaceOrSkip::Skip,
    }
}

impl FileReplaceDialog {
    pub(crate) async fn confirm(&self, file: &PathBuf) -> ReplaceOrSkip {
        self.file_name.set_text(file.to_str().unwrap());
        self.message.show_all();
        let response = self.message.run_future().await;
        self.message.hide();
        response_to_enum(&response)
    }
}

pub(crate) fn create_replace_confirm_dialog(cancel_id: u32) -> FileReplaceDialog {
    let dialog = Dialog::new();
    dialog.set_destroy_with_parent(true);

    // CSS
    let css_provider = CssProvider::new();
    let css = r#"
        headerbar entry,
        headerbar spinbutton,
        headerbar button,
        headerbar separator {
            margin-top: 0px; /* same as headerbar side padding for nicer proportions */
            margin-bottom: 0px;
            font-size: 14px;
        }

        headerbar {
            min-height: 0px;
            padding: 0px 2px;
            margin: 0px; /* same as headerbar side padding for nicer proportions */
        }

        label#message {
            font-size:14px;
        }

        #confirm-button{
            min-width:16px;
        }
    "#;
    css_provider.load_from_data(css.as_bytes()).unwrap();

    // HeaderBar
    let header = gtk::HeaderBar::new();
    header.set_show_close_button(true);
    header.style_context().add_provider(&css_provider, STYLE_PROVIDER_PRIORITY_APPLICATION);
    dialog.set_titlebar(Some(&header));
    dialog.set_title("Confirm File Replace");

    let content_area = dialog.content_area();
    content_area.set_orientation(Orientation::Vertical);
    content_area.set_halign(Align::Start);
    content_area.set_hexpand(false);

    // Message Label
    let message_label_container = gtk::Box::new(Orientation::Vertical, 5);
    let messge_label1 = Label::new(Some("There is already a file with the same name in the destination directory."));
    messge_label1.set_xalign(0.0);
    messge_label1.set_margin_start(10);
    messge_label1.set_margin_end(10);
    messge_label1.set_widget_name("message");
    messge_label1.style_context().add_provider(&css_provider, STYLE_PROVIDER_PRIORITY_APPLICATION);
    let messge_label2 = Label::new(Some("Would you like to replace the existing file?"));
    messge_label2.set_xalign(0.0);
    messge_label2.set_margin_start(10);
    messge_label2.set_margin_end(10);
    messge_label2.set_widget_name("message");
    messge_label2.style_context().add_provider(&css_provider, STYLE_PROVIDER_PRIORITY_APPLICATION);
    message_label_container.pack_start(&messge_label1, false, false, 0);
    message_label_container.pack_start(&messge_label2, false, false, 0);
    content_area.pack_start(&message_label_container, true, true, 5);

    // image
    let images = gtk::Box::new(Orientation::Horizontal, 0);

    let svg = r#"
        <svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" fill="currentColor" class="bi bi-file-earmark-richtext" viewBox="0 0 16 16">
            <path d="M14 4.5V14a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V2a2 2 0 0 1 2-2h5.5zm-3 0A1.5 1.5 0 0 1 9.5 3V1H4a1 1 0 0 0-1 1v12a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1V4.5z"/>
            <path d="M4.5 12.5A.5.5 0 0 1 5 12h3a.5.5 0 0 1 0 1H5a.5.5 0 0 1-.5-.5m0-2A.5.5 0 0 1 5 10h6a.5.5 0 0 1 0 1H5a.5.5 0 0 1-.5-.5m1.639-3.708 1.33.886 1.854-1.855a.25.25 0 0 1 .289-.047l1.888.974V8.5a.5.5 0 0 1-.5.5H5a.5.5 0 0 1-.5-.5V8s1.54-1.274 1.639-1.208M6.25 6a.75.75 0 1 0 0-1.5.75.75 0 0 0 0 1.5"/>
        </svg>
    "#;
    let img = create_image(svg, 48, 48);
    img.set_margin_start(20);
    let file_name = Label::new(None);
    file_name.set_xalign(0.0);
    file_name.set_margin_start(10);
    file_name.set_widget_name("message");
    file_name.set_ellipsize(gtk::pango::EllipsizeMode::End);
    file_name.style_context().add_provider(&css_provider, STYLE_PROVIDER_PRIORITY_APPLICATION);
    images.pack_start(&img, false, false, 0);
    images.pack_start(&file_name, false, false, 0);
    content_area.pack_start(&images, true, true, 5);

    let checkbox = gtk::CheckButton::with_label("Do this for all conflicts");
    checkbox.set_margin_start(10);
    content_area.pack_start(&checkbox, true, true, 5);

    let buttons = gtk::Box::new(Orientation::Horizontal, 5);
    buttons.set_halign(Align::Center);
    let overwrite = gtk::Button::with_label("Overwrite");
    overwrite.set_widget_name("confirm-button");
    overwrite.style_context().add_provider(&css_provider, STYLE_PROVIDER_PRIORITY_APPLICATION);
    let skip = gtk::Button::with_label("Skip");
    skip.set_widget_name("confirm-button");
    skip.style_context().add_provider(&css_provider, STYLE_PROVIDER_PRIORITY_APPLICATION);
    let cancel = gtk::Button::with_label("Cancel");
    cancel.set_widget_name("confirm-button");
    cancel.style_context().add_provider(&css_provider, STYLE_PROVIDER_PRIORITY_APPLICATION);
    buttons.pack_start(&overwrite, false, false, 5);
    buttons.pack_start(&skip, false, false, 5);
    buttons.pack_start(&cancel, false, false, 5);
    content_area.pack_start(&buttons, true, true, 5);

    overwrite.connect_button_release_event(clone!(@weak dialog, @strong checkbox => @default-return gio::glib::Propagation::Proceed, move |_, _| {
        if checkbox.is_active() {
            dialog.response(ResponseType::Other(REPLACE_ALL));
        } else {
            dialog.response(ResponseType::Other(REPLACE));
        }
        gio::glib::Propagation::Proceed
    }));

    skip.connect_button_release_event(clone!(@weak dialog, @strong checkbox => @default-return gio::glib::Propagation::Proceed, move |_, _| {
        if checkbox.is_active() {
            dialog.response(ResponseType::Other(SKIP_ALL));
        } else {
            dialog.response(ResponseType::Other(SKIP));
        }
        gio::glib::Propagation::Proceed
    }));

    cancel.connect_button_release_event(clone!(@weak dialog => @default-return gio::glib::Propagation::Proceed, move |_, _| {
        dialog.response(ResponseType::Cancel);
        gio::glib::Propagation::Proceed
    }));

    unsafe { dialog.set_data("cancel_id", cancel_id) };

    dialog.connect_destroy(|dialog| {
        try_cancel(dialog);
    });

    dialog.connect_close(|dialog| {
        // The default binding for this signal is the Escape key
        try_cancel(dialog);
    });

    dialog.connect_response(|dialog, response| {
        if response == ResponseType::Cancel || response == ResponseType::Close {
            try_cancel(dialog);
        }
    });

    FileReplaceDialog {
        message: dialog,
        file_name,
    }
}

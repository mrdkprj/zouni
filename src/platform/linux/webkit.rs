pub use gtk::{
    gio::Cancellable,
    glib::{Error, IsA},
};
pub use webkit2gtk::WebView;
use webkit2gtk::WebViewExt;

#[doc(alias = "webkit_web_view_can_execute_editing_command")]
pub fn can_execute_editing_command<P: FnOnce(Result<(), Error>) + 'static>(webview: &impl IsA<WebView>, command: &str, cancellable: Option<&impl IsA<Cancellable>>, callback: P) {
    webview.can_execute_editing_command(command, cancellable, callback);
}

#[doc(alias = "webkit_web_view_execute_editing_command")]
pub fn execute_editing_command(webview: &impl IsA<WebView>, command: &str) {
    webview.execute_editing_command(command);
}

#[doc(alias = "webkit_web_view_execute_editing_command_with_argument")]
pub fn execute_editing_command_with_argument(webview: &impl IsA<WebView>, command: &str, argument: &str) {
    webview.execute_editing_command_with_argument(command, argument);
}

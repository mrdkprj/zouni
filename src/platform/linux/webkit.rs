use gtk::glib::IsA;
pub use webkit2gtk::{WebView, WebViewExt};

pub fn to_webview_ext(view: &impl IsA<WebView>) -> &impl WebViewExt {
    view
}

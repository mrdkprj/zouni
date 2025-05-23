use super::util::encode_wide;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Mutex};
use webview2_com::{
    ExecuteScriptCompletedHandler,
    Microsoft::Web::WebView2::Win32::{ICoreWebView2, ICoreWebView2File, ICoreWebView2WebMessageReceivedEventArgs, ICoreWebView2WebMessageReceivedEventArgs2},
    WebMessageReceivedEventHandler,
};
use windows::core::{Interface, PCWSTR, PWSTR};

#[derive(Clone, Serialize, Deserialize)]
pub struct FileDropEvent {
    pub paths: Vec<String>,
}

struct DropHandler {
    token: i64,
    callback: Box<dyn Fn(FileDropEvent) + 'static + Send>,
}

static HANDLERS: Lazy<Mutex<HashMap<isize, DropHandler>>> = Lazy::new(|| Mutex::new(HashMap::new()));

pub fn register_file_drop<F: Fn(FileDropEvent) + 'static + Send>(webview: &ICoreWebView2, target_id: Option<String>, callback: F) -> Result<(), String> {
    let js = if let Some(target) = &target_id {
        format!(
            r#"
                const __nonstd__drop__handler__ = (e) => {{
                    const mached = e.composed ? e.composedPath().some((p) => p.id == "{}") : e.target.id == "{}";
                    if ( mached ) {{
                        e.preventDefault();
                        if (e.dataTransfer && e.dataTransfer.files) {{
                            window.chrome.webview.postMessageWithAdditionalObjects("getPathForFiles", e.dataTransfer.files);
                        }}
                    }}
                }}

                document.removeEventListener("drop", __nonstd__drop__handler__);
                document.addEventListener("drop", __nonstd__drop__handler__);
            "#,
            target.clone(),
            target.clone()
        )
    } else {
        r#"
            const __nonstd__drop__handler__ = (e) => {{
                e.preventDefault();
                if (e.dataTransfer && e.dataTransfer.files) {{
                    window.chrome.webview.postMessageWithAdditionalObjects("getPathForFiles", e.dataTransfer.files);
                }}
            }}

            document.removeEventListener("drop", __nonstd__drop__handler__);
            document.addEventListener("drop", __nonstd__drop__handler__);
        "#
        .to_string()
    };

    unsafe { webview.ExecuteScript(PCWSTR::from_raw(encode_wide(js.clone()).as_ptr()), &ExecuteScriptCompletedHandler::create(Box::new(|_, _| Ok(())))) }.map_err(|e| e.message())?;

    let mut token = 0;
    unsafe { webview.add_WebMessageReceived(&WebMessageReceivedEventHandler::create(Box::new(drop_handler)), &mut token) }.map_err(|e| e.message())?;

    let old_handler = HANDLERS.lock().unwrap().insert(
        webview.as_raw() as _,
        DropHandler {
            token,
            callback: Box::new(callback),
        },
    );

    if let Some(handler) = old_handler {
        unsafe { webview.remove_WebMessageReceived(handler.token) }.map_err(|e| e.message())?;
    }

    Ok(())
}

pub fn clear() {
    let _ = {
        let mut lock = HANDLERS.lock().unwrap();
        std::mem::take(&mut *lock)
    };
}

fn drop_handler(webview: Option<ICoreWebView2>, args: Option<ICoreWebView2WebMessageReceivedEventArgs>) -> windows::core::Result<()> {
    if let Some(args) = args {
        let mut webmessageasstring = PWSTR::null();
        unsafe { args.TryGetWebMessageAsString(&mut webmessageasstring) }?;

        if unsafe { webmessageasstring.to_string().unwrap() } == "getPathForFiles" {
            let args2: ICoreWebView2WebMessageReceivedEventArgs2 = args.cast()?;
            if let Ok(obj) = unsafe { args2.AdditionalObjects() } {
                let mut count = 0;
                let mut paths = Vec::new();
                unsafe { obj.Count(&mut count) }?;
                for i in 0..count {
                    let value = unsafe { obj.GetValueAtIndex(i) }?;
                    if let Ok(file) = value.cast::<ICoreWebView2File>() {
                        let mut path_ptr = PWSTR::null();
                        unsafe { file.Path(&mut path_ptr) }?;
                        let path = unsafe { path_ptr.to_string().unwrap() };
                        paths.push(path);
                    }
                }

                if paths.is_empty() {
                    return Ok(());
                }

                if let Some(webview) = webview {
                    let id: isize = webview.as_raw() as _;
                    if let Some(handler) = HANDLERS.lock().unwrap().get(&id) {
                        (handler.callback)(FileDropEvent {
                            paths,
                        });
                    }
                }
            }
        }
    }

    Ok(())
}

use super::util::encode_wide;
use webview2_com::{
    ExecuteScriptCompletedHandler,
    Microsoft::Web::WebView2::Win32::{
        ICoreWebView2, ICoreWebView2ExecuteScriptCompletedHandler, ICoreWebView2File, ICoreWebView2WebMessageReceivedEventArgs, ICoreWebView2WebMessageReceivedEventArgs2,
        ICoreWebView2WebMessageReceivedEventHandler,
    },
    WebMessageReceivedEventHandler,
};
use windows::core::{Interface, PCWSTR, PWSTR};

pub fn reg_drop(webview: &ICoreWebView2, target_id: Option<String>) {
    let js = if let Some(target) = &target_id {
        format!(
            r#"
                const target = document.getElementById("{}");
                target.addEventListener("drop", (e) => {{
                    e.preventDefault();
                    console.log("here")
                    if (e.dataTransfer && e.dataTransfer.files) {{
                       window.chrome.webview.postMessageWithAdditionalObjects("getPathForFiles", e.dataTransfer.files);
                    }}
                }});
            "#,
            target.clone()
        )
    } else {
        r#"
            document.addEventListener("drop", (e) => {{
                e.preventDefault();
                if (e.dataTransfer && e.dataTransfer.files) {{
                    window.chrome.webview.postMessageWithAdditionalObjects("getPathForFiles", e.dataTransfer.files);
                }}
            }});
        "#
        .to_string()
    };

    let handler: ICoreWebView2ExecuteScriptCompletedHandler = ExecuteScriptCompletedHandler::create(Box::new(|_, _| Ok(())));
    unsafe { webview.ExecuteScript(PCWSTR::from_raw(encode_wide(js).as_ptr()), &handler) }.unwrap();

    let mut token = 0;
    let event_handler: ICoreWebView2WebMessageReceivedEventHandler = WebMessageReceivedEventHandler::create(Box::new(drop_handler));
    unsafe { webview.add_WebMessageReceived(&event_handler, &mut token) }.unwrap();
}

#[derive(serde::Serialize)]
struct File {
    path: String,
}

fn drop_handler(webview: Option<ICoreWebView2>, args: Option<ICoreWebView2WebMessageReceivedEventArgs>) -> windows::core::Result<()> {
    unsafe {
        if let Some(args) = args {
            let mut webmessageasstring = PWSTR::null();
            args.TryGetWebMessageAsString(&mut webmessageasstring)?;

            if webmessageasstring.to_string().unwrap() == "getPathForFiles" {
                let args2: ICoreWebView2WebMessageReceivedEventArgs2 = args.cast()?;
                if let Ok(obj) = args2.AdditionalObjects() {
                    let mut count = 0;
                    let mut files = Vec::new();
                    obj.Count(&mut count)?;
                    for i in 0..count {
                        let value = obj.GetValueAtIndex(i)?;
                        if let Ok(file) = value.cast::<ICoreWebView2File>() {
                            let mut path_ptr = PWSTR::null();
                            file.Path(&mut path_ptr)?;
                            let path = path_ptr.to_string().unwrap();
                            files.push(File {
                                path,
                            });
                        }
                    }

                    if files.is_empty() {
                        return Ok(());
                    }

                    if let Ok(str) = serde_json::to_string(&files) {
                        let json = encode_wide(str);
                        if let Some(webview) = webview {
                            webview.PostWebMessageAsJson(PCWSTR::from_raw(json.as_ptr()))?;
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

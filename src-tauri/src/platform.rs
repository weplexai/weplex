/// Platform-specific utilities: URL opening, macOS traffic lights.

#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    // Strict URL validation: only https:// or http://localhost are allowed
    let is_safe = url.starts_with("https://")
        || url.starts_with("http://localhost")
        || url.starts_with("http://127.0.0.1");
    if !is_safe {
        return Err("Blocked: only https:// and http://localhost URLs are allowed".to_string());
    }
    // Reject URLs containing shell metacharacters (defense in depth)
    if url.chars().any(|c| {
        matches!(
            c,
            '`' | '$' | '|' | ';' | '&' | '\n' | '\r' | '"' | '\'' | '\\' | '<' | '>' | '(' | ')'
        )
    }) {
        return Err("Blocked: URL contains invalid characters".to_string());
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&url)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&url)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        // On Windows, use cmd /C start "" "url" — empty title + quoted URL prevents injection
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &url])
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
mod mac_utils {
    use tauri::Manager;

    unsafe extern "C" {
        fn objc_msgSend(obj: *mut std::ffi::c_void, sel: *mut std::ffi::c_void, ...) -> *mut std::ffi::c_void;
        fn sel_registerName(name: *const u8) -> *mut std::ffi::c_void;
    }

    pub fn set_traffic_lights(app: &tauri::AppHandle, visible: bool) {
        if let Some(window) = app.get_webview_window("main") {
            if let Ok(ns_win) = window.ns_window() {
                unsafe {
                    let ns_win = ns_win as *mut std::ffi::c_void;
                    let sel_button = sel_registerName(b"standardWindowButton:\0".as_ptr());
                    let sel_hidden = sel_registerName(b"setHidden:\0".as_ptr());
                    // 0=close, 1=miniaturize, 2=zoom
                    for i in 0u64..3 {
                        let button: *mut std::ffi::c_void = {
                            type Fn = unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, u64) -> *mut std::ffi::c_void;
                            let f: Fn = std::mem::transmute(objc_msgSend as *const ());
                            f(ns_win, sel_button, i)
                        };
                        if !button.is_null() {
                            type FnBool = unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, bool);
                            let f: FnBool = std::mem::transmute(objc_msgSend as *const ());
                            f(button, sel_hidden, !visible);
                        }
                    }
                }
            }
        }
    }
}

#[tauri::command]
pub fn set_traffic_lights_visible(app: tauri::AppHandle, visible: bool) {
    #[cfg(target_os = "macos")]
    mac_utils::set_traffic_lights(&app, visible);
    #[cfg(not(target_os = "macos"))]
    let _ = (app, visible);
}

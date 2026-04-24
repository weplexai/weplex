//! Browser Plugin — launches browsers in app/debug mode and provides CDP control.
//!
//! Supports Chrome, Firefox, and Edge. Uses Chrome DevTools Protocol (CDP)
//! for tab management, navigation, and extension access.

use serde::Serialize;
use std::process::Command;

#[derive(Debug, Clone, Serialize)]
pub struct BrowserInfo {
    pub name: String,       // "chrome", "firefox", "edge"
    pub display_name: String,
    pub path: String,
    pub available: bool,
}

/// Detect installed browsers on macOS.
pub fn detect_browsers() -> Vec<BrowserInfo> {
    let browsers = vec![
        ("chrome", "Google Chrome", "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"),
        ("firefox", "Firefox", "/Applications/Firefox.app/Contents/MacOS/firefox"),
        ("edge", "Microsoft Edge", "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge"),
    ];

    browsers
        .into_iter()
        .map(|(name, display, path)| BrowserInfo {
            name: name.to_string(),
            display_name: display.to_string(),
            available: std::path::Path::new(path).exists(),
            path: path.to_string(),
        })
        .collect()
}

/// Launch a browser with remote debugging enabled.
pub fn launch_browser(browser: &str, port: u16, url: &str) -> Result<u32, String> {
    let browsers = detect_browsers();
    let info = browsers
        .iter()
        .find(|b| b.name == browser && b.available)
        .ok_or_else(|| format!("Browser '{}' not found or not installed", browser))?;

    let mut cmd = Command::new(&info.path);

    match browser {
        "chrome" | "edge" => {
            cmd.arg(format!("--remote-debugging-port={}", port))
                .arg("--no-first-run")
                .arg("--no-default-browser-check")
                .arg(format!("--app={}", url));
        }
        "firefox" => {
            cmd.arg(format!("--remote-debugging-port={}", port))
                .arg("--new-instance")
                .arg(url);
        }
        _ => return Err(format!("Unsupported browser: {}", browser)),
    }

    let child = cmd
        .spawn()
        .map_err(|e| format!("Failed to launch {}: {}", browser, e))?;

    Ok(child.id())
}

/// Get the next available CDP port (9222-9230).
pub fn next_cdp_port() -> u16 {
    // Simple: try ports 9222-9230, return first available
    for port in 9222..=9230 {
        if std::net::TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return port;
        }
    }
    9222 // fallback
}

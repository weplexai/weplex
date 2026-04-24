use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};
use tauri::Emitter;

const TIMEOUT: Duration = Duration::from_secs(60);

const SUCCESS_HTML: &str = r#"<!DOCTYPE html>
<html>
<head><title>Weplex Auth</title>
<style>body{font-family:system-ui;display:flex;justify-content:center;align-items:center;height:100vh;margin:0;background:#0a0a0f;color:#f9fafb}
.card{text-align:center;padding:40px;border-radius:12px;background:#16161e;border:1px solid #2a2a3a}
h1{font-size:20px;margin-bottom:8px}p{color:#9ca3af;font-size:14px}</style>
</head>
<body><div class="card"><h1>Authentication complete</h1><p>You can close this tab and return to Weplex.</p></div></body>
</html>"#;

const ERROR_HTML: &str = r#"<!DOCTYPE html>
<html>
<head><title>Weplex Auth</title>
<style>body{font-family:system-ui;display:flex;justify-content:center;align-items:center;height:100vh;margin:0;background:#0a0a0f;color:#f9fafb}
.card{text-align:center;padding:40px;border-radius:12px;background:#16161e;border:1px solid #3a2020}
h1{font-size:20px;margin-bottom:8px;color:#ef4444}p{color:#9ca3af;font-size:14px}</style>
</head>
<body><div class="card"><h1>Authentication failed</h1><p>Something went wrong. Please close this tab and try again in Weplex.</p></div></body>
</html>"#;

/// Start a one-shot HTTP server on a dynamic localhost port.
/// Emits `oauth-server-ready` event with the allocated port so the frontend
/// can construct the OAuth URL before opening the browser.
/// Waits for GET /auth/callback?code=XXX&state=YYY, responds with HTML, returns the code.
/// Validates the state parameter matches expected_state to prevent CSRF.
/// Times out after 60 seconds.
#[tauri::command]
pub fn start_oauth_server(app: tauri::AppHandle, expected_state: String) -> Result<String, String> {
    if expected_state.is_empty() {
        return Err("expected_state must not be empty".to_string());
    }
    // Bind to port 0 — OS assigns a free port
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| format!("Bind failed: {}", e))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("Failed to get local addr: {}", e))?
        .port();

    // Notify frontend of the actual port so it can build the OAuth URL
    app.emit("oauth-server-ready", port)
        .map_err(|e| format!("Failed to emit port event: {}", e))?;

    listener
        .set_nonblocking(true)
        .map_err(|e| format!("set_nonblocking failed: {}", e))?;

    let start = Instant::now();

    loop {
        if start.elapsed() > TIMEOUT {
            return Err("OAuth callback timed out (60s)".to_string());
        }

        match listener.accept() {
            Ok((mut stream, _)) => {
                // Read the request (up to 4KB is plenty for a callback URL)
                let mut buf = [0u8; 4096];
                // Brief blocking read on the accepted stream
                stream.set_nonblocking(false).map_err(|e| e.to_string())?;
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .map_err(|e| e.to_string())?;

                let n = stream.read(&mut buf).map_err(|e| e.to_string())?;
                let request = String::from_utf8_lossy(&buf[..n]);

                // Validate request path is exactly /auth/callback
                let first_line = request.lines().next().unwrap_or("");
                let path = first_line.split_whitespace().nth(1).unwrap_or("");
                let path_only = path.split_once('?').map(|(p, _)| p).unwrap_or(path);
                if path_only != "/auth/callback" {
                    // Respond with 404 and keep listening
                    let not_found =
                        "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                    let _ = stream.write_all(not_found.as_bytes());
                    let _ = stream.flush();
                    continue;
                }

                // Check for OAuth error response (user denied access or provider error)
                if let Some(err) = extract_param(&request, "error") {
                    let desc = extract_param(&request, "error_description").unwrap_or_default();
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        ERROR_HTML.len(),
                        ERROR_HTML
                    );
                    let _ = stream.write_all(response.as_bytes());
                    let _ = stream.flush();
                    let msg = format!("OAuth denied: {} {}", err, desc);
                    return Err(msg.trim().to_string());
                }

                // Extract code and state from callback URL
                let code = extract_param(&request, "code");
                let state = extract_param(&request, "state");

                // Always respond with HTML
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    SUCCESS_HTML.len(),
                    SUCCESS_HTML
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();

                // Validate state matches to prevent CSRF
                match state {
                    Some(ref s) if s == &expected_state => {}
                    _ => return Err("OAuth state mismatch — possible CSRF attack".to_string()),
                }

                match code {
                    Some(c) => return Ok(c),
                    None => return Err("No code parameter in callback".to_string()),
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // No connection yet — sleep briefly and retry
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => {
                return Err(format!("Accept failed: {}", e));
            }
        }
    }
}

/// Extract a named query parameter from an HTTP GET request line.
fn extract_param(request: &str, name: &str) -> Option<String> {
    // First line: "GET /auth/callback?code=XXX&state=YYY HTTP/1.1"
    let first_line = request.lines().next()?;
    let path = first_line.split_whitespace().nth(1)?;
    let query = path.split_once('?').map(|(_, q)| q)?;

    let prefix = format!("{}=", name);
    for pair in query.split('&') {
        if let Some(value) = pair.strip_prefix(&prefix) {
            let decoded = urldecode(value);
            if !decoded.is_empty() {
                return Some(decoded);
            }
        }
    }
    None
}

/// Minimal URL decoding (handles %XX sequences).
fn urldecode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                result.push(byte as char);
            }
        } else if c == '+' {
            result.push(' ');
        } else {
            result.push(c);
        }
    }
    result
}

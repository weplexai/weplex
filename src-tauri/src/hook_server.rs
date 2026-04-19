//! Lightweight local HTTP server that receives Claude Code hook events.
//!
//! Hook scripts (PreToolUse, PostToolUse, Stop) POST JSON payloads here.
//! The server resolves session_id from the payload and emits Tauri events
//! to the frontend for real-time UI updates.
//!
//! Security: Bearer token auth required. Token generated at startup,
//! written to ~/.weplex/hook-token (mode 0600).

use serde::{Deserialize, Serialize};
use std::io::Read;
use std::sync::Arc;
use tauri::Emitter;

/// Hook event types matching Claude Code hook lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum HookEventType {
    PreToolUse,
    PostToolUse,
    Stop,
    SubagentStart,
    SubagentStop,
    SessionStart,
}

/// Payload sent by hook scripts to the Weplex HTTP listener.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookEvent {
    /// Hook type (pre_tool_use, post_tool_use, stop)
    pub event_type: HookEventType,
    /// Weplex session ID (resolved by hook script from session-map)
    pub session_id: u32,
    /// Tool name (e.g. "Write", "Edit", "Bash", "Read")
    pub tool_name: Option<String>,
    /// File path affected by the tool (for Write/Edit/Read)
    pub file_path: Option<String>,
    /// Working directory of the Claude session
    pub cwd: Option<String>,
    /// Tool input (truncated for large payloads)
    pub tool_input: Option<String>,
    /// Tool output/result (truncated, PostToolUse only)
    pub tool_output: Option<String>,
    /// Sub-agent type (e.g. "Explore", "Plan", "Bash") for SubagentStart/Stop
    pub agent_type: Option<String>,
    /// Sub-agent unique ID for matching start/stop events
    pub agent_id: Option<String>,
    /// Claude Code session UUID (from SessionStart hook)
    pub claude_session_id: Option<String>,
}

/// Tauri event payload emitted to the frontend.
#[derive(Clone, Serialize)]
pub struct HookEventPayload {
    pub event_type: HookEventType,
    pub session_id: u32,
    pub tool_name: Option<String>,
    pub file_path: Option<String>,
    pub cwd: Option<String>,
    pub tool_input: Option<String>,
    pub tool_output: Option<String>,
    pub agent_type: Option<String>,
    pub agent_id: Option<String>,
    pub claude_session_id: Option<String>,
    pub timestamp: u64,
}

/// Generate a cryptographically random hex token (32 bytes = 64 hex chars).
fn generate_token() -> String {
    use std::fmt::Write;
    let mut bytes = [0u8; 32];
    // Use /dev/urandom on Unix
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        let _ = std::io::Read::read_exact(&mut f, &mut bytes);
    } else {
        // Fallback: use timestamp + pid (less secure but functional)
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = ((seed >> (i % 16 * 8)) & 0xff) as u8;
        }
    }
    let mut hex = String::with_capacity(64);
    for b in &bytes {
        let _ = write!(hex, "{:02x}", b);
    }
    hex
}

/// Start the hook HTTP listener on a random localhost port.
/// Returns the port number. Writes port and auth token to ~/.weplex/.
pub fn start_hook_server(app_handle: tauri::AppHandle) -> Result<u16, String> {
    let server = tiny_http::Server::http("127.0.0.1:0")
        .map_err(|e| format!("Failed to start hook server: {}", e))?;

    let port = server
        .server_addr()
        .to_ip()
        .ok_or("Failed to get server address")?
        .port();

    // Generate auth token and write both port and token
    let token = generate_token();
    write_hook_files(port, &token)?;

    let server = Arc::new(server);
    let expected_token = format!("Bearer {}", token);

    log::info!("hook server listening on 127.0.0.1:{}", port);

    // Spawn listener thread
    let app = app_handle.clone();
    std::thread::spawn(move || {
        run_hook_server(server, app, &expected_token);
    });

    Ok(port)
}

/// Write port and token files with secure permissions (0600).
fn write_hook_files(port: u16, token: &str) -> Result<(), String> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    let weplex_dir = format!("{}/.weplex", home);
    std::fs::create_dir_all(&weplex_dir)
        .map_err(|e| format!("Failed to create ~/.weplex: {}", e))?;

    // Harden directory permissions (owner-only) so other local users
    // cannot list files inside, even if umask is permissive.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&weplex_dir, std::fs::Permissions::from_mode(0o700));
    }

    let port_path = format!("{}/hook-port", weplex_dir);
    let token_path = format!("{}/hook-token", weplex_dir);

    std::fs::write(&port_path, port.to_string())
        .map_err(|e| format!("Failed to write hook-port: {}", e))?;
    std::fs::write(&token_path, token)
        .map_err(|e| format!("Failed to write hook-token: {}", e))?;

    // Set secure permissions (owner-only read/write)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        let _ = std::fs::set_permissions(&port_path, perms.clone());
        let _ = std::fs::set_permissions(&token_path, perms);
    }

    Ok(())
}

/// Clean up hook-port and hook-token files on shutdown.
pub fn cleanup_hook_files() {
    let home = std::env::var("HOME").unwrap_or_default();
    let _ = std::fs::remove_file(format!("{}/.weplex/hook-port", home));
    let _ = std::fs::remove_file(format!("{}/.weplex/hook-token", home));
}

/// Main server loop — processes incoming hook events.
fn run_hook_server(server: Arc<tiny_http::Server>, app: tauri::AppHandle, expected_token: &str) {
    loop {
        let mut request = match server.recv() {
            Ok(req) => req,
            Err(_) => break, // Server shut down
        };

        // Only accept POST /hook
        if request.method() != &tiny_http::Method::Post {
            let response = tiny_http::Response::from_string("Method not allowed")
                .with_status_code(405);
            let _ = request.respond(response);
            continue;
        }

        // Check URL path
        if request.url() != "/hook" {
            let response = tiny_http::Response::from_string("Not found")
                .with_status_code(404);
            let _ = request.respond(response);
            continue;
        }

        // Validate bearer token
        let auth_valid = request
            .headers()
            .iter()
            .any(|h| h.field.equiv("Authorization") && h.value.as_str() == expected_token);

        if !auth_valid {
            let response = tiny_http::Response::from_string("Unauthorized")
                .with_status_code(401);
            let _ = request.respond(response);
            continue;
        }

        // Read body (limit to 64KB to prevent abuse)
        let mut body = String::new();
        let mut reader = request.as_reader().take(65_536);
        if reader.read_to_string(&mut body).is_err() {
            let response = tiny_http::Response::from_string("Bad request")
                .with_status_code(400);
            let _ = request.respond(response);
            continue;
        }

        // Parse hook event
        match serde_json::from_str::<HookEvent>(&body) {
            Ok(event) => {
                let payload = HookEventPayload {
                    event_type: event.event_type.clone(),
                    session_id: event.session_id,
                    tool_name: event.tool_name.clone(),
                    file_path: event.file_path.clone(),
                    cwd: event.cwd.clone(),
                    tool_input: event.tool_input.clone(),
                    tool_output: event.tool_output.clone(),
                    agent_type: event.agent_type.clone(),
                    agent_id: event.agent_id.clone(),
                    claude_session_id: event.claude_session_id.clone(),
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64,
                };

                log::debug!(
                    "hook event: {:?} session={} tool={:?}",
                    event.event_type, event.session_id, event.tool_name
                );

                // Emit to frontend
                let _ = app.emit("hook-event", payload);

                let response = tiny_http::Response::from_string("ok")
                    .with_status_code(200);
                let _ = request.respond(response);
            }
            Err(e) => {
                log::warn!("invalid hook payload: {}", e);
                let response = tiny_http::Response::from_string(format!("Invalid JSON: {}", e))
                    .with_status_code(400);
                let _ = request.respond(response);
            }
        }
    }
}

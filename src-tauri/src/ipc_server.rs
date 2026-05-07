use crate::pty_manager::PtyManager;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::AppHandle;

// ── JSON-RPC helpers ──────────────────────────────────────────────────────

fn rpc_ok(result: serde_json::Value) -> serde_json::Value {
    serde_json::json!({ "result": result })
}

fn rpc_error(code: i32, message: impl std::fmt::Display) -> serde_json::Value {
    serde_json::json!({ "error": { "code": code, "message": message.to_string() } })
}

// ── Socket pool ────────────────────────────────────────────────────────────

struct RunSocket {
    /// Path to the .sock file
    path: PathBuf,
    /// Shutdown signal — set to true to stop the listener loop
    shutdown: Arc<AtomicBool>,
    /// Join handle for the listener thread
    handle: Option<std::thread::JoinHandle<()>>,
}

/// Manages Unix domain sockets for MCP communication.
pub struct IpcSocketPool {
    active: HashMap<String, RunSocket>,
}

impl IpcSocketPool {
    pub fn new() -> Self {
        Self {
            active: HashMap::new(),
        }
    }

    /// Base directory for all IPC sockets.
    pub fn socket_dir() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
        PathBuf::from(format!("{}/.weplex/ipc", home))
    }

    /// Remove leftover .sock files from the IPC directory.
    /// Does not require an instance — safe to call on startup before any sockets are active.
    pub fn cleanup_stale_socket_files() {
        let dir = Self::socket_dir();
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("sock") {
                    let _ = std::fs::remove_file(&path);
                }
            }
        }
    }

    /// Stop all active sockets and clean up stale files.
    ///
    /// **Process-exit only.** Does not join listener threads — relies on the OS
    /// reaping them when the process exits. Joining here can deadlock if a
    /// listener is mid-request (holding the pty mutex). If this is ever called
    /// outside of `RunEvent::Exit`, an orphan listener may race with a fresh
    /// `start_global_socket` and `remove_file` the new socket on its way out.
    pub fn cleanup_for_exit(&mut self) {
        let keys: Vec<String> = self.active.keys().cloned().collect();
        for key in keys {
            if let Some(mut rs) = self.active.remove(&key) {
                rs.shutdown.store(true, Ordering::Relaxed);
                let _ = std::fs::remove_file(&rs.path);
                rs.handle.take();
            }
        }

        Self::cleanup_stale_socket_files();
    }

    /// Start the global IPC socket for MCP v2 cross-session tools.
    /// Available to all Claude sessions via the MCP server.
    pub fn start_global_socket(
        &mut self,
        pty_manager: Arc<Mutex<PtyManager>>,
        app: AppHandle,
    ) -> Result<String, String> {
        let key = "__global__".to_string();
        if self.active.contains_key(&key) {
            return Ok(Self::global_socket_path().to_string_lossy().to_string());
        }

        let dir = Self::socket_dir();
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
        }

        let socket_path = Self::global_socket_path();
        let _ = std::fs::remove_file(&socket_path);

        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_clone = Arc::clone(&shutdown);
        let path_clone = socket_path.clone();

        let handle = std::thread::spawn(move || {
            global_socket_listener(path_clone, pty_manager, app, shutdown_clone);
        });

        self.active.insert(
            key,
            RunSocket {
                path: socket_path.clone(),
                shutdown,
                handle: Some(handle),
            },
        );

        Ok(socket_path.to_string_lossy().to_string())
    }

    /// Path for the global MCP socket.
    pub fn global_socket_path() -> PathBuf {
        Self::socket_dir().join("global.sock")
    }
}

// ── Global socket listener (MCP v2) ──────────────────────────────────────

fn global_socket_listener(
    socket_path: PathBuf,
    pty_manager: Arc<Mutex<PtyManager>>,
    app: AppHandle,
    shutdown: Arc<AtomicBool>,
) {
    let listener = match UnixListener::bind(&socket_path) {
        Ok(l) => l,
        Err(e) => {
            log::error!("IPC failed to bind global socket: {}", e);
            return;
        }
    };

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600));
    }

    listener
        .set_nonblocking(true)
        .unwrap_or_else(|e| log::warn!("IPC non-blocking error: {}", e));

    log::info!("IPC global socket listening on {:?}", socket_path);

    while !shutdown.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                let _ = stream.set_nonblocking(false);
                let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(30)));
                let _ = stream.set_write_timeout(Some(std::time::Duration::from_secs(10)));

                let pty = Arc::clone(&pty_manager);
                let app = app.clone();
                handle_global_connection(stream, &pty, &app);
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(e) => {
                if !shutdown.load(Ordering::Relaxed) {
                    log::error!("IPC global accept error: {}", e);
                }
                break;
            }
        }
    }

    let _ = std::fs::remove_file(&socket_path);
    log::info!("IPC global socket stopped");
}

fn handle_global_connection(
    stream: std::os::unix::net::UnixStream,
    pty_manager: &Arc<Mutex<PtyManager>>,
    app: &AppHandle,
) {
    let reader_stream = match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    };
    let mut writer = stream;
    let reader = BufReader::new(reader_stream);

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let request: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                let resp = rpc_error(-32700, format!("Parse error: {}", e));
                let _ = writeln!(writer, "{}", resp);
                let _ = writer.flush();
                continue;
            }
        };

        let method = request.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let params = request.get("params").cloned().unwrap_or(serde_json::json!({}));

        let response = match method {
            "list_sessions" => handle_list_sessions(pty_manager, app),
            "create_session" => handle_create_session(&params, pty_manager, app),
            "read_output" => handle_read_output(&params, pty_manager),
            "send_input" => handle_send_input(&params, pty_manager),
            "get_context" => handle_get_context(app),
            "log_activity" => handle_log_activity(&params),
            _ => rpc_error(-32601, format!("Unknown method: {}", method)),
        };

        let _ = writeln!(writer, "{}", response);
        let _ = writer.flush();
    }
}

// ── MCP v2 handlers ───────────────────────────────────────────────────────

fn handle_list_sessions(
    pty_manager: &Arc<Mutex<PtyManager>>,
    _app: &AppHandle,
) -> serde_json::Value {
    let mgr = match pty_manager.lock() {
        Ok(m) => m,
        Err(e) => {
            log::error!("IPC pty_manager mutex poisoned: {}", e);
            return rpc_error(-32603, "Internal error: mutex poisoned");
        }
    };
    let ids = mgr.list_session_ids();
    let sessions: Vec<serde_json::Value> = ids
        .iter()
        .map(|&id| {
            serde_json::json!({
                "session_id": id,
                "alive": mgr.is_alive(id),
            })
        })
        .collect();

    serde_json::json!({ "result": { "sessions": sessions } })
}

fn handle_create_session(
    params: &serde_json::Value,
    pty_manager: &Arc<Mutex<PtyManager>>,
    app: &AppHandle,
) -> serde_json::Value {
    let command = params.get("command").and_then(|c| c.as_str()).map(|s| s.to_string());
    let cwd = params.get("cwd").and_then(|c| c.as_str()).map(|s| s.to_string());
    let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("mcp-session");

    // Generate a unique session ID using atomic counter (avoids collision).
    // Starts at 900_000 to leave room for frontend session IDs below that range.
    // Guard against overflow: u32::MAX / 2 sessions would be ~2.1B — practically
    // unreachable in a single process lifetime, but we check to fail loudly
    // rather than silently wrap and collide with old IDs.
    static NEXT_MCP_SESSION_ID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(900_000);
    const MAX_MCP_SESSION_ID: u32 = u32::MAX - 1;
    let session_id = NEXT_MCP_SESSION_ID.fetch_add(1, Ordering::Relaxed);
    if session_id >= MAX_MCP_SESSION_ID {
        return rpc_error(-32603, "Session ID counter exhausted; restart required");
    }

    let mut mgr = match pty_manager.lock() {
        Ok(m) => m,
        Err(e) => {
            log::error!("IPC pty_manager mutex poisoned: {}", e);
            return rpc_error(-32603, "Internal error: mutex poisoned");
        }
    };
    match mgr.create(session_id, 120, 40, command.clone(), cwd.clone(), None, app.clone()) {
        Ok(()) => {
            log::debug!("IPC created session {} via MCP (name: {}, cmd: {:?})", session_id, name, command);
            serde_json::json!({
                "result": {
                    "session_id": session_id,
                    "name": name,
                    "cwd": cwd,
                    "command": command,
                }
            })
        }
        Err(e) => serde_json::json!({
            "error": { "code": -32600, "message": format!("Failed to create session: {}", e) }
        }),
    }
}

fn handle_read_output(
    params: &serde_json::Value,
    pty_manager: &Arc<Mutex<PtyManager>>,
) -> serde_json::Value {
    let session_id = match params.get("session_id").and_then(|s| s.as_u64()) {
        Some(id) => id as u32,
        None => {
            return serde_json::json!({
                "error": { "code": -32602, "message": "Missing param: session_id" }
            });
        }
    };

    let last_n = params.get("last_n").and_then(|n| n.as_u64()).unwrap_or(50) as usize;

    let mgr = match pty_manager.lock() {
        Ok(m) => m,
        Err(e) => {
            log::error!("IPC pty_manager mutex poisoned: {}", e);
            return rpc_error(-32603, "Internal error: mutex poisoned");
        }
    };
    match mgr.read_output(session_id, last_n) {
        Ok(lines) => serde_json::json!({
            "result": { "lines": lines, "count": lines.len() }
        }),
        Err(e) => serde_json::json!({
            "error": { "code": -32602, "message": format!("Session {}: {}", session_id, e) }
        }),
    }
}

fn handle_send_input(
    params: &serde_json::Value,
    pty_manager: &Arc<Mutex<PtyManager>>,
) -> serde_json::Value {
    let session_id = match params.get("session_id").and_then(|s| s.as_u64()) {
        Some(id) => id as u32,
        None => {
            return serde_json::json!({
                "error": { "code": -32602, "message": "Missing param: session_id" }
            });
        }
    };

    let text = match params.get("text").and_then(|t| t.as_str()) {
        Some(t) => t,
        None => {
            return serde_json::json!({
                "error": { "code": -32602, "message": "Missing param: text" }
            });
        }
    };

    // Limit input size to 4KB to prevent flooding
    if text.len() > 4096 {
        return serde_json::json!({
            "error": { "code": -32602, "message": "Input text exceeds 4KB limit" }
        });
    }

    let mut mgr = match pty_manager.lock() {
        Ok(m) => m,
        Err(e) => {
            log::error!("IPC pty_manager mutex poisoned: {}", e);
            return rpc_error(-32603, "Internal error: mutex poisoned");
        }
    };
    match mgr.write(session_id, text) {
        Ok(()) => rpc_ok(serde_json::json!({ "ok": true })),
        Err(e) => serde_json::json!({
            "error": { "code": -32602, "message": format!("Session {}: {}", session_id, e) }
        }),
    }
}

fn handle_get_context(_app: &AppHandle) -> serde_json::Value {
    // Context is primarily frontend state — return basic system info
    // Full context (spaces, sessions, cost) requires frontend store access
    // which is not available from the Rust backend. For now, return what we can.
    let home = std::env::var("HOME").unwrap_or_default();
    serde_json::json!({
        "result": {
            "home": home,
            "platform": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
        }
    })
}

/// Append a session activity note. Encryption + Keychain access happen here
/// (in the Tauri process) so the mcp-server sidecar doesn't need its own
/// keychain ACL.
///
/// Params:
///   session_id (string, required) — Weplex session id from WEPLEX_SESSION_ID
///   profile_id (string, optional) — profile config_dir from WEPLEX_PROFILE_ID,
///                                   defaults to "default"
///   text (string, required) — note body, ≤10KB
///   files_changed (array of string, optional)
///   decisions (array of string, optional)
fn handle_log_activity(params: &serde_json::Value) -> serde_json::Value {
    const MAX_TEXT_LEN: usize = 10 * 1024;

    let session_id = match params.get("session_id").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => return rpc_error(-32602, "Missing or empty param: session_id"),
    };
    // Defence-in-depth: same allowlist as get_session_summary.
    if !session_id
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return rpc_error(-32602, "Invalid session_id format");
    }

    let text = match params.get("text").and_then(|v| v.as_str()) {
        Some(t) => t.to_string(),
        None => return rpc_error(-32602, "Missing param: text"),
    };
    if text.len() > MAX_TEXT_LEN {
        return rpc_error(-32602, format!("text exceeds {}B limit", MAX_TEXT_LEN));
    }

    // profile_id is required so that mcp-server invocations with a missing
    // WEPLEX_PROFILE_ID env fail loudly here rather than silently writing
    // notes under the system profile's key.
    let profile_id = match params.get("profile_id").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => return rpc_error(-32602, "Missing or empty param: profile_id"),
    };

    let files_changed: Vec<String> = params
        .get("files_changed")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let decisions: Vec<String> = params
        .get("decisions")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let note = crate::session_summary::NoteEntry {
        text,
        files_changed,
        decisions,
        at: now,
    };

    match crate::session_summary::append_note(&session_id, &profile_id, note) {
        Ok(()) => rpc_ok(serde_json::json!({ "ok": true, "at": now })),
        Err(e) => rpc_error(-32603, e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// Build a `RunSocket` whose listener thread parks until `shutdown` is set.
    /// Mimics a healthy listener that respects the shutdown flag.
    fn make_cooperative_socket(path: PathBuf) -> RunSocket {
        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_clone = Arc::clone(&shutdown);
        let handle = std::thread::spawn(move || {
            while !shutdown_clone.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(20));
            }
        });
        RunSocket { path, shutdown, handle: Some(handle) }
    }

    /// Build a `RunSocket` with a thread that ignores shutdown for 30s.
    /// Simulates a stuck listener (e.g. holding pty mutex during a request).
    /// Used to prove `cleanup_for_exit` does not block on `join()`.
    fn make_stuck_socket(path: PathBuf) -> RunSocket {
        let shutdown = Arc::new(AtomicBool::new(false));
        let handle = std::thread::spawn(|| {
            std::thread::sleep(Duration::from_secs(30));
        });
        RunSocket { path, shutdown, handle: Some(handle) }
    }

    #[test]
    fn cleanup_for_exit_removes_sock_files() {
        let tmp = std::env::temp_dir().join(format!("weplex_ipc_test_{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let sock_a = tmp.join("a.sock");
        let sock_b = tmp.join("b.sock");
        std::fs::write(&sock_a, "").unwrap();
        std::fs::write(&sock_b, "").unwrap();

        let mut pool = IpcSocketPool::new();
        pool.active.insert("a".into(), make_cooperative_socket(sock_a.clone()));
        pool.active.insert("b".into(), make_cooperative_socket(sock_b.clone()));

        pool.cleanup_for_exit();

        assert!(!sock_a.exists(), "a.sock should be removed");
        assert!(!sock_b.exists(), "b.sock should be removed");
        assert!(pool.active.is_empty(), "pool should be drained");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn cleanup_for_exit_does_not_block_on_stuck_listener() {
        let tmp = std::env::temp_dir().join(format!("weplex_ipc_stuck_{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let sock = tmp.join("stuck.sock");
        std::fs::write(&sock, "").unwrap();

        let mut pool = IpcSocketPool::new();
        pool.active.insert("stuck".into(), make_stuck_socket(sock.clone()));

        let started = Instant::now();
        pool.cleanup_for_exit();
        let elapsed = started.elapsed();

        assert!(
            elapsed < Duration::from_millis(500),
            "cleanup_for_exit should not block on join (took {:?})",
            elapsed
        );
        assert!(!sock.exists());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn cleanup_for_exit_is_idempotent() {
        let mut pool = IpcSocketPool::new();
        pool.cleanup_for_exit();
        pool.cleanup_for_exit();
        assert!(pool.active.is_empty());
    }

    #[test]
    fn cleanup_stale_socket_files_skips_non_sock() {
        let tmp = IpcSocketPool::socket_dir();
        std::fs::create_dir_all(&tmp).unwrap();
        let keep = tmp.join("keep.txt");
        let drop_path = tmp.join("drop_test.sock");
        std::fs::write(&keep, "x").unwrap();
        std::fs::write(&drop_path, "y").unwrap();

        IpcSocketPool::cleanup_stale_socket_files();

        assert!(keep.exists(), "non-.sock files must be preserved");
        assert!(!drop_path.exists(), ".sock files must be removed");
        let _ = std::fs::remove_file(&keep);
    }
}

use crate::pty_manager::PtyManager;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::AppHandle;

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

    /// Stop all active sockets and clean up stale files. Called on app exit.
    pub fn cleanup_all(&mut self) {
        // Stop all active sockets
        let keys: Vec<String> = self.active.keys().cloned().collect();
        for key in keys {
            if let Some(mut rs) = self.active.remove(&key) {
                rs.shutdown.store(true, Ordering::Relaxed);
                let _ = std::fs::remove_file(&rs.path);
                if let Some(handle) = rs.handle.take() {
                    let _ = handle.join();
                }
            }
        }

        // Remove any leftover .sock files
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
            eprintln!("[weplex-ipc] failed to bind global socket: {}", e);
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
        .unwrap_or_else(|e| eprintln!("[weplex-ipc] non-blocking error: {}", e));

    eprintln!("[weplex-ipc] global socket listening on {:?}", socket_path);

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
                    eprintln!("[weplex-ipc] global accept error: {}", e);
                }
                break;
            }
        }
    }

    let _ = std::fs::remove_file(&socket_path);
    eprintln!("[weplex-ipc] global socket stopped");
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
                let resp = serde_json::json!({
                    "error": { "code": -32700, "message": format!("Parse error: {}", e) }
                });
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
            _ => serde_json::json!({
                "error": { "code": -32601, "message": format!("Unknown method: {}", method) }
            }),
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
            eprintln!("[weplex-ipc] pty_manager mutex poisoned: {}", e);
            return serde_json::json!({ "error": { "code": -32603, "message": "Internal error: mutex poisoned" } });
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

    // Generate a unique session ID using atomic counter (avoids collision)
    static NEXT_MCP_SESSION_ID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(900_000);
    let session_id = NEXT_MCP_SESSION_ID.fetch_add(1, Ordering::Relaxed);

    let mut mgr = match pty_manager.lock() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("[weplex-ipc] pty_manager mutex poisoned: {}", e);
            return serde_json::json!({ "error": { "code": -32603, "message": "Internal error: mutex poisoned" } });
        }
    };
    match mgr.create(session_id, 120, 40, command.clone(), cwd.clone(), None, app.clone()) {
        Ok(()) => {
            eprintln!("[weplex-ipc] created session {} via MCP (name: {}, cmd: {:?})", session_id, name, command);
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
            eprintln!("[weplex-ipc] pty_manager mutex poisoned: {}", e);
            return serde_json::json!({ "error": { "code": -32603, "message": "Internal error: mutex poisoned" } });
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
            eprintln!("[weplex-ipc] pty_manager mutex poisoned: {}", e);
            return serde_json::json!({ "error": { "code": -32603, "message": "Internal error: mutex poisoned" } });
        }
    };
    match mgr.write(session_id, text) {
        Ok(()) => serde_json::json!({ "result": { "ok": true } }),
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


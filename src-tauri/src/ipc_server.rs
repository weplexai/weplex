use crate::pipeline_engine::PipelineEngine;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};

// ── Event payloads ─────────────────────────────────────────────────────────

#[derive(Clone, serde::Serialize)]
struct McpStageCompletePayload {
    run_id: String,
    stage_name: String,
    artifact: String,
    status: String,
    error: String,
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

/// Manages a pool of scoped Unix domain sockets, one per pipeline run.
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

    /// Socket path for a specific pipeline run.
    pub fn socket_path_for_run(run_id: &str) -> PathBuf {
        Self::socket_dir().join(format!("run-{}.sock", run_id))
    }

    /// Start a scoped IPC socket for a specific pipeline run.
    /// Returns the socket path as a string.
    pub fn start_run_socket(
        &mut self,
        run_id: String,
        engine: Arc<Mutex<PipelineEngine>>,
        app: AppHandle,
    ) -> Result<String, String> {
        if self.active.contains_key(&run_id) {
            // Already running — return existing path
            return Ok(Self::socket_path_for_run(&run_id)
                .to_string_lossy()
                .to_string());
        }

        let dir = Self::socket_dir();
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

        // Set directory permissions to 0700 (owner-only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o700);
            let _ = std::fs::set_permissions(&dir, perms);
        }

        let socket_path = Self::socket_path_for_run(&run_id);

        // Remove stale socket file if it exists
        let _ = std::fs::remove_file(&socket_path);

        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_clone = Arc::clone(&shutdown);
        let path_clone = socket_path.clone();
        let run_id_clone = run_id.clone();

        let handle = std::thread::spawn(move || {
            run_socket_listener(run_id_clone, path_clone, engine, app, shutdown_clone);
        });

        self.active.insert(
            run_id,
            RunSocket {
                path: socket_path.clone(),
                shutdown,
                handle: Some(handle),
            },
        );

        Ok(socket_path.to_string_lossy().to_string())
    }

    /// Stop and clean up socket for a run.
    pub fn stop_run_socket(&mut self, run_id: &str) {
        if let Some(mut rs) = self.active.remove(run_id) {
            rs.shutdown.store(true, Ordering::Relaxed);
            // Remove socket file to unblock accept()
            let _ = std::fs::remove_file(&rs.path);
            if let Some(handle) = rs.handle.take() {
                let _ = handle.join();
            }
        }
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
        let run_ids: Vec<String> = self.active.keys().cloned().collect();
        for id in run_ids {
            self.stop_run_socket(&id);
        }

        // Remove any leftover .sock files
        Self::cleanup_stale_socket_files();
    }
}

// ── Socket listener ────────────────────────────────────────────────────────

/// Listener loop for a single run's socket. Accepts connections and handles
/// JSON-RPC requests scoped to `run_id`.
fn run_socket_listener(
    run_id: String,
    socket_path: PathBuf,
    engine: Arc<Mutex<PipelineEngine>>,
    app: AppHandle,
    shutdown: Arc<AtomicBool>,
) {
    let listener = match UnixListener::bind(&socket_path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!(
                "[weplex-ipc] failed to bind socket {:?}: {}",
                socket_path, e
            );
            return;
        }
    };

    // Set socket file permissions to 0600 (owner-only)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        let _ = std::fs::set_permissions(&socket_path, perms);
    }

    // Non-blocking accept so we can check the shutdown flag
    listener
        .set_nonblocking(true)
        .unwrap_or_else(|e| eprintln!("[weplex-ipc] failed to set non-blocking: {}", e));

    eprintln!(
        "[weplex-ipc] listening on {:?} for run {}",
        socket_path, run_id
    );

    while !shutdown.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                // Set the stream back to blocking for the connection handler
                let _ = stream.set_nonblocking(false);
                let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(30)));
                let _ = stream.set_write_timeout(Some(std::time::Duration::from_secs(10)));

                let run_id = run_id.clone();
                let engine = Arc::clone(&engine);
                let app = app.clone();

                // Handle connection in the same thread — MCP server sends
                // one request at a time, so serial handling is fine
                handle_connection(stream, &run_id, &engine, &app);
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // No pending connection — sleep briefly and retry
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(e) => {
                if !shutdown.load(Ordering::Relaxed) {
                    eprintln!("[weplex-ipc] accept error: {}", e);
                }
                break;
            }
        }
    }

    // Cleanup: remove socket file
    let _ = std::fs::remove_file(&socket_path);
    eprintln!("[weplex-ipc] listener stopped for run {}", run_id);
}

// ── Connection handler ─────────────────────────────────────────────────────

/// Handle a single MCP server connection. Reads JSON Lines, processes each
/// request, and writes JSON Lines responses.
fn handle_connection(
    stream: std::os::unix::net::UnixStream,
    run_id: &str,
    engine: &Arc<Mutex<PipelineEngine>>,
    app: &AppHandle,
) {
    let reader_stream = match stream.try_clone() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[weplex-ipc] failed to clone stream: {}", e);
            return;
        }
    };
    let mut writer = stream;
    let reader = BufReader::new(reader_stream);

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("[weplex-ipc] read error: {}", e);
                break;
            }
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let request: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[weplex-ipc] invalid JSON: {}", e);
                let resp = serde_json::json!({
                    "error": { "code": -32700, "message": format!("Parse error: {}", e) }
                });
                let _ = writeln!(writer, "{}", resp);
                let _ = writer.flush();
                continue;
            }
        };

        let method = request
            .get("method")
            .and_then(|m| m.as_str())
            .unwrap_or("");
        let params = request.get("params").cloned().unwrap_or(serde_json::json!({}));

        // Validate run_id scope — reject cross-run access
        let request_run_id = params.get("run_id").and_then(|r| r.as_str()).unwrap_or("");
        if !request_run_id.is_empty() && request_run_id != run_id {
            let resp = serde_json::json!({
                "error": {
                    "code": -32600,
                    "message": format!("Access denied: this socket serves run '{}', not '{}'", run_id, request_run_id)
                }
            });
            let _ = writeln!(writer, "{}", resp);
            let _ = writer.flush();
            continue;
        }

        let response = match method {
            "stage_complete" => handle_stage_complete(run_id, &params, engine, app),
            "get_artifact" => handle_get_artifact(run_id, &params, engine),
            "pipeline_info" => handle_pipeline_info(run_id, engine),
            _ => serde_json::json!({
                "error": { "code": -32601, "message": format!("Unknown method: {}", method) }
            }),
        };

        let _ = writeln!(writer, "{}", response);
        let _ = writer.flush();
    }
}

// ── IPC method handlers ────────────────────────────────────────────────────

fn handle_stage_complete(
    run_id: &str,
    params: &serde_json::Value,
    engine: &Arc<Mutex<PipelineEngine>>,
    app: &AppHandle,
) -> serde_json::Value {
    let stage_name = match params.get("stage_name").and_then(|s| s.as_str()) {
        Some(s) => s,
        None => {
            return serde_json::json!({
                "error": { "code": -32602, "message": "Missing param: stage_name" }
            })
        }
    };

    let artifact = match params.get("artifact").and_then(|a| a.as_str()) {
        Some(a) => a,
        None => {
            return serde_json::json!({
                "error": { "code": -32602, "message": "Missing param: artifact" }
            })
        }
    };

    let status = params
        .get("status")
        .and_then(|s| s.as_str())
        .unwrap_or("success")
        .to_string();
    let error = params
        .get("error")
        .and_then(|e| e.as_str())
        .unwrap_or("")
        .to_string();

    // Store artifact in PipelineEngine
    {
        let mut eng = engine
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        eng.set_mcp_artifact(run_id, stage_name, artifact);
    }

    // Emit Tauri event so the frontend knows
    let _ = app.emit(
        "mcp-stage-complete",
        McpStageCompletePayload {
            run_id: run_id.to_string(),
            stage_name: stage_name.to_string(),
            artifact: artifact.to_string(),
            status,
            error,
        },
    );

    serde_json::json!({
        "result": { "ok": true }
    })
}

fn handle_get_artifact(
    run_id: &str,
    params: &serde_json::Value,
    engine: &Arc<Mutex<PipelineEngine>>,
) -> serde_json::Value {
    let stage_name = match params.get("stage_name").and_then(|s| s.as_str()) {
        Some(s) => s,
        None => {
            return serde_json::json!({
                "error": { "code": -32602, "message": "Missing param: stage_name" }
            })
        }
    };

    let eng = engine
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    // Check MCP artifacts first (set via deck_stage_complete)
    if let Some(artifact) = eng.get_mcp_artifact(run_id, stage_name) {
        return serde_json::json!({
            "result": { "artifact": artifact }
        });
    }

    // Fallback: check pipeline engine stage artifacts (from stdout capture)
    if let Some(artifact) = eng.get_artifact(run_id, stage_name) {
        return serde_json::json!({
            "result": { "artifact": artifact }
        });
    }

    // Not found locally — for MVP, return error
    // (collaborative fallback via server API can be added later)
    serde_json::json!({
        "error": {
            "code": -32602,
            "message": format!("Artifact not found for stage '{}'", stage_name)
        }
    })
}

fn handle_pipeline_info(
    run_id: &str,
    engine: &Arc<Mutex<PipelineEngine>>,
) -> serde_json::Value {
    let eng = engine
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    match eng.get_run(run_id) {
        Some(run) => serde_json::json!({
            "result": {
                "run_id": run.id,
                "pipeline_name": run.pipeline_name,
                "task": run.task,
                "status": run.status,
                "stages": run.stages,
                "started_at": run.started_at,
                "finished_at": run.finished_at
            }
        }),
        None => serde_json::json!({
            "error": {
                "code": -32602,
                "message": format!("Run '{}' not found", run_id)
            }
        }),
    }
}

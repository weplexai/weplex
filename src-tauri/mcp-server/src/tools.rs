use crate::ipc_client::IpcClient;
use serde_json::Value;
use std::path::PathBuf;
use weplex_mcp_contract::{
    IPC_METHOD_CREATE_SESSION, IPC_METHOD_GET_CONTEXT, IPC_METHOD_LIST_SESSIONS,
    IPC_METHOD_READ_OUTPUT, IPC_METHOD_SEND_INPUT, TOOL_CREATE_SESSION, TOOL_GET_CONTEXT,
    TOOL_LIST_SESSIONS, TOOL_LOG_ACTIVITY, TOOL_READ_OUTPUT, TOOL_SEND_INPUT,
};

// ── Constants ─────────────────────────────────────────────────────────────

/// Maximum summary size in bytes (10KB)
const MAX_SUMMARY_SIZE: usize = 10 * 1024;

// ── Tool definitions ───────────────────────────────────────────────────────

/// Activity journal tool definition — available in all contexts.
fn log_activity_tool() -> Value {
    serde_json::json!({
        "name": TOOL_LOG_ACTIVITY,
        "description": "Personal session journal for future you. Each call appends one entry (short summary of what you accomplished, files touched, key decisions). Entries are stored locally per Weplex session and are private — nothing is shared automatically. Future you (or another Claude run in the same Weplex session) can re-read them to pick up context. Call at natural breakpoints, when finishing a task, or before stopping work.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "summary": {
                    "type": "string",
                    "description": "1-3 sentence summary of what was accomplished in this step"
                },
                "filesChanged": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "File paths that were modified in this step"
                },
                "decisions": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Key technical decisions made in this step"
                }
            },
            "required": ["summary"]
        }
    })
}

/// V2 cross-session tool definitions — available when global socket is connected.
fn v2_tools() -> Vec<Value> {
    vec![
        serde_json::json!({
            "name": TOOL_LIST_SESSIONS,
            "description": "List all active terminal sessions in Weplex. Returns session IDs and their status (alive/dead). Use this to discover what sessions are running before reading output or sending input.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        }),
        serde_json::json!({
            "name": TOOL_CREATE_SESSION,
            "description": "Create a new terminal session in Weplex that runs the specified command. Use for test runners, build watchers, long-running servers, SSH sessions, REPLs, or any user-level process.\n\nDO NOT use this to spawn headless LLM sessions (e.g. `claude -p`, `opencode -p`, `gemini -p`, `aider --message`). Most LLM subscription plans forbid driving the model programmatically, and this tool is not a way around that. If you need to delegate a task to another AI agent, use Claude's built-in Task / sub-agent tool instead — that's the sanctioned mechanism.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "Command to run (e.g. 'npm test', 'cargo build', 'pnpm dev', 'ssh user@host'). If omitted, opens a shell. Do not pass headless LLM invocations here."
                    },
                    "cwd": {
                        "type": "string",
                        "description": "Working directory for the session. Defaults to home directory."
                    },
                    "name": {
                        "type": "string",
                        "description": "Display name for the session in the sidebar."
                    }
                }
            }
        }),
        serde_json::json!({
            "name": TOOL_READ_OUTPUT,
            "description": "Read recent terminal output from another session. Returns the last N lines from the session's output buffer. Use this to check test results, build output, server logs, or the state of long-running processes.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": {
                        "type": "number",
                        "description": "ID of the session to read from (get IDs from weplex_list_sessions)."
                    },
                    "last_n": {
                        "type": "number",
                        "description": "Number of lines to read (default: 50, max: 500)."
                    }
                },
                "required": ["session_id"]
            }
        }),
        serde_json::json!({
            "name": TOOL_SEND_INPUT,
            "description": "Send text input to another session's terminal. The text is written to the session's PTY as if the user typed it. Use this to interact with running processes: answer yes/no prompts, press Enter to continue, send REPL expressions, drive SSH sessions.\n\nDO NOT use this to type prompts into another LLM session (e.g. pasting a question into a running `claude` conversation). That's effectively driving the LLM programmatically, which violates most subscription plans' terms. For agent-to-agent handoff, use Claude's built-in Task / sub-agent tool.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": {
                        "type": "number",
                        "description": "ID of the session to send input to."
                    },
                    "text": {
                        "type": "string",
                        "description": "Text to send (include \\n for Enter key). Should be process control input (y/n, commands, REPL expressions), not an LLM prompt."
                    }
                },
                "required": ["session_id", "text"]
            }
        }),
        serde_json::json!({
            "name": TOOL_GET_CONTEXT,
            "description": "Get information about the Weplex workspace: platform, home directory, and system info. Use this for workspace awareness.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        }),
    ]
}

/// Return the list of available MCP tools.
/// Tools available depend on context:
/// - Always: weplex_log_activity
/// - With global socket: v2 cross-session tools (list/create/read/send/context)
pub fn list_tools(global_socket_path: &str) -> Value {
    let mut tools = vec![log_activity_tool()];

    // V2 tools available when global socket exists
    if !global_socket_path.is_empty() {
        tools.extend(v2_tools());
    }

    serde_json::json!({ "tools": tools })
}

// ── Tool dispatch ──────────────────────────────────────────────────────────

/// Dispatch a tools/call request to the appropriate handler.
pub fn call_tool(
    tool_name: &str,
    arguments: &Value,
    session_id: &str,
    global_ipc: &mut Option<IpcClient>,
) -> Result<Value, String> {
    match tool_name {
        // Notes (always available)
        name if name == TOOL_LOG_ACTIVITY => handle_log_activity(arguments, session_id),
        // Cross-session tools (v2) — require global socket
        name if name == TOOL_LIST_SESSIONS
            || name == TOOL_CREATE_SESSION
            || name == TOOL_READ_OUTPUT
            || name == TOOL_SEND_INPUT
            || name == TOOL_GET_CONTEXT =>
        {
            let gipc = global_ipc.as_mut().ok_or(
                "Cross-session tools require Weplex to be running. Global MCP socket not available."
                    .to_string(),
            )?;
            handle_v2_tool(tool_name, arguments, gipc)
        }
        _ => Err(format!("Unknown tool: {}", tool_name)),
    }
}

/// Handle MCP v2 cross-session tools by forwarding to global socket.
fn handle_v2_tool(
    tool_name: &str,
    arguments: &Value,
    ipc: &mut IpcClient,
) -> Result<Value, String> {
    // Map tool name to IPC method
    let method = match tool_name {
        name if name == TOOL_LIST_SESSIONS => IPC_METHOD_LIST_SESSIONS,
        name if name == TOOL_CREATE_SESSION => IPC_METHOD_CREATE_SESSION,
        name if name == TOOL_READ_OUTPUT => IPC_METHOD_READ_OUTPUT,
        name if name == TOOL_SEND_INPUT => IPC_METHOD_SEND_INPUT,
        name if name == TOOL_GET_CONTEXT => IPC_METHOD_GET_CONTEXT,
        _ => return Err(format!("Unknown v2 tool: {}", tool_name)),
    };

    let request = serde_json::json!({
        "method": method,
        "params": arguments,
    });

    let response = ipc.send(request)?;

    if let Some(err) = response.get("error") {
        return Err(err
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("Unknown error")
            .to_string());
    }

    let result = response.get("result").cloned().unwrap_or(serde_json::json!({}));

    Ok(serde_json::json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&result).unwrap_or_default()
        }]
    }))
}

// ── Activity notes (file-based, no IPC) ───────────────────────────────────

/// Return the path to ~/.weplex/activity/
fn activity_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home).join(".weplex/activity")
}

/// Atomically write `contents` to `path` with mode 0600 via tmp+rename.
/// See `weplex/utils.rs::atomic_write_owner_only` for the long-form
/// justification; this is a crate-local copy because `mcp-server` can't
/// depend on the main Tauri crate. PID-suffixed tmp + `create_new`
/// defends against symlink races.
fn atomic_write_owner_only(path: &std::path::Path, contents: &str) -> Result<(), String> {
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "tmp".to_string());
    let parent = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let tmp_path = parent.join(format!("{}.{}.tmp", file_name, std::process::id()));
    let _ = std::fs::remove_file(&tmp_path);

    #[cfg(unix)]
    let write_result = {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp_path)
            .and_then(|mut f| f.write_all(contents.as_bytes()).and_then(|_| f.sync_all()))
    };
    #[cfg(not(unix))]
    let write_result = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp_path)
        .and_then(|mut f| std::io::Write::write_all(&mut f, contents.as_bytes()));

    if let Err(e) = write_result {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(format!("atomic_write: failed to write tmp: {}", e));
    }

    if let Err(e) = std::fs::rename(&tmp_path, path) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(format!("atomic_write: failed to rename: {}", e));
    }

    Ok(())
}

/// Append an activity note to the session summary file.
/// Reads existing file, adds a new NoteEntry to the `notes` array, writes back.
fn handle_log_activity(arguments: &Value, session_id: &str) -> Result<Value, String> {
    if session_id.is_empty() {
        return Err("WEPLEX_SESSION_ID not set — cannot save notes".to_string());
    }

    let summary = arguments
        .get("summary")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing required argument: summary".to_string())?;

    // Enforce 10KB limit on the note text
    if summary.len() > MAX_SUMMARY_SIZE {
        return Err(format!(
            "Note text exceeds {}KB limit ({} bytes)",
            MAX_SUMMARY_SIZE / 1024,
            summary.len()
        ));
    }

    let files_changed: Vec<String> = arguments
        .get("filesChanged")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let decisions: Vec<String> = arguments
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

    // Build the new note entry
    let new_note = serde_json::json!({
        "text": summary,
        "filesChanged": files_changed,
        "decisions": decisions,
        "at": now
    });

    // Validate session_id to prevent path traversal
    if session_id.is_empty()
        || !session_id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err("Invalid session ID format".to_string());
    }

    let dir = activity_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create activity dir: {}", e))?;

    let path = dir.join(format!("{}.json", session_id));

    // Read existing file or start fresh. Log when existing data is corrupt
    // (we recover by resetting, but the user should know we dropped notes).
    let mut payload: Value = match std::fs::read_to_string(&path) {
        Ok(content) => match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(e) => {
                eprintln!(
                    "[weplex-mcp] session summary {} corrupt, resetting: {}",
                    session_id, e
                );
                serde_json::json!({})
            }
        },
        Err(_) => serde_json::json!({}),
    };

    // Ensure notes array exists and append (max 200 entries to prevent unbounded growth)
    if !payload.get("notes").map(|v| v.is_array()).unwrap_or(false) {
        payload["notes"] = serde_json::json!([]);
    }
    let notes = payload["notes"].as_array_mut().unwrap();
    if notes.len() >= 200 {
        notes.remove(0); // Drop oldest to stay within limit
    }
    notes.push(new_note);

    // Update top-level fields for hook freshness check and backward compat
    payload["updatedAt"] = serde_json::json!(now);
    payload["summary"] = serde_json::json!(summary);
    payload["filesChanged"] = serde_json::json!(files_changed);
    payload["decisions"] = serde_json::json!(decisions);

    let content = serde_json::to_string_pretty(&payload)
        .map_err(|e| format!("Failed to serialize notes: {}", e))?;

    atomic_write_owner_only(&path, &content)?;

    let note_count = payload["notes"].as_array().map(|a| a.len()).unwrap_or(0);

    eprintln!(
        "[weplex-mcp] appended note #{} for session {} ({} bytes)",
        note_count, session_id, content.len()
    );

    Ok(serde_json::json!({
        "content": [{
            "type": "text",
            "text": format!(
                "Activity note #{} recorded. Future you can read it back from this session's timeline.",
                note_count
            )
        }]
    }))
}

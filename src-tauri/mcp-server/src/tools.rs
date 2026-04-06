use crate::ipc_client::IpcClient;
use serde_json::Value;
use std::path::PathBuf;

// ── Constants ─────────────────────────────────────────────────────────────

/// Maximum summary size in bytes (10KB)
const MAX_SUMMARY_SIZE: usize = 10 * 1024;

// ── Tool definitions ───────────────────────────────────────────────────────

/// Activity notes tool definition — available in all contexts.
fn update_notes_tool() -> Value {
    serde_json::json!({
        "name": "deck_update_notes",
        "description": "Record what you accomplished. Notes are appended chronologically and visible to your team in real time. Call at natural breakpoints, when finishing a task, or before stopping work.",
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
            "name": "deck_list_sessions",
            "description": "List all active terminal sessions in Weplex. Returns session IDs and their status (alive/dead). Use this to discover what sessions are running before reading output or sending input.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        }),
        serde_json::json!({
            "name": "deck_create_session",
            "description": "Create a new terminal session in Weplex. The session appears in the sidebar and runs the specified command. Use this to spawn parallel workers, test runners, or build watchers.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "Command to run (e.g. 'npm test', 'cargo build', 'claude -p \"review this\"'). If omitted, opens a shell."
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
            "name": "deck_read_output",
            "description": "Read recent terminal output from another session. Returns the last N lines from the session's output buffer. Use this to check test results, build output, or what another agent is doing.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": {
                        "type": "number",
                        "description": "ID of the session to read from (get IDs from deck_list_sessions)."
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
            "name": "deck_send_input",
            "description": "Send text input to another session's terminal. The text is written to the session's PTY as if the user typed it. Use this to interact with running processes, answer prompts, or send commands.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": {
                        "type": "number",
                        "description": "ID of the session to send input to."
                    },
                    "text": {
                        "type": "string",
                        "description": "Text to send (include \\n for Enter key)."
                    }
                },
                "required": ["session_id", "text"]
            }
        }),
        serde_json::json!({
            "name": "deck_get_context",
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
/// - Always: deck_update_notes
/// - With global socket: v2 cross-session tools (list/create/read/send/context)
/// - With pipeline socket: pipeline tools (stage_complete/get_artifact/pipeline_info)
pub fn list_tools(socket_path: &str, global_socket_path: &str) -> Value {
    let mut tools = vec![update_notes_tool()];

    // V2 tools available when global socket exists
    if !global_socket_path.is_empty() {
        tools.extend(v2_tools());
    }

    // Pipeline tools available when in pipeline context
    if !socket_path.is_empty() {
        tools.push(serde_json::json!({
            "name": "deck_stage_complete",
            "description": "Signal that the current pipeline stage is complete. Provide a structured summary of what was accomplished. This artifact will be passed as context to dependent stages and team members.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "artifact": {
                        "type": "string",
                        "description": "Structured summary of what this stage accomplished. Include: key decisions, files changed, important code snippets, and handoff notes for the next stage. Max 512KB."
                    }
                },
                "required": ["artifact"]
            }
        }));
        tools.push(serde_json::json!({
            "name": "deck_get_artifact",
            "description": "Retrieve the artifact from a previously completed pipeline stage. Use this to understand context and decisions from upstream stages.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "stage_name": {
                        "type": "string",
                        "description": "Name of the completed stage whose artifact to retrieve."
                    }
                },
                "required": ["stage_name"]
            }
        }));
        tools.push(serde_json::json!({
            "name": "deck_pipeline_info",
            "description": "Get information about the current pipeline run: name, task description, all stages with their statuses, and which stage you are currently executing.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        }));
    }

    serde_json::json!({ "tools": tools })
}

// ── Tool dispatch ──────────────────────────────────────────────────────────

/// Dispatch a tools/call request to the appropriate handler.
pub fn call_tool(
    tool_name: &str,
    arguments: &Value,
    run_id: &str,
    stage_name: &str,
    session_id: &str,
    ipc: &mut IpcClient,
    global_ipc: &mut Option<IpcClient>,
) -> Result<Value, String> {
    match tool_name {
        // Pipeline tools (v1)
        "deck_stage_complete" => handle_stage_complete(arguments, run_id, stage_name, ipc),
        "deck_get_artifact" => handle_get_artifact(arguments, run_id, ipc),
        "deck_pipeline_info" => handle_pipeline_info(run_id, ipc),
        // Notes (always available)
        "deck_update_notes" | "deck_session_summary" => handle_update_notes(arguments, session_id),
        // Cross-session tools (v2) — require global socket
        "deck_list_sessions" | "deck_create_session" | "deck_read_output"
        | "deck_send_input" | "deck_get_context" => {
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
        "deck_list_sessions" => "list_sessions",
        "deck_create_session" => "create_session",
        "deck_read_output" => "read_output",
        "deck_send_input" => "send_input",
        "deck_get_context" => "get_context",
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

// ── Tool handlers ──────────────────────────────────────────────────────────

fn handle_stage_complete(
    arguments: &Value,
    run_id: &str,
    stage_name: &str,
    ipc: &mut IpcClient,
) -> Result<Value, String> {
    let artifact = arguments
        .get("artifact")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing required argument: artifact".to_string())?;

    // Enforce 512KB limit
    if artifact.len() > 512 * 1024 {
        return Err("Artifact exceeds 512KB limit".to_string());
    }

    let request = serde_json::json!({
        "method": "stage_complete",
        "params": {
            "run_id": run_id,
            "stage_name": stage_name,
            "artifact": artifact,
            "status": "success"
        }
    });

    let response = ipc.send(request)?;

    if let Some(err) = response.get("error") {
        return Err(err
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("Unknown IPC error")
            .to_string());
    }

    Ok(serde_json::json!({
        "content": [{
            "type": "text",
            "text": format!("Stage '{}' marked as complete. Artifact stored ({} bytes).", stage_name, artifact.len())
        }]
    }))
}

fn handle_get_artifact(
    arguments: &Value,
    run_id: &str,
    ipc: &mut IpcClient,
) -> Result<Value, String> {
    let target_stage = arguments
        .get("stage_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing required argument: stage_name".to_string())?;

    let request = serde_json::json!({
        "method": "get_artifact",
        "params": {
            "run_id": run_id,
            "stage_name": target_stage
        }
    });

    let response = ipc.send(request)?;

    if let Some(err) = response.get("error") {
        return Err(err
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("Unknown IPC error")
            .to_string());
    }

    let artifact = response
        .get("result")
        .and_then(|r| r.get("artifact"))
        .and_then(|a| a.as_str())
        .unwrap_or("");

    Ok(serde_json::json!({
        "content": [{
            "type": "text",
            "text": artifact
        }]
    }))
}

fn handle_pipeline_info(run_id: &str, ipc: &mut IpcClient) -> Result<Value, String> {
    let request = serde_json::json!({
        "method": "pipeline_info",
        "params": {
            "run_id": run_id
        }
    });

    let response = ipc.send(request)?;

    if let Some(err) = response.get("error") {
        return Err(err
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("Unknown IPC error")
            .to_string());
    }

    let info = response.get("result").cloned().unwrap_or(serde_json::json!({}));

    Ok(serde_json::json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&info).unwrap_or_default()
        }]
    }))
}

// ── Activity notes (file-based, no IPC) ───────────────────────────────────

/// Return the path to ~/.weplex/summaries/
fn summaries_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home).join(".weplex/summaries")
}

/// Append an activity note to the session summary file.
/// Reads existing file, adds a new NoteEntry to the `notes` array, writes back.
fn handle_update_notes(arguments: &Value, session_id: &str) -> Result<Value, String> {
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

    let dir = summaries_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create summaries dir: {}", e))?;

    let path = dir.join(format!("{}.json", session_id));

    // Read existing file or start fresh
    let mut payload: Value = if let Ok(content) = std::fs::read_to_string(&path) {
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
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

    std::fs::write(&path, &content)
        .map_err(|e| format!("Failed to write summary file: {}", e))?;

    let note_count = payload["notes"].as_array().map(|a| a.len()).unwrap_or(0);

    eprintln!(
        "[weplex-mcp] appended note #{} for session {} ({} bytes)",
        note_count, session_id, content.len()
    );

    Ok(serde_json::json!({
        "content": [{
            "type": "text",
            "text": format!(
                "Activity note #{} saved. Team members will see this in real time.",
                note_count
            )
        }]
    }))
}

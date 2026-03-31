use crate::ipc_client::IpcClient;
use serde_json::Value;
use std::path::PathBuf;

// ── Constants ─────────────────────────────────────────────────────────────

/// Maximum summary size in bytes (10KB)
const MAX_SUMMARY_SIZE: usize = 10 * 1024;

// ── Tool definitions ───────────────────────────────────────────────────────

/// Session summary tool definition — available in all contexts.
fn session_summary_tool() -> Value {
    serde_json::json!({
        "name": "deck_session_summary",
        "description": "Save a summary of what you accomplished in this session. Your team members will see this context even when you're offline. Call when finishing work or at natural breakpoints.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "summary": {
                    "type": "string",
                    "description": "1-3 sentence summary of what was accomplished"
                },
                "filesChanged": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "File paths that were modified"
                },
                "decisions": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Key technical decisions made"
                }
            },
            "required": ["summary"]
        }
    })
}

/// Return the list of available MCP tools.
/// When `socket_path` is empty (no pipeline context), returns only `deck_session_summary`.
/// When in a pipeline context, returns all pipeline tools + `deck_session_summary`.
pub fn list_tools(socket_path: &str) -> Value {
    if socket_path.is_empty() {
        return serde_json::json!({ "tools": [session_summary_tool()] });
    }

    serde_json::json!({
        "tools": [
            {
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
            },
            {
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
            },
            {
                "name": "deck_pipeline_info",
                "description": "Get information about the current pipeline run: name, task description, all stages with their statuses, and which stage you are currently executing.",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            },
            session_summary_tool()
        ]
    })
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
) -> Result<Value, String> {
    match tool_name {
        "deck_stage_complete" => handle_stage_complete(arguments, run_id, stage_name, ipc),
        "deck_get_artifact" => handle_get_artifact(arguments, run_id, ipc),
        "deck_pipeline_info" => handle_pipeline_info(run_id, ipc),
        "deck_session_summary" => handle_session_summary(arguments, session_id),
        _ => Err(format!("Unknown tool: {}", tool_name)),
    }
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

// ── Session summary (file-based, no IPC) ──────────────────────────────────

/// Return the path to ~/.weplex/summaries/
fn summaries_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home).join(".weplex/summaries")
}

fn handle_session_summary(arguments: &Value, session_id: &str) -> Result<Value, String> {
    if session_id.is_empty() {
        return Err("WEPLEX_SESSION_ID not set — cannot save summary".to_string());
    }

    let summary = arguments
        .get("summary")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing required argument: summary".to_string())?;

    // Enforce 10KB limit on the summary text
    if summary.len() > MAX_SUMMARY_SIZE {
        return Err(format!(
            "Summary exceeds {}KB limit ({} bytes)",
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

    let updated_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let payload = serde_json::json!({
        "summary": summary,
        "filesChanged": files_changed,
        "decisions": decisions,
        "updatedAt": updated_at
    });

    let dir = summaries_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create summaries dir: {}", e))?;

    let path = dir.join(format!("{}.json", session_id));
    let content = serde_json::to_string_pretty(&payload)
        .map_err(|e| format!("Failed to serialize summary: {}", e))?;

    std::fs::write(&path, &content)
        .map_err(|e| format!("Failed to write summary file: {}", e))?;

    eprintln!(
        "[weplex-mcp] saved session summary for {} ({} bytes)",
        session_id,
        content.len()
    );

    Ok(serde_json::json!({
        "content": [{
            "type": "text",
            "text": format!(
                "Session summary saved ({} bytes). Team members will see this context.",
                content.len()
            )
        }]
    }))
}

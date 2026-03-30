use crate::ipc_client::IpcClient;
use serde_json::Value;

// ── Tool definitions ───────────────────────────────────────────────────────

/// Return the list of available MCP tools.
/// If `socket_path` is empty (not in a pipeline), returns an empty array.
pub fn list_tools(socket_path: &str) -> Value {
    if socket_path.is_empty() {
        return serde_json::json!({ "tools": [] });
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
            }
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
    ipc: &mut IpcClient,
) -> Result<Value, String> {
    match tool_name {
        "deck_stage_complete" => handle_stage_complete(arguments, run_id, stage_name, ipc),
        "deck_get_artifact" => handle_get_artifact(arguments, run_id, ipc),
        "deck_pipeline_info" => handle_pipeline_info(run_id, ipc),
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

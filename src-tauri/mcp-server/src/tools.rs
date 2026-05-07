use crate::ipc_client::IpcClient;
use serde_json::Value;
use weplex_mcp_contract::{
    IPC_METHOD_CREATE_SESSION, IPC_METHOD_GET_CONTEXT, IPC_METHOD_LIST_SESSIONS,
    IPC_METHOD_LOG_ACTIVITY, IPC_METHOD_READ_OUTPUT, IPC_METHOD_SEND_INPUT, TOOL_CREATE_SESSION,
    TOOL_GET_CONTEXT, TOOL_LIST_SESSIONS, TOOL_LOG_ACTIVITY, TOOL_READ_OUTPUT, TOOL_SEND_INPUT,
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
        // Notes — now goes through Tauri IPC so encryption + Keychain access
        // happens in one trusted process. Requires Weplex running.
        name if name == TOOL_LOG_ACTIVITY => {
            let gipc = global_ipc.as_mut().ok_or(
                "weplex_log_activity requires Weplex to be running (global socket not available)."
                    .to_string(),
            )?;
            handle_log_activity(arguments, session_id, gipc)
        }
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

// ── Activity notes (delegated to Tauri over IPC) ───────────────────────────
//
// Encryption + Keychain access live in the main Tauri process. The sidecar
// just validates and forwards. This is intentional: only one binary needs
// keychain ACL and the cipher; if the user runs Claude without Weplex, notes
// fail loudly instead of silently writing plaintext. The trade-off was made
// explicitly — see CLAUDE.md / mcp-contract docs for the full rationale.

/// Append an activity note by forwarding to Tauri IPC `log_activity`.
fn handle_log_activity(
    arguments: &Value,
    session_id: &str,
    ipc: &mut IpcClient,
) -> Result<Value, String> {
    if session_id.is_empty()
        || !session_id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err("Invalid or missing WEPLEX_SESSION_ID".to_string());
    }

    let summary = arguments
        .get("summary")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing required argument: summary".to_string())?;
    if summary.len() > MAX_SUMMARY_SIZE {
        return Err(format!(
            "Note text exceeds {}KB limit ({} bytes)",
            MAX_SUMMARY_SIZE / 1024,
            summary.len()
        ));
    }

    // Profile id comes from WEPLEX_PROFILE_ID, set by TerminalView when it
    // spawns the PTY. We accept "default" for the system profile but refuse
    // an empty / unset env — the caller is using a Weplex build that doesn't
    // know about this contract, and silently shoehorning notes into the
    // default profile's Keychain key would scramble cross-profile reads.
    let profile_id = match std::env::var("WEPLEX_PROFILE_ID") {
        Ok(v) if !v.is_empty() => v,
        _ => {
            return Err(
                "WEPLEX_PROFILE_ID env not set — upgrade Weplex to a build that injects it"
                    .to_string(),
            );
        }
    };

    let files_changed: Vec<Value> = arguments
        .get("filesChanged")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let decisions: Vec<Value> = arguments
        .get("decisions")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let request = serde_json::json!({
        "method": IPC_METHOD_LOG_ACTIVITY,
        "params": {
            "session_id": session_id,
            "profile_id": profile_id,
            "text": summary,
            "files_changed": files_changed,
            "decisions": decisions,
        }
    });

    let response = ipc.send(request)?;
    if let Some(err) = response.get("error") {
        let msg = err
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("Unknown error");
        return Err(msg.to_string());
    }

    Ok(serde_json::json!({
        "content": [{
            "type": "text",
            "text": "Activity note recorded. Future you can read it back from this session's timeline."
        }]
    }))
}

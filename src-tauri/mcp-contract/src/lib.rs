//! Shared contract between the Weplex Tauri app and the `weplex-mcp` sidecar.
//!
//! Every string name that crosses a process boundary (MCP tool names, JSON-RPC
//! method names, stop-hook decision actions) lives here so a rename triggers a
//! compile error in both crates.

use serde::{Deserialize, Serialize};

// ── MCP tool names (exposed to agents) ────────────────────────────────────────

pub const TOOL_LOG_ACTIVITY: &str = "weplex_log_activity";
pub const TOOL_LIST_SESSIONS: &str = "weplex_list_sessions";
pub const TOOL_CREATE_SESSION: &str = "weplex_create_session";
pub const TOOL_READ_OUTPUT: &str = "weplex_read_output";
pub const TOOL_SEND_INPUT: &str = "weplex_send_input";
pub const TOOL_GET_CONTEXT: &str = "weplex_get_context";

// ── JSON-RPC method names (MCP protocol) ──────────────────────────────────────

pub const METHOD_INITIALIZE: &str = "initialize";
pub const METHOD_NOTIFICATIONS_INITIALIZED: &str = "notifications/initialized";
pub const METHOD_TOOLS_LIST: &str = "tools/list";
pub const METHOD_TOOLS_CALL: &str = "tools/call";

// ── IPC method names (weplex-mcp → Tauri backend over Unix socket) ────────────

pub const IPC_METHOD_LIST_SESSIONS: &str = "list_sessions";
pub const IPC_METHOD_CREATE_SESSION: &str = "create_session";
pub const IPC_METHOD_READ_OUTPUT: &str = "read_output";
pub const IPC_METHOD_SEND_INPUT: &str = "send_input";
pub const IPC_METHOD_GET_CONTEXT: &str = "get_context";
pub const IPC_METHOD_LOG_ACTIVITY: &str = "log_activity";

// ── Stop-hook courier protocol (hook_server → stop-hook.sh) ───────────────────

/// Decision returned from POST /hook for Stop events.
/// Serialized with `action` as the discriminant so bash can inspect via `jq -r .action`.
///
/// Security note: the `tool` and `message` fields flow from the Rust backend
/// into a bash script and, in the `request_notes` branch, eventually reach the
/// agent's conversation as untrusted text. They MUST NEVER be used in `eval`,
/// command substitution, or any context that interprets the value as code.
/// The current `stop-hook.sh` template only passes them to `jq --arg` (literal
/// string bind) — keep it that way.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum StopDecision {
    /// Hook should exit 0 silently — either no ask needed, ask cap reached,
    /// or pre-conditions not met.
    ExitOk,
    /// Hook should emit `{"decision":"block","reason":message}` on stdout and
    /// exit 2 to make Claude Code feed the message back to the agent.
    RequestNotes { tool: String, message: String },
}

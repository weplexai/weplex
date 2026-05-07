/// Claude Code session state: usage stats, active/idle detection, session summaries.

use std::io::BufRead;

#[derive(serde::Serialize, Default)]
pub struct ClaudeUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub model: Option<String>,
    pub turns: u32,
}

/// Get the Claude projects directory for a given cwd.
fn claude_sessions_dir(cwd: &str) -> String {
    let home = crate::utils::get_home();
    let resolved = crate::utils::resolve_cwd(cwd);
    let encoded = resolved.replace("/", "-");
    format!("{}/.claude/projects/{}", home, encoded)
}

/// Validate session_id to prevent path traversal.
fn validate_session_id(session_id: &str) -> Result<(), String> {
    if session_id.is_empty()
        || !session_id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err("Invalid session ID".to_string());
    }
    Ok(())
}

/// Parse a Claude JSONL session file and aggregate usage stats.
#[tauri::command]
pub fn get_claude_usage(cwd: String, session_id: String) -> Result<Option<ClaudeUsage>, String> {
    validate_session_id(&session_id)?;
    let sessions_dir = claude_sessions_dir(&cwd);
    let path = format!("{}/{}.jsonl", sessions_dir, session_id);

    let file = match std::fs::File::open(&path) {
        Ok(f) => f,
        Err(_) => return Ok(None),
    };

    let reader = std::io::BufReader::new(file);
    let mut usage = ClaudeUsage::default();

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        let obj: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let Some(msg) = obj.get("message") else {
            continue;
        };
        if let Some(u) = msg.get("usage") {
            usage.input_tokens += u.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
            usage.output_tokens += u.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
            usage.cache_read_tokens += u
                .get("cache_read_input_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            usage.cache_write_tokens += u
                .get("cache_creation_input_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            usage.turns += 1;
        }
        if let Some(m) = msg.get("model").and_then(|v| v.as_str()) {
            usage.model = Some(m.to_string());
        }
    }

    if usage.turns == 0 {
        return Ok(None);
    }

    Ok(Some(usage))
}

/// Read the last message in a Claude JSONL session to determine if Claude is active or idle.
///
/// Returns:
///   "active" — Claude is processing (last role = user, or last assistant ends with tool_use)
///   "idle"   — Claude finished its turn (last assistant ends with text)
///   None     — can't determine (file missing, empty, unknown format)
#[tauri::command]
pub fn get_claude_state(cwd: String, session_id: String) -> Result<Option<String>, String> {
    validate_session_id(&session_id)?;
    let sessions_dir = claude_sessions_dir(&cwd);
    let path = format!("{}/{}.jsonl", sessions_dir, session_id);

    let file = match std::fs::File::open(&path) {
        Ok(f) => f,
        Err(_) => return Ok(None),
    };

    let reader = std::io::BufReader::new(file);
    let mut last_msg: Option<serde_json::Value> = None;

    for line in reader.lines() {
        let line = match line {
            Ok(l) if !l.trim().is_empty() => l,
            _ => continue,
        };
        let obj: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if obj.get("message").and_then(|m| m.get("role")).is_some() {
            last_msg = Some(obj);
        }
    }

    let msg = match last_msg.as_ref().and_then(|o| o.get("message")) {
        Some(m) => m,
        None => return Ok(None),
    };

    let role = match msg.get("role").and_then(|r| r.as_str()) {
        Some(r) => r,
        None => return Ok(None),
    };

    let state = match role {
        "user" => "active", // Claude is processing user message or tool result
        "assistant" => {
            // Check the type of the last content item:
            //   tool_use → Claude dispatched a tool, still working   → active
            //   text     → Claude wrote a final response              → idle
            let last_content_type = msg
                .get("content")
                .and_then(|c| c.as_array())
                .and_then(|arr| arr.last())
                .and_then(|item| item.get("type"))
                .and_then(|t| t.as_str());

            match last_content_type {
                Some("tool_use") => "active", // dispatched a tool, waiting for result
                Some("thinking") => "active", // extended thinking, no text yet
                Some("text") | Some("image") => "idle", // final response written
                _ => "idle",
            }
        }
        _ => return Ok(None),
    };

    Ok(Some(state.to_string()))
}

/// Get session summary by session ID. `profile_id` selects the Keychain key
/// used to decrypt the file — the frontend must pass the same value it
/// already uses to spawn PTYs (configDir absolute path, or "default").
///
/// `profile_id` is required, not optional: a forgotten parameter would
/// silently default to the system profile and yield 🔒 for any non-default
/// session — confusing to debug. Better to fail loudly at the IPC layer.
#[tauri::command]
pub fn get_session_summary(
    session_id: String,
    profile_id: String,
) -> Option<crate::session_summary::SessionSummary> {
    // Validate session_id to prevent path traversal or injection
    if session_id.is_empty()
        || !session_id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return None;
    }
    if profile_id.is_empty() {
        return None;
    }
    crate::session_summary::read_summary(&session_id, &profile_id)
}

/// Hook scripts generation and profile synchronization.

/// Shared bash preamble for hooks that resolve session ID from session-map.
const HOOK_PREAMBLE: &str = r#"#!/bin/bash
# Weplex Hook — reads Claude Code hook JSON from stdin,
# resolves Weplex session ID, POSTs event to local hook server.
# Requires: jq, curl (both pre-installed on macOS).

# Fail silently if jq is not available
command -v jq >/dev/null 2>&1 || exit 0

INPUT=$(cat -)

# Extract cwd from stdin JSON using jq (safe parsing)
CWD=$(echo "$INPUT" | jq -r '.cwd // empty' 2>/dev/null)
if [ -z "$CWD" ]; then exit 0; fi

# Resolve Weplex session ID from session-map
SESSION_MAP_DIR="$HOME/.weplex/session-map"
CWD_NORM=$(echo "$CWD" | sed "s|^$HOME|~|")
ENCODED_CWD=$(echo "$CWD_NORM" | sed 's|/|_|g')
MAP_FILE="$SESSION_MAP_DIR/$ENCODED_CWD"

if [ ! -f "$MAP_FILE" ]; then exit 0; fi
WEPLEX_SID=$(cat "$MAP_FILE")
if [ -z "$WEPLEX_SID" ]; then exit 0; fi

# Read hook server port and auth token
PORT_FILE="$HOME/.weplex/hook-port"
TOKEN_FILE="$HOME/.weplex/hook-token"
if [ ! -f "$PORT_FILE" ]; then exit 0; fi
PORT=$(cat "$PORT_FILE")
if [ -z "$PORT" ]; then exit 0; fi
TOKEN=""
if [ -f "$TOKEN_FILE" ]; then TOKEN=$(cat "$TOKEN_FILE"); fi
"#;

/// SessionStart hook — standalone (reads WEPLEX_SESSION_ID from env, not session-map).
const SESSION_START_SCRIPT: &str = r#"#!/bin/bash
# Weplex Hook — SessionStart: captures Claude session ID and sends to Weplex.
# Claude Code provides session_id (UUID) in stdin JSON at session start.
# Weplex session ID comes from WEPLEX_SESSION_ID env var (set by PTY).

command -v jq >/dev/null 2>&1 || exit 0

INPUT=$(cat -)

CLAUDE_SID=$(echo "$INPUT" | jq -r '.session_id // empty' 2>/dev/null)
if [ -z "$CLAUDE_SID" ]; then exit 0; fi

# Weplex session ID from environment (set by Weplex when creating PTY)
WEPLEX_SID="$WEPLEX_SESSION_ID"
if [ -z "$WEPLEX_SID" ]; then exit 0; fi

# Read hook server port and auth token
PORT_FILE="$HOME/.weplex/hook-port"
TOKEN_FILE="$HOME/.weplex/hook-token"
if [ ! -f "$PORT_FILE" ]; then exit 0; fi
PORT=$(cat "$PORT_FILE")
if [ -z "$PORT" ]; then exit 0; fi
TOKEN=""
if [ -f "$TOKEN_FILE" ]; then TOKEN=$(cat "$TOKEN_FILE"); fi

# Send session_start event with Claude session ID
PAYLOAD=$(jq -n -c --arg evt "session_start" --arg sid "$WEPLEX_SID" --arg csid "$CLAUDE_SID" \
  '{
    event_type: $evt,
    session_id: ($sid | tonumber),
    claude_session_id: $csid
  }' 2>/dev/null)

if [ -z "$PAYLOAD" ]; then exit 0; fi

curl -s -X POST "http://127.0.0.1:$PORT/hook" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d "$PAYLOAD" \
  --max-time 2 > /dev/null 2>&1

exit 0
"#;

/// Extra bash appended to the stop hook (session notes prompt).
const STOP_EXTRA: &str = r#"
# Check if running inside Weplex (WEPLEX_SESSION_ID set by PTY)
# If not in Weplex, skip the notes check entirely
if [ -z "$WEPLEX_SID" ]; then exit 0; fi

# Check if Weplex hook server is reachable (port file exists)
if [ ! -f "$PORT_FILE" ]; then exit 0; fi

# Check if agent provided activity notes
SUMMARY_FILE="$HOME/.weplex/summaries/${WEPLEX_SID}.json"
if [ -f "$SUMMARY_FILE" ]; then
  UPDATED_AT=$(jq -r '.updatedAt // 0' "$SUMMARY_FILE" 2>/dev/null || echo "0")
  NOW=$(date +%s)
  AGE=$(( NOW - UPDATED_AT ))
  if [ "$AGE" -lt 300 ]; then exit 0; fi
fi

# Only request notes if weplex_update_notes MCP tool is likely available
# (Weplex MCP server must be registered and running)
if [ ! -f "$HOME/.weplex/mcp-ready" ] && [ ! -S "$HOME/.weplex/ipc-global.sock" ]; then
  exit 0
fi

echo "Please call the weplex_update_notes tool to record what you accomplished before finishing." >&2
exit 2
"#;

/// Generate a hook script from template: preamble + jq payload + curl POST + optional extra bash.
fn render_hook_script(event_type: &str, jq_fields: &str, extra_bash: &str) -> String {
    format!(
        r#"{preamble}
PAYLOAD=$(echo "$INPUT" | jq -c --arg evt "{event}" --arg sid "$WEPLEX_SID" --arg cwd "$CWD_NORM" \
  '{{
    event_type: $evt,
    session_id: ($sid | tonumber),
    cwd: $cwd{fields}
  }}' 2>/dev/null)

if [ -z "$PAYLOAD" ]; then exit 0; fi

curl -s -X POST "http://127.0.0.1:$PORT/hook" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d "$PAYLOAD" \
  --max-time 2 > /dev/null 2>&1
{extra}
exit 0
"#,
        preamble = HOOK_PREAMBLE,
        event = event_type,
        fields = jq_fields,
        extra = extra_bash,
    )
}

/// Generate all hook scripts at ~/.weplex/hooks/.
pub fn ensure_hook_script() -> Result<(), String> {
    let home = crate::utils::get_home();
    let hooks_dir = format!("{}/.weplex/hooks", home);
    std::fs::create_dir_all(&hooks_dir)
        .map_err(|e| format!("Failed to create hooks dir: {}", e))?;

    let tool_fields = ",\n    tool_name: (.tool_name // null),\n    file_path: (.file_path // null)";
    let agent_fields = ",\n    agent_type: (.agent_type // null),\n    agent_id: (.agent_id // null)";

    let scripts = [
        ("session-start.sh", SESSION_START_SCRIPT.to_string()),
        ("pre-tool-use.sh", render_hook_script(
            "pre_tool_use",
            &format!("{},\n    tool_input: ((.tool_input // \"\") | tostring | .[0:500])", tool_fields),
            "",
        )),
        ("post-tool-use.sh", render_hook_script(
            "post_tool_use",
            &format!("{},\n    tool_output: ((.tool_output // \"\") | tostring | .[0:500])", tool_fields),
            "",
        )),
        ("stop-hook.sh", render_hook_script("stop", "", STOP_EXTRA)),
        ("subagent-start.sh", render_hook_script("subagent_start", agent_fields, "")),
        ("subagent-stop.sh", render_hook_script("subagent_stop", agent_fields, "")),
    ];

    for (name, content) in &scripts {
        let path = format!("{}/{}", hooks_dir, name);
        std::fs::write(&path, content)
            .map_err(|e| format!("Failed to write {}: {}", name, e))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))
                .map_err(|e| format!("Failed to set permissions on {}: {}", name, e))?;
        }
    }

    log::info!("hook scripts written to {}", hooks_dir);
    Ok(())
}

/// Build a single Weplex hook entry for claude-hooks.json / settings.json.
fn weplex_hook_entry(command: &str, status_message: &str) -> serde_json::Value {
    serde_json::json!({
        "hooks": [{
            "type": "command",
            "command": command,
            "timeout": 10,
            "statusMessage": status_message
        }]
    })
}

/// Generate ~/.weplex/claude-hooks.json — single source of truth for all
/// Weplex hooks. This file is then synced into each profile's settings.json.
pub fn write_weplex_hooks_source() -> Result<(), String> {
    let home = crate::utils::get_home();
    let hooks_dir = format!("{}/.weplex/hooks", home);
    let source_path = format!("{}/.weplex/claude-hooks.json", home);

    let source = serde_json::json!({
        "hooks": {
            "SessionStart": [
                weplex_hook_entry(
                    &format!("{}/session-start.sh", hooks_dir),
                    "[Weplex] Capturing session ID"
                )
            ],
            "PreToolUse": [
                weplex_hook_entry(
                    &format!("{}/pre-tool-use.sh", hooks_dir),
                    "[Weplex] Tracking tool use"
                )
            ],
            "PostToolUse": [
                weplex_hook_entry(
                    &format!("{}/post-tool-use.sh", hooks_dir),
                    "[Weplex] Tracking tool result"
                )
            ],
            "Stop": [
                weplex_hook_entry(
                    &format!("{}/stop-hook.sh", hooks_dir),
                    "[Weplex] Session notes check"
                )
            ],
            "SubagentStart": [
                weplex_hook_entry(
                    &format!("{}/subagent-start.sh", hooks_dir),
                    "[Weplex] Tracking subagent"
                )
            ],
            "SubagentStop": [
                weplex_hook_entry(
                    &format!("{}/subagent-stop.sh", hooks_dir),
                    "[Weplex] Subagent finished"
                )
            ]
        }
    });

    let json_str = serde_json::to_string_pretty(&source).map_err(|e| e.to_string())?;
    std::fs::write(&source_path, json_str).map_err(|e| e.to_string())?;

    log::info!("hooks source written to {}", source_path);
    Ok(())
}

/// Check if a hook entry belongs to Weplex (by .weplex/ in command path).
fn is_weplex_hook(entry: &serde_json::Value) -> bool {
    entry
        .get("hooks")
        .and_then(|h| h.as_array())
        .map(|hooks| {
            hooks.iter().any(|hook| {
                hook.get("command")
                    .and_then(|c| c.as_str())
                    .map(|c| c.contains(".weplex/"))
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

/// Sync Weplex hooks from ~/.weplex/claude-hooks.json into a single
/// profile's settings.json. Removes stale Weplex entries first, then
/// appends current ones. Non-Weplex entries are never touched.
fn sync_hooks_to_profile(config_dir: &str) -> Result<(), String> {
    let home = crate::utils::get_home();
    let source_path = format!("{}/.weplex/claude-hooks.json", home);
    let settings_path = format!("{}/settings.json", config_dir);

    // Read source hooks
    let source_content = std::fs::read_to_string(&source_path)
        .map_err(|e| format!("Failed to read hooks source: {}", e))?;
    let source: serde_json::Value = serde_json::from_str(&source_content)
        .map_err(|e| format!("Invalid hooks source JSON: {}", e))?;

    // Read existing profile settings or create empty object
    let mut settings: serde_json::Value = std::fs::read_to_string(&settings_path)
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or(serde_json::json!({}));

    if settings.get("hooks").is_none() {
        settings["hooks"] = serde_json::json!({});
    }

    let source_hooks = match source["hooks"].as_object() {
        Some(h) => h,
        None => return Err("Invalid hooks source: missing hooks object".to_string()),
    };

    for (hook_type, weplex_entries) in source_hooks {
        // Ensure hooks.<Type> is an array
        if !settings["hooks"]
            .get(hook_type)
            .map(|v| v.is_array())
            .unwrap_or(false)
        {
            settings["hooks"][hook_type] = serde_json::json!([]);
        }

        let arr = settings["hooks"][hook_type].as_array_mut().unwrap();

        // Remove old Weplex entries
        arr.retain(|entry| !is_weplex_hook(entry));

        // Append current Weplex entries
        if let Some(entries) = weplex_entries.as_array() {
            for entry in entries {
                arr.push(entry.clone());
            }
        }
    }

    // Also clean up Weplex hooks from event types that are no longer in source
    // (in case we removed a hook type in an update)
    if let Some(settings_hooks) = settings["hooks"].as_object_mut() {
        for (hook_type, entries) in settings_hooks.iter_mut() {
            if let Some(arr) = entries.as_array_mut() {
                if !source_hooks.contains_key(hook_type) {
                    arr.retain(|entry| !is_weplex_hook(entry));
                }
            }
        }
    }

    // Ensure config dir exists
    std::fs::create_dir_all(config_dir)
        .map_err(|e| format!("Failed to create config dir {}: {}", config_dir, e))?;

    // Atomic write: write to temp file, then rename to avoid partial writes.
    // Remove stale .tmp first to prevent writing through a symlink.
    let json_str = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    let tmp_path = format!("{}.tmp", settings_path);
    let _ = std::fs::remove_file(&tmp_path);
    std::fs::write(&tmp_path, &json_str)
        .map_err(|e| format!("Failed to write temp settings: {}", e))?;
    std::fs::rename(&tmp_path, &settings_path)
        .map_err(|e| format!("Failed to rename temp settings: {}", e))?;

    log::info!("hooks synced to {}", settings_path);
    Ok(())
}

/// Tauri command: sync Weplex hooks into all known profiles.
/// Called from frontend after profileStore loads from localStorage.
/// Ensures source file is fresh before syncing.
#[tauri::command]
pub fn sync_hooks_for_profiles(profile_config_dirs: Vec<String>) -> Result<(), String> {
    let home = crate::utils::get_home();

    // Ensure source file is fresh (no race with startup thread)
    let _ = write_weplex_hooks_source();

    // Always sync default profile (~/.claude/)
    let default_dir = format!("{}/.claude", home);
    if let Err(e) = sync_hooks_to_profile(&default_dir) {
        log::warn!("failed to sync hooks to default profile: {}", e);
    }

    // Sync each custom profile
    for config_dir in &profile_config_dirs {
        if config_dir.is_empty() {
            continue;
        }
        match crate::utils::validate_config_dir(config_dir) {
            Ok(validated) => {
                if let Err(e) = sync_hooks_to_profile(&validated) {
                    log::warn!("failed to sync hooks to {}: {}", validated, e);
                }
            }
            Err(e) => log::warn!("skipping invalid profile dir {}: {}", config_dir, e),
        }
    }

    Ok(())
}

/// Tauri command: sync Weplex hooks into a single profile.
/// Called when user creates a new profile.
#[tauri::command]
pub fn sync_hooks_for_profile(config_dir: String) -> Result<(), String> {
    // Ensure source file exists
    let _ = write_weplex_hooks_source();

    if config_dir.is_empty() {
        let home = crate::utils::get_home();
        return sync_hooks_to_profile(&format!("{}/.claude", home));
    }

    let validated = crate::utils::validate_config_dir(&config_dir)?;
    sync_hooks_to_profile(&validated)
}

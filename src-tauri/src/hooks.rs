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

/// Build the Stop hook script. Unlike other hooks, this one doesn't just POST
/// telemetry — it reads the server's JSON response and, if the server asks,
/// emits `{"decision":"block","reason":...}` on stdout with exit 2 so Claude
/// Code feeds the reason back to the agent.
///
/// Fault-tolerance contract: any curl error, HTTP non-200, malformed JSON,
/// missing `action` field, or unknown action → `exit 0` silently. The only
/// path that emits exit 2 is a well-formed `{"action":"request_notes",...}`
/// response from the backend. The backend itself enforces the 1-ask-per-
/// session cap (see `hook_decision.rs`), so even if this script is called in
/// a tight loop it cannot produce more than one block per session lifetime.
fn render_stop_hook_script() -> String {
    format!(
        r#"{preamble}
PAYLOAD=$(echo "$INPUT" | jq -c --arg evt "stop" --arg sid "$WEPLEX_SID" --arg cwd "$CWD_NORM" \
  '{{
    event_type: $evt,
    session_id: ($sid | tonumber),
    cwd: $cwd
  }}' 2>/dev/null)

if [ -z "$PAYLOAD" ]; then exit 0; fi

# Capture body and HTTP status separately so we can fall through on any error.
RESP=$(curl -s -X POST "http://127.0.0.1:$PORT/hook" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d "$PAYLOAD" \
  --max-time 2 \
  -w "\n%{{http_code}}" 2>/dev/null) || exit 0

STATUS=$(printf '%s\n' "$RESP" | tail -n 1)
BODY=$(printf '%s\n' "$RESP" | sed '$d')

# Anything other than 200 → silently allow stop.
if [ "$STATUS" != "200" ]; then exit 0; fi

ACTION=$(printf '%s' "$BODY" | jq -r '.action // "exit_ok"' 2>/dev/null)

if [ "$ACTION" = "request_notes" ]; then
  REASON=$(printf '%s' "$BODY" | jq -r '.message // empty' 2>/dev/null)
  if [ -z "$REASON" ]; then exit 0; fi
  # Emit Claude Code's Stop-hook block directive: stdout JSON + exit 2.
  printf '%s' "$BODY" | jq -c --arg r "$REASON" '{{decision: "block", reason: $r}}' 2>/dev/null || exit 0
  exit 2
fi

exit 0
"#,
        preamble = HOOK_PREAMBLE,
    )
}

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
        ("stop-hook.sh", render_stop_hook_script()),
        ("subagent-start.sh", render_hook_script("subagent_start", agent_fields, "")),
        ("subagent-stop.sh", render_hook_script("subagent_stop", agent_fields, "")),
    ];

    // Atomic write with 0700 from birth — avoids the brief window where the
    // script exists with default umask perms before the chmod call.
    for (name, content) in &scripts {
        let path = format!("{}/{}", hooks_dir, name);
        crate::utils::atomic_write_exec_owner_only(&path, content)?;
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
    crate::utils::atomic_write_owner_only(&source_path, &json_str)?;

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

    // Atomic tmp+rename with owner-only perms: settings.json may contain
    // other tool configs (auth scopes, API endpoints) that shouldn't be
    // world-readable during the write window.
    let json_str = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    crate::utils::atomic_write_owner_only(&settings_path, &json_str)?;

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

// ─────────────────────────────────────────────────────────────────────────────
// Shell characterization tests for stop-hook.sh
//
// These tests run the actual generated bash script against a stubbed `curl`
// on PATH, verifying the exit code + stdout behaviour for every decision
// branch. The anti-loop guarantee ultimately rests on this script obeying
// the backend's `action`, so the production-rendered text is exercised
// end-to-end (bash, jq, sed, tail, printf included).
//
// Requirements (available on macOS + most Linux CI images): bash, jq, sed,
// printf, tail, date. If jq is missing, each test is skipped rather than
// failing, because the hook preamble itself exits 0 without jq.
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
#[cfg(unix)]
mod stop_hook_script_tests {
    use super::render_stop_hook_script;
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};

    /// Spin up an isolated $HOME + PATH with a stubbed `curl` that returns the
    /// given body + HTTP status. Runs the production-rendered `stop-hook.sh`
    /// via bash and returns (exit_code, stdout).
    fn run_hook(body: &str, status: u16, curl_exit: i32) -> (i32, String) {
        let tmp_root = std::env::temp_dir().join(format!(
            "weplex-stop-hook-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let home = tmp_root.join("home");
        let bin = tmp_root.join("bin");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&bin).unwrap();

        // Fake ~/.weplex layout: session-map, activity, port, token.
        let weplex = home.join(".weplex");
        let session_map = weplex.join("session-map");
        std::fs::create_dir_all(weplex.join("activity")).unwrap();
        std::fs::create_dir_all(&session_map).unwrap();
        std::fs::write(weplex.join("hook-port"), "9999").unwrap();
        std::fs::write(weplex.join("hook-token"), "testtoken").unwrap();

        // The preamble normalizes $HOME prefix to `~`, then replaces `/` with
        // `_`. Using a cwd under $HOME makes the encoded path deterministic.
        let cwd = format!("{}/project", home.display());
        let encoded_cwd = "~_project";
        std::fs::write(session_map.join(encoded_cwd), "42").unwrap();

        // Stub `curl`: body lines first, then a trailing status line
        // (mirrors what `-w "\n%{http_code}"` produces). If `curl_exit` is
        // non-zero, exit with that code instead to simulate a network error.
        let curl_stub = if curl_exit != 0 {
            format!("#!/bin/bash\nexit {}\n", curl_exit)
        } else {
            // Use a heredoc-safe encoding: no EOF markers in body.
            // printf is the safest way to emit the body as-is then newline+status.
            format!(
                "#!/bin/bash\nprintf '%s\\n%d' {body} {status}\n",
                body = shell_quote(body),
                status = status
            )
        };
        write_stub(&bin.join("curl"), &curl_stub);

        let script_path = tmp_root.join("stop-hook.sh");
        write_stub(&script_path, &render_stop_hook_script());

        // Prepend stub dir so our fake curl wins; keep rest of PATH for jq/sed/etc.
        let path = format!(
            "{}:{}",
            bin.display(),
            std::env::var("PATH").unwrap_or_default()
        );

        let input = format!(r#"{{"cwd":"{}"}}"#, cwd);

        let mut child = Command::new("bash")
            .arg(&script_path)
            .env("HOME", &home)
            .env("PATH", &path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("bash spawn failed");
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        let output = child.wait_with_output().expect("bash wait failed");

        let _ = std::fs::remove_dir_all(&tmp_root);
        (
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stdout).to_string(),
        )
    }

    fn write_stub(path: &Path, contents: &str) {
        std::fs::write(path, contents).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// Single-quote-safe shell encoding for embedding arbitrary string into
    /// a bash script: 'abc' → ''abc'', embedded single-quotes escaped.
    fn shell_quote(s: &str) -> String {
        let mut out = String::from("'");
        for c in s.chars() {
            if c == '\'' {
                out.push_str(r#"'\''"#);
            } else {
                out.push(c);
            }
        }
        out.push('\'');
        out
    }

    fn jq_available() -> bool {
        Command::new("jq")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    #[test]
    fn request_notes_emits_block_and_exits_2() {
        if !jq_available() {
            return;
        }
        let body = r#"{"action":"request_notes","tool":"weplex_log_activity","message":"Please call the tool"}"#;
        let (code, stdout) = run_hook(body, 200, 0);
        assert_eq!(code, 2, "stdout was: {}", stdout);
        assert!(
            stdout.contains(r#""decision":"block""#),
            "expected block in: {}",
            stdout
        );
        assert!(
            stdout.contains(r#""reason":"Please call the tool""#),
            "expected reason in: {}",
            stdout
        );
    }

    #[test]
    fn exit_ok_action_exits_0_silent() {
        if !jq_available() {
            return;
        }
        let body = r#"{"action":"exit_ok"}"#;
        let (code, stdout) = run_hook(body, 200, 0);
        assert_eq!(code, 0);
        assert!(stdout.is_empty(), "expected no stdout, got: {}", stdout);
    }

    #[test]
    fn http_500_exits_0_silent() {
        if !jq_available() {
            return;
        }
        let (code, stdout) = run_hook(r#"{"error":"boom"}"#, 500, 0);
        assert_eq!(code, 0);
        assert!(stdout.is_empty(), "expected no stdout, got: {}", stdout);
    }

    #[test]
    fn malformed_json_exits_0_silent() {
        if !jq_available() {
            return;
        }
        let (code, stdout) = run_hook("not-json-at-all", 200, 0);
        assert_eq!(code, 0);
        assert!(stdout.is_empty(), "expected no stdout, got: {}", stdout);
    }

    #[test]
    fn unknown_action_exits_0_silent() {
        if !jq_available() {
            return;
        }
        let body = r#"{"action":"teleport","details":"nope"}"#;
        let (code, stdout) = run_hook(body, 200, 0);
        assert_eq!(code, 0);
        assert!(stdout.is_empty(), "expected no stdout, got: {}", stdout);
    }

    #[test]
    fn curl_failure_exits_0_silent() {
        if !jq_available() {
            return;
        }
        // curl exits 7 (CURLE_COULDNT_CONNECT) → bash `curl || exit 0` fires.
        let (code, stdout) = run_hook("", 0, 7);
        assert_eq!(code, 0);
        assert!(stdout.is_empty(), "expected no stdout, got: {}", stdout);
    }

    #[test]
    fn request_notes_without_message_exits_0_silent() {
        if !jq_available() {
            return;
        }
        // Backend forgot to include `message` — courier must not emit an
        // empty-reason block; just give up silently.
        let body = r#"{"action":"request_notes","tool":"weplex_log_activity"}"#;
        let (code, stdout) = run_hook(body, 200, 0);
        assert_eq!(code, 0);
        assert!(stdout.is_empty(), "expected no stdout, got: {}", stdout);
    }

    // Helpers are only used in tests.
    #[allow(dead_code)]
    fn _unused_marker(_p: PathBuf) {}
}

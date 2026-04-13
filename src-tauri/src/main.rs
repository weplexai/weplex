// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(clippy::too_many_arguments)]

mod hook_server;
mod ipc_server;
mod plugin_host;
mod plugins;
mod keychain;
mod oauth_server;
mod pty_manager;
mod resources;
mod secure_store;
mod session_summary;
// mod weplex_agents; // Removed: Claude-first strategy, no agent-agnostic format

use pty_manager::PtyManager;
use std::io::BufRead;
use std::sync::Mutex;
use tauri::{Manager, State};

struct AppState {
    pty_manager: std::sync::Arc<Mutex<PtyManager>>,
    ipc_pool: Mutex<ipc_server::IpcSocketPool>,
}

#[tauri::command]
fn create_pty(
    state: State<AppState>,
    app: tauri::AppHandle,
    session_id: u32,
    cols: u16,
    rows: u16,
    command: Option<String>,
    cwd: Option<String>,
    env_vars: Option<std::collections::HashMap<String, String>>,
) -> Result<(), String> {
    // Write session-map file so stop hooks can resolve cwd → session_id
    if let Some(ref cwd_path) = cwd {
        let _ = write_session_map(session_id, cwd_path);
    }

    let mut manager = state.pty_manager.lock().map_err(|e| e.to_string())?;
    manager
        .create(session_id, cols, rows, command, cwd, env_vars, app)
        .map_err(|e| e.to_string())
}

/// Clean all session-map files on startup (stale from previous runs).
fn clean_session_map() -> Result<(), String> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    let map_dir = format!("{}/.weplex/session-map", home);
    if let Ok(entries) = std::fs::read_dir(&map_dir) {
        for entry in entries.flatten() {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    Ok(())
}

/// Write cwd → session_id mapping for stop hook resolution.
/// Path: ~/.weplex/session-map/<encoded_cwd> containing session ID.
/// Normalizes $HOME → ~ before encoding to match hook script behavior.
fn write_session_map(session_id: u32, cwd: &str) -> Result<(), String> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    let map_dir = format!("{}/.weplex/session-map", home);
    std::fs::create_dir_all(&map_dir).map_err(|e| e.to_string())?;

    // Normalize: replace $HOME with ~ (must match hook script normalization)
    let normalized = if cwd.starts_with(&home) {
        format!("~{}", &cwd[home.len()..])
    } else {
        cwd.to_string()
    };
    // Encode: replace / with _
    let encoded = normalized.replace('/', "_");
    let map_path = format!("{}/{}", map_dir, encoded);
    std::fs::write(&map_path, session_id.to_string()).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn write_pty(state: State<AppState>, session_id: u32, data: String) -> Result<(), String> {
    let mut manager = state.pty_manager.lock().map_err(|e| e.to_string())?;
    manager.write(session_id, &data).map_err(|e| e.to_string())
}

#[tauri::command]
fn resize_pty(state: State<AppState>, session_id: u32, cols: u16, rows: u16) -> Result<(), String> {
    let mut manager = state.pty_manager.lock().map_err(|e| e.to_string())?;
    manager
        .resize(session_id, cols, rows)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn kill_pty(state: State<AppState>, session_id: u32) -> Result<(), String> {
    let mut manager = state.pty_manager.lock().map_err(|e| e.to_string())?;
    manager.kill(session_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_session_summary(session_id: String) -> Option<session_summary::SessionSummary> {
    // Validate session_id to prevent path traversal or injection
    if session_id.is_empty()
        || !session_id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return None;
    }
    session_summary::read_summary(&session_id)
}

/// Resolve a cwd with tilde to an absolute path.
fn resolve_cwd(cwd: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    if cwd == "~" {
        home
    } else if let Some(rest) = cwd.strip_prefix("~/") {
        format!("{}/{}", home, rest)
    } else if let Some(rest) = cwd.strip_prefix("~") {
        format!("{}/{}", home, rest)
    } else {
        cwd.to_string()
    }
}

/// Get the Claude projects directory for a given cwd.
fn claude_sessions_dir(cwd: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    let resolved = resolve_cwd(cwd);
    let encoded = resolved.replace("/", "-");
    format!("{}/.claude/projects/{}", home, encoded)
}

#[derive(serde::Serialize, Default)]
struct ClaudeUsage {
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_write_tokens: u64,
    model: Option<String>,
    turns: u32,
}

/// Parse a Claude JSONL session file and aggregate usage stats.
#[tauri::command]
fn get_claude_usage(cwd: String, session_id: String) -> Result<Option<ClaudeUsage>, String> {
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
fn get_claude_state(cwd: String, session_id: String) -> Result<Option<String>, String> {
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

#[tauri::command]
fn list_dirs(partial: String) -> Vec<String> {
    if partial.is_empty() {
        return Vec::new();
    }

    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    let resolved = resolve_cwd(&partial);

    // Split into parent directory and prefix to filter by
    let (parent_path, prefix) = if partial == "~" || resolved.ends_with('/') {
        (resolved.clone(), String::new())
    } else {
        let p = std::path::Path::new(&resolved);
        let parent = p
            .parent()
            .and_then(|pp| pp.to_str())
            .unwrap_or("/")
            .to_string();
        let name = p
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        (parent, name)
    };

    let entries = match std::fs::read_dir(&parent_path) {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };

    let prefix_lower = prefix.to_lowercase();
    let mut results = Vec::new();

    for entry in entries.flatten() {
        let is_dir = entry
            .file_type()
            .map(|ft| {
                ft.is_dir()
                    || (ft.is_symlink() && entry.metadata().map(|m| m.is_dir()).unwrap_or(false))
            })
            .unwrap_or(false);
        if !is_dir {
            continue;
        }

        let name = match entry.file_name().into_string() {
            Ok(n) => n,
            Err(_) => continue,
        };

        // Skip hidden dirs unless user typed a dot
        if name.starts_with('.') && !prefix.starts_with('.') {
            continue;
        }

        if prefix.is_empty() || name.to_lowercase().starts_with(&prefix_lower) {
            let full = format!("{}/{}", parent_path.trim_end_matches('/'), name);
            let display = if full.starts_with(&home) {
                format!("~{}", &full[home.len()..])
            } else {
                full
            };
            results.push(display);
        }
    }

    results.sort_by_key(|a| a.to_lowercase());
    results.truncate(20);
    results
}

#[derive(serde::Serialize)]
struct DiscoveredProfile {
    path: String,
    name: String,
    source: String,
}

/// Infer a human-readable name from a `.claude-*` directory suffix.
/// e.g. "work" → "Work", "client-acme" → "Client Acme"
fn infer_profile_name(suffix: &str) -> String {
    suffix
        .split('-')
        .map(|word| {
            let mut c = word.chars();
            match c.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().to_string() + c.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Scan filesystem and shell configs for existing Claude configuration directories.
#[tauri::command]
fn discover_profiles() -> Result<Vec<DiscoveredProfile>, String> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    let mut results: Vec<DiscoveredProfile> = Vec::new();
    let mut seen_paths: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Skip ~/.claude/ — it's always the Default profile

    // Step 1: Scan ~/.claude-* directories
    if let Ok(entries) = std::fs::read_dir(&home) {
        for entry in entries.flatten() {
            let name = match entry.file_name().into_string() {
                Ok(n) => n,
                Err(_) => continue,
            };
            if !name.starts_with(".claude-") || name.len() <= 8 {
                continue;
            }
            let is_dir = entry.metadata().map(|m| m.is_dir()).unwrap_or(false);
            if !is_dir {
                continue;
            }
            let full_path = format!("{}/{}", home, name);
            let suffix = &name[8..]; // after ".claude-"
            let profile_name = infer_profile_name(suffix);
            seen_paths.insert(full_path.clone());
            results.push(DiscoveredProfile {
                path: full_path,
                name: profile_name,
                source: "filesystem".to_string(),
            });
        }
    }

    // Step 1b: Check ~/.config/claude/
    let config_claude = format!("{}/.config/claude", home);
    if std::path::Path::new(&config_claude).is_dir() && !seen_paths.contains(&config_claude) {
        seen_paths.insert(config_claude.clone());
        results.push(DiscoveredProfile {
            path: config_claude,
            name: "Config".to_string(),
            source: "filesystem".to_string(),
        });
    }

    // Step 2: Parse shell configs for CLAUDE_CONFIG_DIR
    let shell_files = [".zshrc", ".bashrc", ".zprofile", ".bash_profile"];
    for filename in &shell_files {
        let filepath = format!("{}/{}", home, filename);
        let content = match std::fs::read_to_string(&filepath) {
            Ok(c) => c,
            Err(_) => continue,
        };
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('#') {
                continue;
            }
            // Match: export CLAUDE_CONFIG_DIR=... or CLAUDE_CONFIG_DIR=...
            let rest = if let Some(r) = trimmed.strip_prefix("export ") {
                r.trim()
            } else {
                trimmed
            };
            let value = if let Some(v) = rest.strip_prefix("CLAUDE_CONFIG_DIR=") {
                v
            } else {
                continue;
            };
            // Remove quotes
            let unquoted = value.trim_matches('"').trim_matches('\'');
            // Resolve $HOME and ~
            let resolved = unquoted.replace("$HOME", &home).replace("${HOME}", &home);
            let resolved = resolve_cwd(&resolved);

            if !std::path::Path::new(&resolved).is_dir() {
                continue;
            }
            if seen_paths.contains(&resolved) {
                continue;
            }
            // Infer name from directory basename
            let basename = std::path::Path::new(&resolved)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Unknown");
            let profile_name = if let Some(suffix) = basename.strip_prefix(".claude-") {
                infer_profile_name(suffix)
            } else {
                infer_profile_name(basename)
            };

            seen_paths.insert(resolved.clone());
            results.push(DiscoveredProfile {
                path: resolved,
                name: profile_name,
                source: "shell_config".to_string(),
            });
        }
    }

    Ok(results)
}

// ═══════════════════════════════════════════════════════════════════════
// Agents
// ═══════════════════════════════════════════════════════════════════════

/// Sanitize a name for use as a filename: only allow alphanumeric, dash, underscore.
fn sanitize_name(name: &str) -> Result<String, String> {
    let sanitized: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else if c == ' ' {
                '-'
            } else {
                '_'
            }
        })
        .collect();
    if sanitized.is_empty() {
        return Err("Name cannot be empty".to_string());
    }
    // Prevent path traversal
    if sanitized.contains("..") || sanitized.contains('/') || sanitized.contains('\\') {
        return Err("Invalid name".to_string());
    }
    Ok(sanitized)
}

/// Validate that a file path is within the expected directory.
fn validate_path_within(file_path: &str, expected_dir: &str) -> Result<(), String> {
    let canonical = std::fs::canonicalize(file_path)
        .map_err(|_| format!("Path does not exist: {}", file_path))?;
    let canonical_dir = std::fs::canonicalize(expected_dir)
        .map_err(|_| format!("Directory does not exist: {}", expected_dir))?;
    if !canonical.starts_with(&canonical_dir) {
        return Err("Path traversal detected".to_string());
    }
    Ok(())
}

/// Escape a YAML string value: wrap in quotes if it contains special chars.
fn yaml_escape(value: &str) -> String {
    if value.is_empty() {
        return "\"\"".to_string();
    }
    if value.contains(':')
        || value.contains('#')
        || value.contains('"')
        || value.contains('\'')
        || value.contains('\n')
        || value.contains('\t')
        || value.contains('{')
        || value.contains('}')
        || value.contains('[')
        || value.contains(']')
        || value.starts_with(' ')
        || value.ends_with(' ')
    {
        // Use double-quoted YAML string, escape internal quotes, backslashes, and control chars
        let escaped = value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\t', "\\t");
        return format!("\"{}\"", escaped);
    }
    value.to_string()
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct AgentConfig {
    name: String,
    description: String,
    model: String,
    tools: Vec<String>,
    disallowed_tools: Vec<String>,
    permission_mode: Option<String>,
    memory: Option<String>,
    max_turns: Option<u32>,
    background: Option<bool>,
    isolation: Option<String>,
    skills: Vec<String>,
    system_prompt: String,
    file_path: String,
    source: String, // "user" or "project"
}

/// Parse YAML frontmatter from a Claude agent .md file.
/// Handles both inline `[a, b, c]` and multi-line `- item` YAML lists.
fn parse_agent_file(content: &str, file_path: &str, source: &str) -> Option<AgentConfig> {
    let content = content.trim_start_matches('\n');
    if !content.starts_with("---") {
        return None;
    }
    let rest = &content[3..];
    let end = rest.find("\n---")?;
    let frontmatter = &rest[..end];
    let body = rest[end + 4..].trim_start_matches('\n');

    let mut name = String::new();
    let mut description = String::new();
    let mut model = String::new();
    let mut tools: Vec<String> = Vec::new();
    let mut disallowed_tools: Vec<String> = Vec::new();
    let mut permission_mode: Option<String> = None;
    let mut memory: Option<String> = None;
    let mut max_turns: Option<u32> = None;
    let mut background: Option<bool> = None;
    let mut isolation: Option<String> = None;
    let mut skills: Vec<String> = Vec::new();

    // Track which list field we're currently collecting multi-line items for
    let mut current_list: Option<String> = None;

    for line in frontmatter.lines() {
        let trimmed = line.trim();

        // Multi-line list item: "  - value"
        if trimmed.starts_with("- ") && current_list.is_some() {
            let item = trimmed[2..].trim().to_string();
            if !item.is_empty() {
                match current_list.as_deref() {
                    Some("tools") => tools.push(item),
                    Some("disallowedTools") => disallowed_tools.push(item),
                    Some("skills") => skills.push(item),
                    _ => {}
                }
            }
            continue;
        }

        // Key: value line — only split on first colon, and only if line starts with a key
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim();
            // For description and other text fields, take everything after first colon
            let value = value.trim().to_string();
            // Unescape quoted YAML values
            let value = if (value.starts_with('"') && value.ends_with('"'))
                || (value.starts_with('\'') && value.ends_with('\''))
            {
                value[1..value.len() - 1]
                    .replace("\\\"", "\"")
                    .replace("\\n", "\n")
                    .replace("\\\\", "\\")
            } else {
                value
            };
            current_list = None; // reset

            match key {
                "name" => name = value,
                "description" => description = value,
                "model" => model = value,
                "permissionMode" => {
                    permission_mode = if value.is_empty() { None } else { Some(value) }
                }
                "memory" => memory = if value.is_empty() { None } else { Some(value) },
                "maxTurns" => max_turns = value.parse().ok(),
                "background" => background = value.parse().ok(),
                "isolation" => isolation = if value.is_empty() { None } else { Some(value) },
                "tools" | "disallowedTools" | "skills" => {
                    let list = parse_yaml_list_value(&value);
                    if list.is_empty() && value.is_empty() {
                        // Empty value = multi-line list follows
                        current_list = Some(key.to_string());
                    } else {
                        match key {
                            "tools" => tools = list,
                            "disallowedTools" => disallowed_tools = list,
                            "skills" => skills = list,
                            _ => {}
                        }
                    }
                }
                _ => {
                    current_list = None;
                }
            }
        }
    }

    if name.is_empty() {
        return None;
    }

    Some(AgentConfig {
        name,
        description,
        model,
        tools,
        disallowed_tools,
        permission_mode,
        memory,
        max_turns,
        background,
        isolation,
        skills,
        system_prompt: body.to_string(),
        file_path: file_path.to_string(),
        source: source.to_string(),
    })
}

/// Parse a YAML value that can be either:
/// - Inline list: `[Read, Grep, Edit]` or `Read, Grep, Edit`
/// - Single value: `Read`
fn parse_yaml_list_value(value: &str) -> Vec<String> {
    let v = value.trim();
    if v.is_empty() {
        return Vec::new();
    }
    // Bracketed list: [a, b, c]
    if v.starts_with('[') && v.ends_with(']') {
        return v[1..v.len() - 1]
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }
    // Comma-separated: a, b, c
    if v.contains(',') {
        return v
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }
    // Single value
    vec![v.to_string()]
}

/// Read agents from a directory, returning parsed configs.
fn read_agents_from_dir(dir_path: &str, source: &str) -> Vec<AgentConfig> {
    let dir = match std::fs::read_dir(dir_path) {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };

    let mut agents: Vec<AgentConfig> = Vec::new();
    for entry in dir.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let file_path = path.to_string_lossy().to_string();
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        if let Some(agent) = parse_agent_file(&content, &file_path, source) {
            agents.push(agent);
        }
    }
    agents
}

/// List all configured Claude agents (user-level from ~/.claude/agents/).
#[tauri::command]
fn list_agents() -> Result<Vec<AgentConfig>, String> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    let agents_dir = format!("{}/.claude/agents", home);
    let mut agents = read_agents_from_dir(&agents_dir, "user");
    agents.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(agents)
}

/// List project-level agents from {cwd}/.claude/agents/.
#[tauri::command]
fn list_project_agents(cwd: String) -> Result<Vec<AgentConfig>, String> {
    let resolved = resolve_cwd(&cwd);
    let agents_dir = format!("{}/.claude/agents", resolved);
    let mut agents = read_agents_from_dir(&agents_dir, "project");
    agents.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(agents)
}

// save_agent and delete_agent removed — replaced by resource system
// (create_resource_in_profile, delete_resource_file)

// ── Git integration ────────────────────────────────────────────────────────

#[derive(serde::Serialize)]
struct GitFileChange {
    path: String,
    status: String, // "M", "A", "D", "R", "?"
}

/// Get the current git branch for a directory.
#[tauri::command]
fn get_git_branch(cwd: String) -> Result<Option<String>, String> {
    let resolved = resolve_cwd(&cwd);
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(&resolved)
        .output()
        .map_err(|e| format!("Failed to run git: {}", e))?;

    if !output.status.success() {
        // Not a git repo or git not available
        return Ok(None);
    }

    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if branch.is_empty() {
        return Ok(None);
    }

    // Detached HEAD — show short commit hash instead of literal "HEAD"
    if branch == "HEAD" {
        let hash = std::process::Command::new("git")
            .args(["rev-parse", "--short", "HEAD"])
            .current_dir(&resolved)
            .output()
            .ok()
            .and_then(|h| {
                let s = String::from_utf8_lossy(&h.stdout).trim().to_string();
                if s.is_empty() { None } else { Some(format!("detached@{}", s)) }
            });
        return Ok(hash);
    }

    Ok(Some(branch))
}

/// Get git status (modified/added/deleted files) for a directory.
#[tauri::command]
fn get_git_status(cwd: String) -> Result<Vec<GitFileChange>, String> {
    let resolved = resolve_cwd(&cwd);
    let output = std::process::Command::new("git")
        .args(["status", "--porcelain", "-unormal"])
        .current_dir(&resolved)
        .output()
        .map_err(|e| format!("Failed to run git: {}", e))?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut files = Vec::new();

    for line in stdout.lines() {
        if line.len() < 4 {
            continue;
        }
        // git status --porcelain format: "XY path" where X=index, Y=worktree
        let status_char = line.chars().nth(0).unwrap_or(' ');
        let worktree_char = line.chars().nth(1).unwrap_or(' ');

        // Prefer worktree status, fallback to index status
        let status = match worktree_char {
            'M' => "M",
            'D' => "D",
            'A' => "A",
            '?' => "?",
            _ => match status_char {
                'M' => "M",
                'A' => "A",
                'D' => "D",
                'R' => "R",
                '?' => "?",
                _ => continue,
            },
        };

        let raw_path = &line[3..];
        // Renamed files: "R  old -> new" — use the new path
        let path = if status == "R" {
            raw_path.split(" -> ").last().unwrap_or(raw_path).to_string()
        } else {
            raw_path.to_string()
        };
        files.push(GitFileChange {
            path,
            status: status.to_string(),
        });
    }

    // Cap at 200 files to avoid huge payloads
    files.truncate(200);
    Ok(files)
}

// ── Project & Skills ────────────────────────────────────────────────────

#[derive(serde::Serialize)]
struct ProjectConfig {
    exists: bool,
    content: String,
    cwd: String,
    config_path: String,
}

/// Read a project's CLAUDE.md (checks .claude/CLAUDE.md and root CLAUDE.md).
#[tauri::command]
fn get_project_config(cwd: String) -> Result<ProjectConfig, String> {
    let resolved = resolve_cwd(&cwd);

    // Check .claude/CLAUDE.md first, then root CLAUDE.md
    for path_suffix in &[".claude/CLAUDE.md", "CLAUDE.md"] {
        let config_path = format!("{}/{}", resolved, path_suffix);
        if let Ok(content) = std::fs::read_to_string(&config_path) {
            return Ok(ProjectConfig {
                exists: true,
                content,
                cwd: resolved,
                config_path,
            });
        }
    }

    let config_path = format!("{}/.claude/CLAUDE.md", resolved);
    Ok(ProjectConfig {
        exists: false,
        content: String::new(),
        cwd: resolved,
        config_path,
    })
}

// ── Claude Commands (.claude/commands/*.md) ─────────────────────────────

#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct CommandFile {
    name: String,
    file_path: String,
    scope: String, // "user" or "project"
    description: String,
    argument_hint: String,
    allowed_tools: Vec<String>,
    model: String,
    body: String,
}

/// Parse a Claude command .md file (YAML frontmatter + body).
fn parse_command_file(content: &str, file_path: &str, scope: &str) -> CommandFile {
    let name = std::path::Path::new(file_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();

    let content = content.trim_start_matches('\n');
    let mut description = String::new();
    let mut argument_hint = String::new();
    let mut allowed_tools: Vec<String> = Vec::new();
    let mut model = String::new();
    let mut body = content.to_string();

    if content.starts_with("---") {
        let rest = &content[3..];
        if let Some(end) = rest.find("\n---") {
            let frontmatter = &rest[..end];
            body = rest[end + 4..].trim_start_matches('\n').to_string();

            for line in frontmatter.lines() {
                // Split on first ": " (colon-space) to handle values with colons (URLs etc.)
                let parts = if let Some(pos) = line.find(": ") {
                    Some((&line[..pos], &line[pos + 2..]))
                } else if let Some((k, v)) = line.split_once(':') {
                    // Fallback for "key:" with no value
                    Some((k, v))
                } else {
                    None
                };
                if let Some((key, value)) = parts {
                    let key = key.trim();
                    let value = value.trim().to_string();
                    let value = if (value.starts_with('"') && value.ends_with('"'))
                        || (value.starts_with('\'') && value.ends_with('\''))
                    {
                        value[1..value.len() - 1].to_string()
                    } else {
                        value
                    };

                    match key {
                        "description" => description = value,
                        "argument-hint" => argument_hint = value,
                        "model" => model = value,
                        "allowed-tools" => {
                            allowed_tools = value
                                .split(',')
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .collect();
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    CommandFile {
        name,
        file_path: file_path.to_string(),
        scope: scope.to_string(),
        description,
        argument_hint,
        allowed_tools,
        model,
        body,
    }
}

/// Read all .md command files from a directory.
fn read_commands_from_dir(dir_path: &str, scope: &str) -> Vec<CommandFile> {
    let dir = match std::fs::read_dir(dir_path) {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };

    let mut commands = Vec::new();
    for entry in dir.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let file_path = path.to_string_lossy().to_string();
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        commands.push(parse_command_file(&content, &file_path, scope));
    }
    commands
}

/// Create default command files in ~/.claude/commands/ if they don't exist.
#[tauri::command]
fn ensure_default_commands() -> Result<u32, String> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    let dir = format!("{}/.claude/commands", home);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let defaults: Vec<(&str, &str)> = vec![
        ("review", r#"---
description: Code review for architecture, security, tests, and requirements
allowed-tools: Read, Grep, Glob, Bash, Agent
---

Review the current changes for:
- Architecture: patterns, structure, code quality, maintainability
- Security: vulnerabilities, input validation, secrets exposure, access control
- Testing: coverage of critical paths, edge cases, regression risks
- Requirements: scope alignment, acceptance criteria, completeness

If issues are found — fix them and review again.
Iterate until all areas pass.
"#),
        ("review-iterate", r#"---
description: Re-run review after applying fixes
allowed-tools: Read, Grep, Glob, Bash, Agent
---

Re-review all areas (architecture, security, testing, requirements).
If everything passes — report summary. If not — fix and review again.
"#),
        ("plan", r#"---
description: Plan implementation approach before coding
allowed-tools: Read, Grep, Glob, Bash
---

Plan the implementation:
- Which files will be affected
- Architectural approach and patterns
- Dependencies and risks
- Edge cases to consider

Do not write code — only the plan.
"#),
    ];

    let mut created = 0u32;
    for (name, content) in defaults {
        let path = format!("{}/{}.md", dir, name);
        if !std::path::Path::new(&path).exists() {
            std::fs::write(&path, content).map_err(|e| e.to_string())?;
            created += 1;
        }
    }
    Ok(created)
}

/// List all Claude commands: user-level (~/.claude/commands/) + project-level ({cwd}/.claude/commands/).
#[tauri::command]
fn list_commands(cwd: Option<String>) -> Result<Vec<CommandFile>, String> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    let user_dir = format!("{}/.claude/commands", home);
    let mut commands = read_commands_from_dir(&user_dir, "user");

    if let Some(cwd) = cwd {
        let resolved = resolve_cwd(&cwd);
        let project_dir = format!("{}/.claude/commands", resolved);
        if project_dir != user_dir {
            commands.extend(read_commands_from_dir(&project_dir, "project"));
        }
    }

    commands.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(commands)
}

/// Save a Claude command file (frontmatter + body).
#[tauri::command]
fn save_command(
    name: String,
    scope: String,
    cwd: Option<String>,
    description: String,
    argument_hint: String,
    allowed_tools: Vec<String>,
    model: String,
    body: String,
) -> Result<String, String> {
    let safe_name = sanitize_name(&name)?;
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());

    let dir = if scope == "project" {
        let cwd = cwd.ok_or("cwd required for project commands")?;
        let resolved = resolve_cwd(&cwd);
        format!("{}/.claude/commands", resolved)
    } else {
        format!("{}/.claude/commands", home)
    };

    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = format!("{}/{}.md", dir, safe_name);

    // Sanitize frontmatter values: strip newlines and "---" to prevent injection
    let sanitize_fm = |s: &str| -> String {
        s.replace('\n', " ").replace('\r', "").replace("---", "")
    };

    let mut frontmatter = String::from("---\n");
    if !description.is_empty() {
        frontmatter.push_str(&format!("description: {}\n", sanitize_fm(&description)));
    }
    if !argument_hint.is_empty() {
        frontmatter.push_str(&format!("argument-hint: {}\n", sanitize_fm(&argument_hint)));
    }
    if !allowed_tools.is_empty() {
        frontmatter.push_str(&format!("allowed-tools: {}\n", sanitize_fm(&allowed_tools.join(", "))));
    }
    if !model.is_empty() {
        frontmatter.push_str(&format!("model: {}\n", sanitize_fm(&model)));
    }
    frontmatter.push_str("---\n\n");

    let content = format!("{}{}", frontmatter, body);
    std::fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Delete a Claude command file.
#[tauri::command]
fn delete_command(path: String) -> Result<(), String> {
    let canon = std::fs::canonicalize(&path).map_err(|e| e.to_string())?;
    let canon_str = canon.to_string_lossy().to_string();

    // Must be a .md file
    if !canon_str.ends_with(".md") {
        return Err("Can only delete .md files".to_string());
    }

    // Validate: parent dir must be named "commands" and grandparent must be ".claude"
    let parent = canon.parent().ok_or("Invalid path")?;
    let grandparent = parent.parent().ok_or("Invalid path")?;
    let parent_name = parent.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let grandparent_name = grandparent.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if parent_name != "commands" || grandparent_name != ".claude" {
        return Err("Path is not within a .claude/commands/ directory".to_string());
    }

    // Additionally verify it's under $HOME
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    if !canon_str.starts_with(&home) {
        return Err("Path must be under home directory".to_string());
    }
    if canon.exists() {
        std::fs::remove_file(&canon).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Inject Weplex workspace context into the project's CLAUDE.local.md.
/// This file is gitignored by design (Claude Code convention), so it won't
/// pollute the shared repo. Writes only when content has changed.
#[tauri::command]
fn inject_context_block(cwd: String, context_block: String) -> Result<String, String> {
    let resolved = resolve_cwd(&cwd);

    // Path validation: must be an existing directory, no traversal
    let canonical = std::fs::canonicalize(&resolved)
        .map_err(|_| format!("Invalid project directory: {}", resolved))?;
    if !canonical.is_dir() {
        return Err(format!("Not a directory: {}", resolved));
    }
    let resolved = canonical.to_string_lossy().to_string();

    let config_path = format!("{}/CLAUDE.local.md", resolved);

    // Ensure CLAUDE.local.md is in .gitignore
    let gitignore_path = format!("{}/.gitignore", resolved);
    if std::path::Path::new(&gitignore_path).exists() {
        let gitignore = std::fs::read_to_string(&gitignore_path).unwrap_or_default();
        if !gitignore.lines().any(|l| l.trim() == "CLAUDE.local.md") {
            let separator = if gitignore.ends_with('\n') || gitignore.is_empty() { "" } else { "\n" };
            std::fs::write(&gitignore_path, format!("{}{}{}\n", gitignore, separator, "CLAUDE.local.md"))
                .map_err(|e| format!("Failed to update .gitignore: {}", e))?;
            eprintln!("[weplex] added CLAUDE.local.md to .gitignore");
        }
    }

    // Read existing content
    let existing = std::fs::read_to_string(&config_path).unwrap_or_default();

    let new_content = if let Some((before, after)) = strip_weplex_block(&existing) {
        // Block exists — replace in place, preserve surrounding content
        match (before.is_empty(), after.is_empty()) {
            (true, true) => context_block.clone(),
            (true, false) => format!("{}\n\n{}", context_block, after),
            (false, true) => format!("{}\n\n{}", before, context_block),
            (false, false) => format!("{}\n\n{}\n\n{}", before, context_block, after),
        }
    } else if existing.trim().is_empty() {
        // New file — just the block
        context_block.clone()
    } else {
        // File exists but no block — prepend
        format!("{}\n\n{}", context_block, existing)
    };

    // Skip write if content unchanged
    if existing == new_content {
        return Ok(config_path);
    }

    std::fs::write(&config_path, &new_content)
        .map_err(|e| format!("Failed to write CLAUDE.local.md: {}", e))?;

    eprintln!("[weplex] injected context into {}", config_path);
    Ok(config_path)
}

/// Find and strip the Weplex context block delimited by HTML comments.
/// Returns Some((before, after)) with trimmed surrounding content, or None if no block found.
fn strip_weplex_block(content: &str) -> Option<(String, String)> {
    let start_marker = "<!-- wplx-ctx";
    let end_marker = "<!-- /wplx-ctx -->";

    let start = content.find(start_marker)?;

    let after_start = &content[start..];
    let end = if let Some(rel_end) = after_start.find(end_marker) {
        start + rel_end + end_marker.len()
    } else {
        // Missing end marker — strip from start to end of file
        content.len()
    };

    let before = content[..start].trim_end().to_string();
    let after = content[end..].trim_start().to_string();
    Some((before, after))
}

#[derive(serde::Serialize)]
struct SkillInfo {
    name: String,
    description: String,
}

/// Read skills from a directory (each subdirectory with SKILL.md is a skill).
fn read_skills_from_dir(dir_path: &str) -> Vec<SkillInfo> {
    let dir = match std::fs::read_dir(dir_path) {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };

    let mut skills = Vec::new();
    for entry in dir.flatten() {
        if !entry.metadata().map(|m| m.is_dir()).unwrap_or(false) {
            continue;
        }
        let name = match entry.file_name().into_string() {
            Ok(n) => n,
            Err(_) => continue,
        };
        let skill_md = format!("{}/{}/SKILL.md", dir_path, name);
        let description = std::fs::read_to_string(&skill_md)
            .ok()
            .and_then(|c| {
                if let Some(rest) = c.strip_prefix("---")
                    && let Some(end) = rest.find("\n---")
                {
                    let fm = &rest[..end];
                    for line in fm.lines() {
                        if let Some((k, v)) = line.split_once(':')
                            && k.trim() == "description"
                        {
                            return Some(v.trim().to_string());
                        }
                    }
                }
                None
            })
            .unwrap_or_default();
        skills.push(SkillInfo { name, description });
    }
    skills
}

/// List available skills from both ~/.claude/skills/ and ~/.weplex/skills/.
#[tauri::command]
fn list_skills() -> Vec<SkillInfo> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    // Weplex skills first (higher priority), then Claude skills
    let mut skills = read_skills_from_dir(&format!("{}/.weplex/skills", home));
    let claude_skills = read_skills_from_dir(&format!("{}/.claude/skills", home));
    for cs in claude_skills {
        if !skills.iter().any(|s| s.name == cs.name) {
            skills.push(cs);
        }
    }
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    skills
}

/// Read the full content of a skill's SKILL.md for injection into agent prompts.
#[tauri::command]
fn read_skill_content(name: String) -> Result<String, String> {
    // Validate name to prevent path traversal
    if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        return Err("Invalid skill name".to_string());
    }

    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());

    // Check Weplex skills first, then Claude skills
    for dir in &[
        format!("{}/.weplex/skills/{}/SKILL.md", home, name),
        format!("{}/.claude/skills/{}/SKILL.md", home, name),
    ] {
        if let Ok(content) = std::fs::read_to_string(dir) {
            return Ok(content);
        }
    }

    Err(format!("Skill '{}' not found", name))
}

/// Validate store key: only alphanumeric, underscore, hyphen allowed.
/// Prevents path traversal (e.g., "../../etc/passwd").
fn validate_store_key(key: &str) -> Result<(), String> {
    if key.is_empty() || key.len() > 64 {
        return Err("Invalid store key length".to_string());
    }
    if !key
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(format!("Invalid store key: {}", key));
    }
    Ok(())
}

/// Store directory name: "stores" for release, "stores-dev" for debug.
/// Prevents dev and production from sharing/overwriting each other's data.
fn stores_dir_name() -> &'static str {
    if cfg!(debug_assertions) {
        "stores-dev"
    } else {
        "stores"
    }
}

/// Atomic write with backup rotation.
/// Writes to `appDataDir/stores/{key}.json` with `.tmp` + rename for crash safety.
#[tauri::command]
fn persist_store(app: tauri::AppHandle, key: String, value: String) -> Result<(), String> {
    validate_store_key(&key)?;

    let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let stores_dir = app_data.join(stores_dir_name());
    std::fs::create_dir_all(&stores_dir).map_err(|e| e.to_string())?;

    let path = stores_dir.join(format!("{}.json", key));
    let tmp_path = stores_dir.join(format!("{}.json.tmp", key));
    let backup_path = stores_dir.join(format!("{}.json.backup", key));

    // Write to temp file first
    std::fs::write(&tmp_path, &value).map_err(|e| e.to_string())?;

    // Rotate: current → backup (ignore error if current doesn't exist yet)
    if path.exists() {
        let _ = std::fs::rename(&path, &backup_path);
    }

    // Atomic rename: tmp → current
    std::fs::rename(&tmp_path, &path).map_err(|e| e.to_string())?;

    // Restrict permissions for sensitive keys (auth tokens)
    #[cfg(unix)]
    if key.contains("auth") {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }

    Ok(())
}

/// Load store data from file, with fallback to backup.
#[tauri::command]
fn load_store(app: tauri::AppHandle, key: String) -> Result<Option<String>, String> {
    validate_store_key(&key)?;

    let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let stores_dir = app_data.join(stores_dir_name());

    let path = stores_dir.join(format!("{}.json", key));
    let backup_path = stores_dir.join(format!("{}.json.backup", key));

    // Try primary
    if path.exists()
        && let Ok(content) = std::fs::read_to_string(&path)
        && !content.is_empty()
    {
        return Ok(Some(content));
    }

    // Fallback to backup
    if backup_path.exists()
        && let Ok(content) = std::fs::read_to_string(&backup_path)
        && !content.is_empty()
    {
        return Ok(Some(content));
    }

    Ok(None)
}

/// Save a marketplace package (agent/skill YAML) to local filesystem.
#[tauri::command]
fn save_marketplace_package(dir: String, name: String, content: String) -> Result<(), String> {
    // Whitelist dir to prevent path traversal
    if dir != "agents" && dir != "skills" {
        return Err("Invalid directory: must be 'agents' or 'skills'".to_string());
    }

    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    let target_dir = format!("{}/.weplex/{}", home, dir);
    std::fs::create_dir_all(&target_dir)
        .map_err(|e| format!("Failed to create directory: {}", e))?;

    // Sanitize filename
    let safe_name: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect();

    let ext = "yaml";
    let path = format!("{}/{}.{}", target_dir, safe_name, ext);

    std::fs::write(&path, &content)
        .map_err(|e| format!("Failed to write package: {}", e))?;

    eprintln!("[weplex] marketplace package saved: {}", path);
    Ok(())
}

// ── Plugin Management ──────────────────────────────────────────────────────

#[tauri::command]
fn list_installed_plugins() -> Vec<plugin_host::PluginInfo> {
    plugin_host::list_plugins()
}

#[tauri::command]
fn activate_plugin(plugin_id: String) -> Result<(), String> {
    plugin_host::activate_plugin(&plugin_id)
}

#[tauri::command]
fn deactivate_plugin(plugin_id: String) -> Result<(), String> {
    plugin_host::deactivate_plugin(&plugin_id)
}

// ── Browser Plugin Commands ────────────────────────────────────────────────

#[tauri::command]
fn browser_detect() -> Vec<plugins::browser::BrowserInfo> {
    plugins::browser::detect_browsers()
}

#[tauri::command]
fn browser_launch(browser: String, url: String) -> Result<serde_json::Value, String> {
    let port = plugins::browser::next_cdp_port();
    let pid = plugins::browser::launch_browser(&browser, port, &url)?;
    Ok(serde_json::json!({ "pid": pid, "port": port }))
}

/// Save a marketplace skill to ~/.weplex/skills/<name>/SKILL.md.
#[tauri::command]
fn save_marketplace_skill(name: String, content: String) -> Result<(), String> {
    let safe_name: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect();

    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    let skill_dir = format!("{}/.weplex/skills/{}", home, safe_name);
    std::fs::create_dir_all(&skill_dir)
        .map_err(|e| format!("Failed to create skill directory: {}", e))?;

    let path = format!("{}/SKILL.md", skill_dir);
    std::fs::write(&path, &content)
        .map_err(|e| format!("Failed to write skill: {}", e))?;

    eprintln!("[weplex] marketplace skill saved: {}", path);
    Ok(())
}

#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    // Strict URL validation: only https:// or http://localhost are allowed
    let is_safe = url.starts_with("https://")
        || url.starts_with("http://localhost")
        || url.starts_with("http://127.0.0.1");
    if !is_safe {
        return Err("Blocked: only https:// and http://localhost URLs are allowed".to_string());
    }
    // Reject URLs containing shell metacharacters (defense in depth)
    if url.chars().any(|c| {
        matches!(
            c,
            '`' | '$' | '|' | ';' | '&' | '\n' | '\r' | '"' | '\'' | '\\' | '<' | '>' | '(' | ')'
        )
    }) {
        return Err("Blocked: URL contains invalid characters".to_string());
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&url)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&url)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        // On Windows, use cmd /C start "" "url" — empty title + quoted URL prevents injection
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &url])
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ── MCP Commands ───────────────────────────────────────────────────────────

/// Get the path to the weplex-mcp binary.
/// In dev mode: looks in src-tauri/mcp-server/target/debug/
/// In release: looks next to the main binary (Contents/MacOS/)
#[tauri::command]
fn get_mcp_binary_path(app: tauri::AppHandle) -> Result<String, String> {
    find_mcp_binary(&app)
}

/// Register the weplex MCP server in ~/.claude.json.
/// Creates the file if it doesn't exist, adds/updates the mcpServers.weplex entry.
#[tauri::command]
fn register_mcp_in_claude(app: tauri::AppHandle) -> Result<(), String> {
    do_register_mcp_in_claude(&app)
}

/// Find the weplex-mcp binary path based on build mode and platform.
/// In production, Tauri externalBin places it next to the main executable.
fn find_mcp_binary(_app: &tauri::AppHandle) -> Result<String, String> {
    // Dev mode: check mcp-server build directory
    if cfg!(debug_assertions) {
        let dev_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("mcp-server/target/debug/weplex-mcp");
        if dev_path.exists() {
            return Ok(dev_path.to_string_lossy().to_string());
        }
        let dev_release = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("mcp-server/target/release/weplex-mcp");
        if dev_release.exists() {
            return Ok(dev_release.to_string_lossy().to_string());
        }
        return Err("weplex-mcp binary not found. Run: cd src-tauri/mcp-server && cargo build".to_string());
    }

    // Production: Tauri externalBin places sidecar next to main executable
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let sidecar_path = dir.join("weplex-mcp");
            if sidecar_path.exists() {
                return Ok(sidecar_path.to_string_lossy().to_string());
            }
        }
    }

    Err("weplex-mcp binary not found".to_string())
}

/// Generate all hook scripts at ~/.weplex/hooks/.
/// Each hook reads JSON from stdin (Claude Code hook protocol), resolves
/// the Weplex session ID, and POSTs the event to the local hook server.
fn ensure_hook_script() -> Result<(), String> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    let hooks_dir = format!("{}/.weplex/hooks", home);

    std::fs::create_dir_all(&hooks_dir)
        .map_err(|e| format!("Failed to create hooks dir: {}", e))?;

    // Shared preamble: read stdin, resolve session ID, read hook-port + auth token.
    // Uses jq for safe JSON parsing and construction (pre-installed on macOS).
    let preamble = r#"#!/bin/bash
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

    // ── PreToolUse hook ──
    let pre_tool_script = format!(
        r#"{}
# Build safe JSON payload using jq
PAYLOAD=$(echo "$INPUT" | jq -c --arg evt "pre_tool_use" --arg sid "$WEPLEX_SID" --arg cwd "$CWD_NORM" \
  '{{
    event_type: $evt,
    session_id: ($sid | tonumber),
    tool_name: (.tool_name // null),
    file_path: (.file_path // null),
    cwd: $cwd,
    tool_input: ((.tool_input // "") | tostring | .[0:500])
  }}' 2>/dev/null)

if [ -z "$PAYLOAD" ]; then exit 0; fi

curl -s -X POST "http://127.0.0.1:$PORT/hook" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d "$PAYLOAD" \
  --max-time 2 > /dev/null 2>&1

exit 0
"#,
        preamble
    );

    // ── PostToolUse hook ──
    let post_tool_script = format!(
        r#"{}
PAYLOAD=$(echo "$INPUT" | jq -c --arg evt "post_tool_use" --arg sid "$WEPLEX_SID" --arg cwd "$CWD_NORM" \
  '{{
    event_type: $evt,
    session_id: ($sid | tonumber),
    tool_name: (.tool_name // null),
    file_path: (.file_path // null),
    cwd: $cwd,
    tool_output: ((.tool_output // "") | tostring | .[0:500])
  }}' 2>/dev/null)

if [ -z "$PAYLOAD" ]; then exit 0; fi

curl -s -X POST "http://127.0.0.1:$PORT/hook" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d "$PAYLOAD" \
  --max-time 2 > /dev/null 2>&1

exit 0
"#,
        preamble
    );

    // ── Stop hook ──
    let stop_script = format!(
        r#"{}
PAYLOAD=$(echo "$INPUT" | jq -c --arg evt "stop" --arg sid "$WEPLEX_SID" --arg cwd "$CWD_NORM" \
  '{{
    event_type: $evt,
    session_id: ($sid | tonumber),
    cwd: $cwd
  }}' 2>/dev/null)

if [ -n "$PAYLOAD" ]; then
  curl -s -X POST "http://127.0.0.1:$PORT/hook" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer $TOKEN" \
    -d "$PAYLOAD" \
    --max-time 2 > /dev/null 2>&1
fi

# Check if agent provided activity notes
SUMMARY_FILE="$HOME/.weplex/summaries/${{WEPLEX_SID}}.json"
if [ -f "$SUMMARY_FILE" ]; then
  UPDATED_AT=$(jq -r '.updatedAt // 0' "$SUMMARY_FILE" 2>/dev/null || echo "0")
  NOW=$(date +%s)
  AGE=$(( NOW - UPDATED_AT ))
  if [ "$AGE" -lt 300 ]; then exit 0; fi
fi

echo "Please call the deck_update_notes tool to record what you accomplished before finishing." >&2
exit 2
"#,
        preamble
    );

    // ── SubagentStart hook ──
    let subagent_start_script = format!(
        r#"{}
PAYLOAD=$(echo "$INPUT" | jq -c --arg evt "subagent_start" --arg sid "$WEPLEX_SID" --arg cwd "$CWD_NORM" \
  '{{
    event_type: $evt,
    session_id: ($sid | tonumber),
    cwd: $cwd,
    agent_type: (.agent_type // null),
    agent_id: (.agent_id // null)
  }}' 2>/dev/null)

if [ -z "$PAYLOAD" ]; then exit 0; fi

curl -s -X POST "http://127.0.0.1:$PORT/hook" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d "$PAYLOAD" \
  --max-time 2 > /dev/null 2>&1

exit 0
"#,
        preamble
    );

    // ── SubagentStop hook ──
    let subagent_stop_script = format!(
        r#"{}
PAYLOAD=$(echo "$INPUT" | jq -c --arg evt "subagent_stop" --arg sid "$WEPLEX_SID" --arg cwd "$CWD_NORM" \
  '{{
    event_type: $evt,
    session_id: ($sid | tonumber),
    cwd: $cwd,
    agent_type: (.agent_type // null),
    agent_id: (.agent_id // null)
  }}' 2>/dev/null)

if [ -z "$PAYLOAD" ]; then exit 0; fi

curl -s -X POST "http://127.0.0.1:$PORT/hook" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d "$PAYLOAD" \
  --max-time 2 > /dev/null 2>&1

exit 0
"#,
        preamble
    );

    // ── SessionStart hook ──
    // Unlike other hooks, SessionStart reads WEPLEX_SESSION_ID from env
    // (set by PTY at spawn time) instead of session-map files.
    let session_start_script = r#"#!/bin/bash
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
"#.to_string();

    // Write all scripts
    let scripts = [
        ("session-start.sh", session_start_script),
        ("pre-tool-use.sh", pre_tool_script),
        ("post-tool-use.sh", post_tool_script),
        ("stop-hook.sh", stop_script),
        ("subagent-start.sh", subagent_start_script),
        ("subagent-stop.sh", subagent_stop_script),
    ];

    for (name, content) in &scripts {
        let path = format!("{}/{}", hooks_dir, name);
        std::fs::write(&path, content)
            .map_err(|e| format!("Failed to write {}: {}", name, e))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o700);
            std::fs::set_permissions(&path, perms)
                .map_err(|e| format!("Failed to set permissions on {}: {}", name, e))?;
        }
    }

    eprintln!("[weplex] hook scripts written to {}", hooks_dir);
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
fn write_weplex_hooks_source() -> Result<(), String> {
    let home = std::env::var("HOME")
        .map_err(|_| "HOME environment variable not set".to_string())?;
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

    eprintln!("[weplex] hooks source written to {}", source_path);
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
    let home = std::env::var("HOME")
        .map_err(|_| "HOME environment variable not set".to_string())?;
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

    eprintln!("[weplex] hooks synced to {}", settings_path);
    Ok(())
}

/// Validate that config_dir is an absolute path under $HOME.
/// Resolves symlinks to prevent symlink attacks.
fn validate_config_dir(config_dir: &str) -> Result<String, String> {
    let home = std::env::var("HOME")
        .map_err(|_| "HOME environment variable not set".to_string())?;

    // Must be absolute
    if !config_dir.starts_with('/') {
        return Err(format!("Config dir must be absolute: {}", config_dir));
    }

    // Resolve symlinks. If dir doesn't exist yet, canonicalize parent.
    let path = std::path::Path::new(config_dir);
    let canonical = if path.exists() {
        std::fs::canonicalize(path).map_err(|e| format!("Cannot resolve path: {}", e))?
    } else {
        let parent = path
            .parent()
            .ok_or_else(|| format!("No parent dir for: {}", config_dir))?;
        let canonical_parent = std::fs::canonicalize(parent)
            .map_err(|e| format!("Cannot resolve parent: {}", e))?;
        let file_name = path
            .file_name()
            .ok_or_else(|| format!("No dir name in: {}", config_dir))?;
        canonical_parent.join(file_name)
    };

    if !canonical.starts_with(&home) {
        return Err(format!(
            "Config dir must be under HOME ({}): {}",
            home, config_dir
        ));
    }

    canonical
        .to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| format!("Config dir path is not valid UTF-8: {}", config_dir))
}

/// Tauri command: sync Weplex hooks into all known profiles.
/// Called from frontend after profileStore loads from localStorage.
/// Ensures source file is fresh before syncing.
#[tauri::command]
fn sync_hooks_for_profiles(profile_config_dirs: Vec<String>) -> Result<(), String> {
    let home = std::env::var("HOME")
        .map_err(|_| "HOME environment variable not set".to_string())?;

    // Ensure source file is fresh (no race with startup thread)
    let _ = write_weplex_hooks_source();

    // Always sync default profile (~/.claude/)
    let default_dir = format!("{}/.claude", home);
    if let Err(e) = sync_hooks_to_profile(&default_dir) {
        eprintln!("[weplex] failed to sync hooks to default profile: {}", e);
    }

    // Sync each custom profile
    for config_dir in &profile_config_dirs {
        if config_dir.is_empty() {
            continue;
        }
        match validate_config_dir(config_dir) {
            Ok(validated) => {
                if let Err(e) = sync_hooks_to_profile(&validated) {
                    eprintln!("[weplex] failed to sync hooks to {}: {}", validated, e);
                }
            }
            Err(e) => eprintln!("[weplex] skipping invalid profile dir {}: {}", config_dir, e),
        }
    }

    Ok(())
}

/// Tauri command: sync Weplex hooks into a single profile.
/// Called when user creates a new profile.
#[tauri::command]
fn sync_hooks_for_profile(config_dir: String) -> Result<(), String> {
    // Ensure source file exists
    let _ = write_weplex_hooks_source();

    if config_dir.is_empty() {
        let home = std::env::var("HOME")
            .map_err(|_| "HOME environment variable not set".to_string())?;
        return sync_hooks_to_profile(&format!("{}/.claude", home));
    }

    let validated = validate_config_dir(&config_dir)?;
    sync_hooks_to_profile(&validated)
}

// ═══════════════════════════════════════════════════════════════════════
// Resources (agents, rules, skills) — cross-profile management
// Profile-first: profiles are source of truth, Weplex is viewer + copy tool.
// ═══════════════════════════════════════════════════════════════════════

/// Resolve config dir: empty string = default profile ~/.claude/.
fn resolve_config_dir(config_dir: &str) -> Result<String, String> {
    if config_dir.is_empty() {
        let home = std::env::var("HOME")
            .map_err(|_| "HOME environment variable not set".to_string())?;
        Ok(format!("{}/.claude", home))
    } else {
        validate_config_dir(config_dir)
    }
}

#[tauri::command]
fn discover_resources(
    profiles: Vec<resources::ProfileInfo>,
) -> Result<Vec<resources::UnifiedResource>, String> {
    // Validate all profile config dirs before filesystem access
    for p in &profiles {
        if let Some(ref dir) = p.config_dir {
            if !dir.is_empty() {
                validate_config_dir(dir)?;
            }
        }
    }
    resources::discover(&profiles)
}

#[tauri::command]
fn count_profile_resources(
    profiles: Vec<resources::ProfileInfo>,
) -> Result<resources::ResourceCounts, String> {
    resources::count_resources(&profiles)
}

#[tauri::command]
fn copy_resource_to_profile(
    source_path: String,
    target_config_dir: String,
    resource_type: resources::ResourceType,
    name: String,
    overwrite: bool,
) -> Result<bool, String> {
    // Validate source is under HOME
    let home = std::env::var("HOME")
        .map_err(|_| "HOME environment variable not set".to_string())?;
    let canonical = std::fs::canonicalize(&source_path)
        .map_err(|e| format!("Cannot resolve source: {}", e))?;
    let canonical_str = canonical.to_str()
        .ok_or("Source path not valid UTF-8")?;
    if !canonical_str.starts_with(&home) {
        return Err("Source path must be under HOME".to_string());
    }

    let validated_target = resolve_config_dir(&target_config_dir)?;
    resources::copy_resource(canonical_str, &validated_target, resource_type, &name, overwrite)
}

#[tauri::command]
fn copy_all_resources_to_profile(
    source_profiles: Vec<resources::ProfileInfo>,
    target_config_dir: String,
) -> Result<u32, String> {
    let validated_target = resolve_config_dir(&target_config_dir)?;
    resources::copy_all_to_profile(&source_profiles, &validated_target)
}

#[tauri::command]
fn create_resource_in_profile(
    config_dir: String,
    resource_type: resources::ResourceType,
    name: String,
    content: String,
) -> Result<String, String> {
    let validated = resolve_config_dir(&config_dir)?;
    resources::create_resource(&validated, resource_type, &name, &content)
}

#[tauri::command]
fn delete_resource_file(file_path: String) -> Result<(), String> {
    // Validate file is under HOME
    let home = std::env::var("HOME")
        .map_err(|_| "HOME environment variable not set".to_string())?;
    let canonical = std::fs::canonicalize(&file_path)
        .map_err(|e| format!("Cannot resolve path: {}", e))?;
    if !canonical.starts_with(&home) {
        return Err("File path must be under HOME".to_string());
    }
    resources::delete_resource(canonical.to_str().unwrap_or(&file_path))
}

/// Register or update the weplex MCP server entry in ~/.claude.json.
fn do_register_mcp_in_claude(app: &tauri::AppHandle) -> Result<(), String> {
    let binary_path = match find_mcp_binary(app) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[weplex] MCP registration skipped: {}", e);
            return Ok(()); // Don't fail startup if binary not found
        }
    };

    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    let claude_json_path = format!("{}/.claude.json", home);

    // Read existing config or create empty object
    let mut config: serde_json::Value = if let Ok(content) = std::fs::read_to_string(&claude_json_path) {
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    // Ensure mcpServers object exists
    if !config.get("mcpServers").is_some() {
        config["mcpServers"] = serde_json::json!({});
    }

    // Check if weplex entry exists and matches
    let current_command = config
        .get("mcpServers")
        .and_then(|s| s.get("weplex"))
        .and_then(|w| w.get("command"))
        .and_then(|c| c.as_str())
        .unwrap_or("");

    if current_command == binary_path {
        // Already up to date
        return Ok(());
    }

    // Add/update weplex entry
    config["mcpServers"]["weplex"] = serde_json::json!({
        "command": binary_path
    });

    // Write back — preserve formatting
    let json_str = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    std::fs::write(&claude_json_path, json_str).map_err(|e| e.to_string())?;

    eprintln!(
        "[weplex] MCP server registered in ~/.claude.json (binary: {})",
        binary_path
    );
    Ok(())
}

#[cfg(target_os = "macos")]
mod mac_utils {
    use tauri::Manager;

    unsafe extern "C" {
        fn objc_msgSend(obj: *mut std::ffi::c_void, sel: *mut std::ffi::c_void, ...) -> *mut std::ffi::c_void;
        fn sel_registerName(name: *const u8) -> *mut std::ffi::c_void;
    }

    pub fn set_traffic_lights(app: &tauri::AppHandle, visible: bool) {
        if let Some(window) = app.get_webview_window("main") {
            if let Ok(ns_win) = window.ns_window() {
                unsafe {
                    let ns_win = ns_win as *mut std::ffi::c_void;
                    let sel_button = sel_registerName(b"standardWindowButton:\0".as_ptr());
                    let sel_hidden = sel_registerName(b"setHidden:\0".as_ptr());
                    // 0=close, 1=miniaturize, 2=zoom
                    for i in 0u64..3 {
                        let button: *mut std::ffi::c_void = {
                            type Fn = unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, u64) -> *mut std::ffi::c_void;
                            let f: Fn = std::mem::transmute(objc_msgSend as *const ());
                            f(ns_win, sel_button, i)
                        };
                        if !button.is_null() {
                            type FnBool = unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, bool);
                            let f: FnBool = std::mem::transmute(objc_msgSend as *const ());
                            f(button, sel_hidden, !visible);
                        }
                    }
                }
            }
        }
    }
}

#[tauri::command]
fn set_traffic_lights_visible(app: tauri::AppHandle, visible: bool) {
    #[cfg(target_os = "macos")]
    mac_utils::set_traffic_lights(&app, visible);
    #[cfg(not(target_os = "macos"))]
    let _ = (app, visible);
}

fn main() {
    tauri::Builder::default()
        .manage(AppState {
            pty_manager: std::sync::Arc::new(Mutex::new(PtyManager::new())),
            ipc_pool: Mutex::new(ipc_server::IpcSocketPool::new()),
        })
        .setup(|app| {
            // Clean up stale socket files from previous crashes
            ipc_server::IpcSocketPool::cleanup_stale_socket_files();

            // Ensure summaries directory exists and clean up old files
            session_summary::ensure_summaries_dir();
            session_summary::cleanup_old_summaries();

            // Start global MCP socket for cross-session tools (MCP v2)
            {
                let state: tauri::State<AppState> = app.state();
                let pty_arc = std::sync::Arc::clone(&state.pty_manager);
                let app_handle = app.handle().clone();
                let mut pool = state.ipc_pool.lock().unwrap_or_else(|p| p.into_inner());
                match pool.start_global_socket(pty_arc, app_handle) {
                    Ok(path) => eprintln!("[weplex] global MCP socket started: {}", path),
                    Err(e) => eprintln!("[weplex] failed to start global MCP socket: {}", e),
                }
            }

            // Start hook event listener (must be before hook registration)
            let hook_handle = app.handle().clone();
            match hook_server::start_hook_server(hook_handle) {
                Ok(port) => eprintln!("[weplex] hook server started on port {}", port),
                Err(e) => eprintln!("[weplex] failed to start hook server: {}", e),
            }

            // Register MCP server in ~/.claude.json, generate hook scripts and source.
            // Hook sync into profile settings.json happens from frontend after
            // profileStore loads (via sync_hooks_for_profiles command).
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let _ = do_register_mcp_in_claude(&handle);
                let _ = ensure_hook_script();
                let _ = write_weplex_hooks_source();
                // Clean stale session-map from previous runs
                let _ = clean_session_map();
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            create_pty,
            write_pty,
            resize_pty,
            kill_pty,
            get_claude_usage,
            get_claude_state,
            list_dirs,
            discover_profiles,
            list_agents,
            list_project_agents,
            ensure_default_commands,
            list_commands,
            save_command,
            delete_command,
            get_project_config,
            get_git_branch,
            get_git_status,
            inject_context_block,
            list_skills,
            read_skill_content,
            persist_store,
            load_store,
            oauth_server::start_oauth_server,
            open_url,
            save_marketplace_package,
            save_marketplace_skill,
            list_installed_plugins,
            activate_plugin,
            deactivate_plugin,
            browser_detect,
            browser_launch,
            keychain::keychain_save,
            keychain::keychain_load,
            keychain::keychain_delete,
            secure_store::secure_store_save,
            secure_store::secure_store_load,
            secure_store::secure_store_delete,
            get_mcp_binary_path,
            register_mcp_in_claude,
            sync_hooks_for_profiles,
            sync_hooks_for_profile,
            discover_resources,
            count_profile_resources,
            copy_resource_to_profile,
            copy_all_resources_to_profile,
            create_resource_in_profile,
            delete_resource_file,
            set_traffic_lights_visible,
            get_session_summary,
        ])
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_notification::init())
        .build(tauri::generate_context!())
        .expect("error while building Weplex")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                // Clean up all IPC sockets on app exit
                let state: State<AppState> = app.state();
                let mut pool = state
                    .ipc_pool
                    .lock()
                    .unwrap_or_else(|p| p.into_inner());
                pool.cleanup_all();

                // Clean up hook server files
                hook_server::cleanup_hook_files();
            }
        });
}

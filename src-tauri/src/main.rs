// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(clippy::too_many_arguments)]

mod ipc_server;
mod keychain;
mod oauth_server;
mod pipeline_engine;
mod pipeline_parser;
mod pty_manager;
mod secure_store;
mod weplex_agents;

use pipeline_engine::PipelineEngine;
use pty_manager::PtyManager;
use std::io::BufRead;
use std::sync::Mutex;
use tauri::{Manager, State};

struct AppState {
    pty_manager: Mutex<PtyManager>,
    pipeline_engine: std::sync::Arc<Mutex<PipelineEngine>>,
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
    let mut manager = state.pty_manager.lock().map_err(|e| e.to_string())?;
    manager
        .create(session_id, cols, rows, command, cwd, env_vars, app)
        .map_err(|e| e.to_string())
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

/// Find a Claude session file CREATED after a given timestamp (ms since epoch).
/// Uses file birthtime (macOS) to avoid picking up existing sessions from other terminals.
#[tauri::command]
fn get_new_claude_session(cwd: String, after_epoch_ms: u64) -> Result<Option<String>, String> {
    let sessions_dir = claude_sessions_dir(&cwd);
    let dir = match std::fs::read_dir(&sessions_dir) {
        Ok(d) => d,
        Err(_) => {
            return Ok(None);
        }
    };

    let after = std::time::UNIX_EPOCH + std::time::Duration::from_millis(after_epoch_ms);

    let mut newest: Option<(String, std::time::SystemTime)> = None;
    for entry in dir.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("jsonl")
            && let Ok(meta) = path.metadata()
            && let Ok(created) = meta.created()
            && created > after
        {
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            if newest.is_none() || created > newest.as_ref().unwrap().1 {
                newest = Some((stem, created));
            }
        }
    }

    Ok(newest.map(|(id, _)| id))
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
// Agents & Pipelines
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

/// Save an agent file to ~/.claude/agents/{name}.md
#[tauri::command]
fn save_agent(agent: AgentConfig) -> Result<String, String> {
    let safe_name = sanitize_name(&agent.name)?;
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    let agents_dir = format!("{}/.claude/agents", home);
    std::fs::create_dir_all(&agents_dir).map_err(|e| e.to_string())?;

    let path = format!("{}/{}.md", agents_dir, safe_name);

    let mut frontmatter = format!(
        "---\nname: {}\ndescription: {}\n",
        yaml_escape(&agent.name),
        yaml_escape(&agent.description)
    );
    if !agent.model.is_empty() {
        frontmatter.push_str(&format!("model: {}\n", yaml_escape(&agent.model)));
    }
    if !agent.tools.is_empty() {
        let escaped: Vec<String> = agent.tools.iter().map(|t| yaml_escape(t)).collect();
        frontmatter.push_str(&format!("tools: [{}]\n", escaped.join(", ")));
    }
    if !agent.disallowed_tools.is_empty() {
        let escaped: Vec<String> = agent
            .disallowed_tools
            .iter()
            .map(|t| yaml_escape(t))
            .collect();
        frontmatter.push_str(&format!("disallowedTools: [{}]\n", escaped.join(", ")));
    }
    if let Some(ref pm) = agent.permission_mode {
        frontmatter.push_str(&format!("permissionMode: {}\n", yaml_escape(pm)));
    }
    if let Some(ref mem) = agent.memory {
        frontmatter.push_str(&format!("memory: {}\n", yaml_escape(mem)));
    }
    if let Some(mt) = agent.max_turns {
        frontmatter.push_str(&format!("maxTurns: {}\n", mt));
    }
    if let Some(true) = agent.background {
        frontmatter.push_str("background: true\n");
    }
    if let Some(ref iso) = agent.isolation {
        frontmatter.push_str(&format!("isolation: {}\n", yaml_escape(iso)));
    }
    if !agent.skills.is_empty() {
        frontmatter.push_str("skills:\n");
        for skill in &agent.skills {
            frontmatter.push_str(&format!("  - {}\n", yaml_escape(skill)));
        }
    }
    frontmatter.push_str("---\n");

    let content = format!("{}\n{}", frontmatter, agent.system_prompt);
    std::fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Delete an agent file.
#[tauri::command]
fn delete_agent(name: String) -> Result<(), String> {
    let safe_name = sanitize_name(&name)?;
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    let agents_dir = format!("{}/.claude/agents", home);
    let path = format!("{}/{}.md", agents_dir, safe_name);
    // Verify path is within agents dir before deleting
    if std::path::Path::new(&path).exists() {
        validate_path_within(&path, &agents_dir)?;
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ── Pipeline types ──────────────────────────────────────────────────────

// Pipeline types are now in pipeline_parser module.
// Re-export for Tauri command compatibility.
use pipeline_parser::{PipelineConfig, PipelineStage};

// ── Pipeline CRUD (delegates to pipeline_parser module) ─────────────────────

#[tauri::command]
fn list_pipelines() -> Result<Vec<PipelineConfig>, String> {
    pipeline_parser::list()
}

#[tauri::command]
fn save_pipeline(
    name: String,
    description: String,
    stages: Vec<PipelineStage>,
    old_file_path: Option<String>,
) -> Result<String, String> {
    pipeline_parser::save(
        &name,
        &description,
        &stages,
        &std::collections::HashMap::new(),
        old_file_path.as_deref(),
    )
}

#[tauri::command]
fn delete_pipeline(file_path: String) -> Result<(), String> {
    pipeline_parser::delete(&file_path)
}

#[tauri::command]
fn generate_pipeline_instructions(file_path: String, task: String) -> Result<String, String> {
    pipeline_parser::generate_instructions(&file_path, &task)
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

#[derive(serde::Serialize)]
struct SkillInfo {
    name: String,
    description: String,
}

/// List available skills from ~/.claude/skills/
#[tauri::command]
fn list_skills() -> Vec<SkillInfo> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    let skills_dir = format!("{}/.claude/skills", home);

    let dir = match std::fs::read_dir(&skills_dir) {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };

    let mut skills: Vec<SkillInfo> = Vec::new();
    for entry in dir.flatten() {
        if !entry.metadata().map(|m| m.is_dir()).unwrap_or(false) {
            continue;
        }
        let name = match entry.file_name().into_string() {
            Ok(n) => n,
            Err(_) => continue,
        };
        // Try to read SKILL.md description
        let skill_md = format!("{}/{}/SKILL.md", skills_dir, name);
        let description = std::fs::read_to_string(&skill_md)
            .ok()
            .and_then(|c| {
                // Try to extract description from frontmatter
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

    skills.sort_by(|a, b| a.name.cmp(&b.name));
    skills
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

// ── Weplex Agents (YAML, agent-agnostic) ──────────────────────────────────────

#[tauri::command]
fn list_weplex_agents() -> Result<Vec<weplex_agents::WeplexAgent>, String> {
    weplex_agents::list()
}

#[tauri::command]
fn save_weplex_agent(agent: weplex_agents::WeplexAgent) -> Result<String, String> {
    weplex_agents::save(&agent)
}

#[tauri::command]
fn delete_weplex_agent(name: String) -> Result<(), String> {
    weplex_agents::delete(&name)
}

// ── Pipeline Engine ─────────────────────────────────────────────────────────

#[tauri::command]
fn start_pipeline(
    state: State<AppState>,
    app: tauri::AppHandle,
    pipeline_file: String,
    task: String,
    cwd: String,
    env_vars: Option<std::collections::HashMap<String, String>>,
) -> Result<String, String> {
    // Phase 1: Prepare run (parse config, create run record) — no thread spawned yet
    let prepared = {
        let mut engine = state.pipeline_engine.lock().map_err(|e| e.to_string())?;
        engine.prepare_run(
            &pipeline_file,
            &task,
            &cwd,
            env_vars.unwrap_or_default(),
            &app,
        )?
    };

    let run_id = prepared.run_id.clone();

    // Phase 2: Start MCP IPC socket BEFORE launching the orchestrator thread,
    // so the socket is ready when the first stage agent tries to connect
    {
        let engine_arc = std::sync::Arc::clone(&state.pipeline_engine);
        let mut pool = state.ipc_pool.lock().map_err(|e| e.to_string())?;
        if let Err(e) = pool.start_run_socket(run_id.clone(), engine_arc, app.clone()) {
            eprintln!("[weplex] Failed to start MCP socket for run {}: {}", run_id, e);
        }
    }

    // Phase 3: Launch orchestrator thread (now socket is guaranteed ready)
    {
        let engine_arc = std::sync::Arc::clone(&state.pipeline_engine);
        PipelineEngine::launch_run(prepared, engine_arc, app.clone());
    }

    // Phase 4: Schedule socket cleanup when the pipeline run finishes
    {
        let engine_arc = std::sync::Arc::clone(&state.pipeline_engine);
        let run_id_clone = run_id.clone();
        let app_for_cleanup = app;

        std::thread::spawn(move || {
            // Poll until the run is no longer "running"
            loop {
                std::thread::sleep(std::time::Duration::from_secs(2));
                let eng = engine_arc
                    .lock()
                    .unwrap_or_else(|p| p.into_inner());
                if let Some(run) = eng.get_run(&run_id_clone) {
                    if run.status != pipeline_engine::RunStatus::Running {
                        break;
                    }
                } else {
                    break;
                }
            }
            // Stop the socket via app state
            let state: State<AppState> = app_for_cleanup.state();
            let mut pool = state
                .ipc_pool
                .lock()
                .unwrap_or_else(|p| p.into_inner());
            pool.stop_run_socket(&run_id_clone);
        });
    }

    Ok(run_id)
}

#[tauri::command]
fn cancel_pipeline(state: State<AppState>, run_id: String) -> Result<(), String> {
    let mut engine = state.pipeline_engine.lock().map_err(|e| e.to_string())?;
    engine.cancel_run(&run_id)
}

#[tauri::command]
fn get_pipeline_run(
    state: State<AppState>,
    run_id: String,
) -> Result<Option<pipeline_engine::PipelineRun>, String> {
    let engine = state.pipeline_engine.lock().map_err(|e| e.to_string())?;
    Ok(engine.get_run(&run_id))
}

#[tauri::command]
fn list_pipeline_runs(state: State<AppState>) -> Result<Vec<pipeline_engine::PipelineRun>, String> {
    let engine = state.pipeline_engine.lock().map_err(|e| e.to_string())?;
    Ok(engine.list_runs())
}

#[tauri::command]
fn get_stage_artifact(
    state: State<AppState>,
    run_id: String,
    stage_name: String,
) -> Result<Option<String>, String> {
    let engine = state.pipeline_engine.lock().map_err(|e| e.to_string())?;
    Ok(engine.get_artifact(&run_id, &stage_name))
}

/// Open a URL in the system default browser.
/// Only allows https:// and http://localhost URLs to prevent command injection.
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

/// Validate run_id to prevent path traversal in socket paths.
fn validate_run_id(run_id: &str) -> Result<(), String> {
    if run_id.is_empty()
        || !run_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err("Invalid run_id format".to_string());
    }
    Ok(())
}

/// Start a scoped IPC socket for a pipeline run. Returns the socket path.
#[tauri::command]
fn start_mcp_for_run(
    state: State<AppState>,
    app: tauri::AppHandle,
    run_id: String,
) -> Result<String, String> {
    validate_run_id(&run_id)?;
    let engine_arc = std::sync::Arc::clone(&state.pipeline_engine);
    let mut pool = state.ipc_pool.lock().map_err(|e| e.to_string())?;
    pool.start_run_socket(run_id, engine_arc, app)
}

/// Stop and clean up an IPC socket for a pipeline run.
#[tauri::command]
fn stop_mcp_for_run(state: State<AppState>, run_id: String) -> Result<(), String> {
    validate_run_id(&run_id)?;
    let mut pool = state.ipc_pool.lock().map_err(|e| e.to_string())?;
    pool.stop_run_socket(&run_id);
    Ok(())
}

/// Store an MCP artifact for a stage (called from frontend for prefetching).
#[tauri::command]
fn set_run_artifact(
    state: State<AppState>,
    run_id: String,
    stage_name: String,
    artifact: String,
) -> Result<(), String> {
    validate_run_id(&run_id)?;
    let mut engine = state
        .pipeline_engine
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    engine.set_mcp_artifact(&run_id, &stage_name, &artifact);
    Ok(())
}

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

fn main() {
    tauri::Builder::default()
        .manage(AppState {
            pty_manager: Mutex::new(PtyManager::new()),
            pipeline_engine: std::sync::Arc::new(Mutex::new(PipelineEngine::new())),
            ipc_pool: Mutex::new(ipc_server::IpcSocketPool::new()),
        })
        .setup(|app| {
            // Clean up stale socket files from previous crashes
            ipc_server::IpcSocketPool::cleanup_stale_socket_files();

            // Register MCP server in ~/.claude.json
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let _ = do_register_mcp_in_claude(&handle);
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            create_pty,
            write_pty,
            resize_pty,
            kill_pty,
            get_new_claude_session,
            get_claude_usage,
            get_claude_state,
            list_dirs,
            discover_profiles,
            list_agents,
            list_project_agents,
            save_agent,
            delete_agent,
            list_pipelines,
            save_pipeline,
            delete_pipeline,
            generate_pipeline_instructions,
            get_project_config,
            list_skills,
            persist_store,
            load_store,
            list_weplex_agents,
            save_weplex_agent,
            delete_weplex_agent,
            start_pipeline,
            cancel_pipeline,
            get_pipeline_run,
            list_pipeline_runs,
            get_stage_artifact,
            oauth_server::start_oauth_server,
            open_url,
            keychain::keychain_save,
            keychain::keychain_load,
            keychain::keychain_delete,
            secure_store::secure_store_save,
            secure_store::secure_store_load,
            secure_store::secure_store_delete,
            start_mcp_for_run,
            stop_mcp_for_run,
            set_run_artifact,
            get_mcp_binary_path,
            register_mcp_in_claude,
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
            }
        });
}

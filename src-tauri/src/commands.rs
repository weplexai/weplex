/// Claude commands (.claude/commands/*.md) parsing and management.

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct CommandFile {
    pub name: String,
    pub file_path: String,
    pub scope: String, // "user" or "project"
    pub description: String,
    pub argument_hint: String,
    pub allowed_tools: Vec<String>,
    pub model: String,
    pub body: String,
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
///
/// Each default ships with a sibling `<name>.weplex.yaml` cross-agent
/// manifest. The sidecar is written ONLY when the .md is freshly created
/// — we never overwrite a user-edited file. If sidecar write fails we
/// log a warning but keep the .md (backward-compat: .md alone still
/// works as Claude-only).
#[tauri::command]
pub fn ensure_default_commands() -> Result<u32, String> {
    let home = crate::utils::get_home();
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

    // Sidecar manifests for cross-agent rendering. Mirrors the .md
    // ordering above so we can match by name. Each manifest:
    // - id == name (matches filename basename, validated by manifest::load)
    // - claude.source points at the sibling .md (Claude reads it directly)
    // - codex/cursor get a section heading; opencode gets the default
    //   per-id fragment file.
    let default_sidecars: Vec<(&str, &str)> = vec![
        (
            "review",
            r#"id: review
version: 1.0.0
author: weplex
agents:
  claude:
    source: ./review.md
  codex:
    section: Code Review
  cursor:
    section: Code Review
  opencode: {}
permissions:
  - read_files
  - run_bash
mcp_servers: []
"#,
        ),
        (
            "review-iterate",
            r#"id: review-iterate
version: 1.0.0
author: weplex
agents:
  claude:
    source: ./review-iterate.md
  codex:
    section: Iterate Review
  cursor:
    section: Iterate Review
  opencode: {}
permissions:
  - read_files
  - run_bash
mcp_servers: []
"#,
        ),
        (
            "plan",
            r#"id: plan
version: 1.0.0
author: weplex
agents:
  claude:
    source: ./plan.md
  codex:
    section: Implementation Plan
  cursor:
    section: Implementation Plan
  opencode: {}
permissions:
  - read_files
mcp_servers: []
"#,
        ),
    ];

    let sidecar_for = |name: &str| -> Option<&'static str> {
        for (n, body) in &default_sidecars {
            if *n == name {
                return Some(*body);
            }
        }
        None
    };

    let mut created = 0u32;
    for (name, content) in defaults {
        let path = format!("{}/{}.md", dir, name);
        if !std::path::Path::new(&path).exists() {
            std::fs::write(&path, content).map_err(|e| e.to_string())?;
            created += 1;
        }

        // Self-heal sidecar manifests independently from the .md write.
        // If a previous run created the .md but failed (or was skipped)
        // for the sidecar — e.g. the .weplex.yaml story was added in a
        // later release, or the sidecar was deleted by hand — write it
        // now. Best-effort: never fail the whole call on a sidecar
        // write error, the .md alone still works as Claude-only.
        if let Some(sidecar_body) = sidecar_for(name) {
            let sidecar_path = format!("{}/{}.weplex.yaml", dir, name);
            if !std::path::Path::new(&sidecar_path).exists() {
                if let Err(e) = std::fs::write(&sidecar_path, sidecar_body) {
                    log::warn!(
                        "ensure_default_commands: failed to write sidecar {}: {}",
                        sidecar_path,
                        e
                    );
                }
            }
        }
    }
    Ok(created)
}

/// List all Claude commands: user-level (~/.claude/commands/) + project-level ({cwd}/.claude/commands/).
#[tauri::command]
pub fn list_commands(cwd: Option<String>) -> Result<Vec<CommandFile>, String> {
    let home = crate::utils::get_home();
    let user_dir = format!("{}/.claude/commands", home);
    let mut commands = read_commands_from_dir(&user_dir, "user");

    if let Some(cwd) = cwd {
        let resolved = crate::utils::resolve_cwd(&cwd);
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
pub fn save_command(
    name: String,
    scope: String,
    cwd: Option<String>,
    description: String,
    argument_hint: String,
    allowed_tools: Vec<String>,
    model: String,
    body: String,
) -> Result<String, String> {
    let safe_name = crate::utils::sanitize_name(&name)?;
    let home = crate::utils::get_home();

    let dir = if scope == "project" {
        let cwd = cwd.ok_or("cwd required for project commands")?;
        let resolved = crate::utils::resolve_cwd(&cwd);
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
pub fn delete_command(path: String) -> Result<(), String> {
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
    let home = crate::utils::get_home();
    if !canon_str.starts_with(&home) {
        return Err("Path must be under home directory".to_string());
    }
    if canon.exists() {
        std::fs::remove_file(&canon).map_err(|e| e.to_string())?;
    }
    Ok(())
}

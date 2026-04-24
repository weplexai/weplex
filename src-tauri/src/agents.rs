/// Agent configuration parsing and listing.

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct AgentConfig {
    pub name: String,
    pub description: String,
    pub model: String,
    pub tools: Vec<String>,
    pub disallowed_tools: Vec<String>,
    pub permission_mode: Option<String>,
    pub memory: Option<String>,
    pub max_turns: Option<u32>,
    pub background: Option<bool>,
    pub isolation: Option<String>,
    pub skills: Vec<String>,
    pub system_prompt: String,
    pub file_path: String,
    pub source: String, // "user" or "project"
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
                    let list = crate::yaml::parse_yaml_list_value(&value);
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
pub fn list_agents() -> Result<Vec<AgentConfig>, String> {
    let home = crate::utils::get_home();
    let agents_dir = format!("{}/.claude/agents", home);
    let mut agents = read_agents_from_dir(&agents_dir, "user");
    agents.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(agents)
}

/// List project-level agents from {cwd}/.claude/agents/.
#[tauri::command]
pub fn list_project_agents(cwd: String) -> Result<Vec<AgentConfig>, String> {
    let resolved = crate::utils::resolve_cwd(&cwd);
    let agents_dir = format!("{}/.claude/agents", resolved);
    let mut agents = read_agents_from_dir(&agents_dir, "project");
    agents.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(agents)
}

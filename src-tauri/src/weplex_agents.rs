use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Allowlist of trusted agent binaries that Weplex is allowed to spawn.
const ALLOWED_BINARIES: &[&str] = &["claude", "codex", "aider", "gemini", "opencode", "crush"];

/// Environment variable names that must never be overridden by agent configs.
pub const DENIED_ENV_VARS: &[&str] = &[
    "PATH",
    "HOME",
    "LD_PRELOAD",
    "DYLD_INSERT_LIBRARIES",
    "SHELL",
    "LD_LIBRARY_PATH",
];

/// Check whether a binary name is in the trusted allowlist.
pub fn is_trusted_binary(binary: &str) -> bool {
    // Extract the basename in case a full path is given
    let basename = std::path::Path::new(binary)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(binary);
    ALLOWED_BINARIES.contains(&basename)
}

/// Weplex agent — agent-agnostic format with `binary` field.
/// Stored as YAML in ~/.weplex/agents/*.yaml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeplexAgent {
    pub name: String,
    pub description: String,
    /// CLI binary: "claude", "codex", "aider", "gemini", or a full path
    pub binary: String,
    /// Model hint (binary-specific, e.g. "opus", "sonnet", "gpt-4")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// System prompt / instructions for the agent
    #[serde(default)]
    pub prompt: String,
    /// Optional one-shot command template override.
    /// Placeholders: {prompt}, {model}
    /// Default templates are used per binary if not set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub one_shot: Option<String>,
    /// Extra environment variables passed to the process
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, String>,

    // -- Runtime fields (not in YAML) --
    /// Path to the YAML file — skip when deserializing from YAML, include in Tauri IPC
    #[serde(skip_deserializing)]
    pub file_path: String,
}

fn agents_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    PathBuf::from(format!("{}/.weplex/agents", home))
}

pub fn list() -> Result<Vec<WeplexAgent>, String> {
    let dir = agents_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut agents = Vec::new();
    let entries = std::fs::read_dir(&dir).map_err(|e| e.to_string())?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("yaml")
            && path.extension().and_then(|e| e.to_str()) != Some("yml")
        {
            continue;
        }

        match read_agent_file(&path) {
            Ok(agent) => agents.push(agent),
            Err(e) => eprintln!("Failed to parse {:?}: {}", path, e),
        }
    }

    agents.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(agents)
}

fn read_agent_file(path: &std::path::Path) -> Result<WeplexAgent, String> {
    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut agent: WeplexAgent = serde_yaml::from_str(&content).map_err(|e| e.to_string())?;
    agent.file_path = path.to_string_lossy().to_string();
    Ok(agent)
}

pub fn save(agent: &WeplexAgent) -> Result<String, String> {
    let dir = agents_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    // Sanitize name for filename
    let filename: String = agent
        .name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let path = dir.join(format!("{}.yaml", filename));

    let yaml = serde_yaml::to_string(agent).map_err(|e| e.to_string())?;
    std::fs::write(&path, yaml).map_err(|e| e.to_string())?;

    Ok(path.to_string_lossy().to_string())
}

pub fn delete(name: &str) -> Result<(), String> {
    let dir = agents_dir();
    let entries = std::fs::read_dir(&dir).map_err(|e| e.to_string())?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("yaml")
            && path.extension().and_then(|e| e.to_str()) != Some("yml")
        {
            continue;
        }
        if let Ok(agent) = read_agent_file(&path)
            && agent.name == name
        {
            std::fs::remove_file(&path).map_err(|e| e.to_string())?;
            return Ok(());
        }
    }

    Err(format!("Agent '{}' not found", name))
}

/// Resolve a WeplexAgent + runtime context into a command + args for one-shot execution.
/// Returns (binary, args, full_prompt).
pub fn resolve_command(
    agent: &WeplexAgent,
    role: &str,
    task: &str,
    artifacts: &str,
    skill_contents: &[String],
) -> (String, Vec<String>, String) {
    // Build the full prompt
    let mut prompt = String::new();

    if !agent.prompt.is_empty() {
        prompt.push_str(&agent.prompt);
        prompt.push_str("\n\n");
    }

    // Inject skill knowledge into prompt
    if !skill_contents.is_empty() {
        prompt.push_str("## Skills & Knowledge\n\n");
        for content in skill_contents {
            prompt.push_str(content);
            prompt.push_str("\n\n---\n\n");
        }
    }

    if !artifacts.is_empty() {
        prompt.push_str("## Context from previous stages\n\n");
        prompt.push_str(artifacts);
        prompt.push_str("\n\n");
    }

    prompt.push_str("## Task\n\n");
    prompt.push_str(task);
    prompt.push_str("\n\n");

    if !role.is_empty() {
        prompt.push_str("## Your Role in This Stage\n\n");
        prompt.push_str(role);
    }

    // Custom template override
    if let Some(ref template) = agent.one_shot {
        let cmd_str = template
            .replace("{prompt}", &prompt)
            .replace("{model}", agent.model.as_deref().unwrap_or(""));
        let parts: Vec<String> = shell_split(&cmd_str);
        if parts.is_empty() {
            return (agent.binary.clone(), vec![], prompt);
        }
        return (parts[0].clone(), parts[1..].to_vec(), prompt);
    }

    // Default templates per binary
    let binary = &agent.binary;
    let args = match binary.as_str() {
        "claude" => {
            let mut a = vec!["-p".to_string()];
            if let Some(ref model) = agent.model {
                a.push("--model".to_string());
                a.push(model.clone());
            }
            a
        }
        "codex" => {
            let mut a = vec!["--quiet".to_string()];
            if let Some(ref model) = agent.model {
                a.push("--model".to_string());
                a.push(model.clone());
            }
            a
        }
        "aider" => {
            let mut a = vec!["--message".to_string(), prompt.clone(), "--yes".to_string()];
            if let Some(ref model) = agent.model {
                a.push("--model".to_string());
                a.push(model.clone());
            }
            a
        }
        "gemini" => {
            vec![]
        }
        _ => vec![],
    };

    (binary.clone(), args, prompt)
}

/// Minimal shell-like string splitting (handles quotes).
fn shell_split(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;
    let mut quote_char = ' ';

    for ch in s.chars() {
        if in_quote {
            if ch == quote_char {
                in_quote = false;
            } else {
                current.push(ch);
            }
        } else if ch == '"' || ch == '\'' {
            in_quote = true;
            quote_char = ch;
        } else if ch.is_whitespace() {
            if !current.is_empty() {
                parts.push(current.clone());
                current.clear();
            }
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// A single pipeline stage (sequential or parallel sub-stage).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStage {
    /// Unique stage identifier (used in `receives`). Auto-generated from agent if missing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Agent name from ~/.weplex/agents/
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// Runtime instruction for this stage
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// List of stage names whose output becomes context for this stage
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub receives: Vec<String>,
    /// If true, stage failure doesn't stop the pipeline
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub optional: Option<bool>,
    /// If present, sub-stages run concurrently
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parallel: Option<Vec<PipelineStage>>,
    /// Owner (team member email) for collaborative pipeline delegation
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
}

/// Visual layout positions (optional, for canvas editor).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LayoutPosition {
    pub x: f64,
    pub y: f64,
}

/// Full pipeline definition stored as YAML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub stages: Vec<PipelineStage>,
    /// Visual positions for canvas editor
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub layout: HashMap<String, LayoutPosition>,
    /// Path to the YAML file — skip when deserializing from YAML, but include in Tauri IPC
    #[serde(skip_deserializing)]
    pub file_path: String,
}

fn pipelines_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    PathBuf::from(format!("{}/.weplex/pipelines", home))
}

pub fn parse(content: &str, file_path: &str) -> Result<PipelineConfig, String> {
    let mut config: PipelineConfig =
        serde_yaml::from_str(content).map_err(|e| format!("YAML parse error: {}", e))?;
    config.file_path = file_path.to_string();

    // Auto-generate stage names from agent if missing
    auto_name_stages(&mut config.stages, &mut 0);

    Ok(config)
}

/// Ensure every stage has a name. If missing, use agent name + index for uniqueness.
fn auto_name_stages(stages: &mut [PipelineStage], counter: &mut usize) {
    for stage in stages.iter_mut() {
        if stage.name.is_none() {
            if let Some(ref agent) = stage.agent {
                stage.name = Some(agent.clone());
            } else {
                *counter += 1;
                stage.name = Some(format!("stage-{}", counter));
            }
        }
        if let Some(ref mut parallel) = stage.parallel {
            auto_name_stages(parallel, counter);
        }
    }

    // Deduplicate names within this level
    let mut seen: HashMap<String, usize> = HashMap::new();
    for stage in stages.iter_mut() {
        if let Some(ref name) = stage.name {
            let count = seen.entry(name.clone()).or_insert(0);
            *count += 1;
            if *count > 1 {
                stage.name = Some(format!("{}-{}", name, count));
            }
        }
    }
}

pub fn list() -> Result<Vec<PipelineConfig>, String> {
    let dir = pipelines_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut pipelines = Vec::new();
    let entries = std::fs::read_dir(&dir).map_err(|e| e.to_string())?;

    for entry in entries.flatten() {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str());
        if ext != Some("yaml") && ext != Some("yml") {
            continue;
        }

        let file_path = path.to_string_lossy().to_string();
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        match parse(&content, &file_path) {
            Ok(config) => pipelines.push(config),
            Err(e) => eprintln!("Failed to parse {:?}: {}", path, e),
        }
    }

    pipelines.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(pipelines)
}

pub fn save(
    name: &str,
    description: &str,
    stages: &[PipelineStage],
    layout: &HashMap<String, LayoutPosition>,
    old_file_path: Option<&str>,
) -> Result<String, String> {
    let dir = pipelines_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let slug: String = name
        .to_lowercase()
        .replace(' ', "-")
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect();

    if slug.is_empty() {
        return Err("Invalid pipeline name".to_string());
    }

    let path = dir.join(format!("{}.yaml", slug));

    // Clean up old file on rename
    if let Some(old_path) = old_file_path
        && !old_path.is_empty()
    {
        let old = std::path::Path::new(old_path);
        if old.exists() && old != path.as_path() {
            // Verify old path is inside pipelines dir
            if old.starts_with(&dir) {
                let _ = std::fs::remove_file(old);
            }
        }
    }

    let config = PipelineConfig {
        name: name.to_string(),
        description: description.to_string(),
        stages: stages.to_vec(),
        layout: layout.clone(),
        file_path: String::new(),
    };

    let yaml = serde_yaml::to_string(&config).map_err(|e| e.to_string())?;
    std::fs::write(&path, yaml).map_err(|e| e.to_string())?;

    Ok(path.to_string_lossy().to_string())
}

pub fn delete(file_path: &str) -> Result<(), String> {
    let dir = pipelines_dir();
    let path = std::path::Path::new(file_path);
    if path.exists() && path.starts_with(&dir) {
        std::fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn generate_instructions(file_path: &str, task: &str) -> Result<String, String> {
    let dir = pipelines_dir();
    let path = std::path::Path::new(file_path);
    if !path.starts_with(&dir) {
        return Err("Invalid pipeline path".to_string());
    }

    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let pipeline = parse(&content, file_path)?;

    let mut out = format!(
        "# Pipeline: {}\n\n{}\n\n## Task\n{}\n\n## Pipeline Stages\n\nExecute the following stages sequentially using the Agent tool. Each stage invokes a specialized sub-agent. Pass context from each stage to the next.\n\n",
        pipeline.name, pipeline.description, task
    );

    for (i, stage) in pipeline.stages.iter().enumerate() {
        if let Some(ref parallel) = stage.parallel {
            out.push_str(&format!("### Stage {} (parallel)\n", i + 1));
            out.push_str("Run these sub-agents in parallel:\n\n");
            for ps in parallel {
                if let Some(ref agent) = ps.agent {
                    let opt = if ps.optional == Some(true) {
                        " (optional)"
                    } else {
                        ""
                    };
                    let owner_info = match ps.owner {
                        Some(ref o) => format!(" [owner: {}]", o),
                        None => String::new(),
                    };
                    out.push_str(&format!(
                        "- **{}**{}{}: {}\n",
                        agent,
                        opt,
                        owner_info,
                        ps.role.as_deref().unwrap_or("Execute your role")
                    ));
                }
            }
            out.push('\n');
        } else if let Some(ref agent) = stage.agent {
            let opt = if stage.optional == Some(true) {
                " (optional)"
            } else {
                ""
            };
            let owner_info = match stage.owner {
                Some(ref o) => format!(" (owner: {})", o),
                None => String::new(),
            };
            out.push_str(&format!(
                "### Stage {}: {}{}{}\n\nUse the Agent tool to invoke the `{}` sub-agent with this role:\n> {}\n\n",
                i + 1,
                agent,
                opt,
                owner_info,
                agent,
                stage.role.as_deref().unwrap_or("Execute your role")
            ));
        }
    }

    Ok(out)
}

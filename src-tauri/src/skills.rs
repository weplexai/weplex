/// Skills discovery and content reading.

#[derive(serde::Serialize)]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
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
pub fn list_skills() -> Vec<SkillInfo> {
    let home = crate::utils::get_home();
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
pub fn read_skill_content(name: String) -> Result<String, String> {
    // Validate name to prevent path traversal
    if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        return Err("Invalid skill name".to_string());
    }

    let home = crate::utils::get_home();

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

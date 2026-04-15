/// Marketplace package saving (agents and skills).

/// Save a marketplace package (agent/skill YAML) to local filesystem.
#[tauri::command]
pub fn save_marketplace_package(dir: String, name: String, content: String) -> Result<(), String> {
    // Whitelist dir to prevent path traversal
    if dir != "agents" && dir != "skills" {
        return Err("Invalid directory: must be 'agents' or 'skills'".to_string());
    }

    let home = crate::utils::get_home();
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

/// Save a marketplace skill to ~/.weplex/skills/<name>/SKILL.md.
#[tauri::command]
pub fn save_marketplace_skill(name: String, content: String) -> Result<(), String> {
    let safe_name: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect();

    let home = crate::utils::get_home();
    let skill_dir = format!("{}/.weplex/skills/{}", home, safe_name);
    std::fs::create_dir_all(&skill_dir)
        .map_err(|e| format!("Failed to create skill directory: {}", e))?;

    let path = format!("{}/SKILL.md", skill_dir);
    std::fs::write(&path, &content)
        .map_err(|e| format!("Failed to write skill: {}", e))?;

    eprintln!("[weplex] marketplace skill saved: {}", path);
    Ok(())
}

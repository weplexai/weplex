/// Profile discovery, directory listing, and resource management commands.

#[derive(serde::Serialize)]
pub struct DiscoveredProfile {
    pub path: String,
    pub name: String,
    pub source: String,
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
pub fn discover_profiles() -> Result<Vec<DiscoveredProfile>, String> {
    let home = crate::utils::get_home();
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
            let resolved = crate::utils::resolve_cwd(&resolved);

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

#[tauri::command]
pub fn list_dirs(partial: String) -> Vec<String> {
    if partial.is_empty() {
        return Vec::new();
    }

    let home = crate::utils::get_home();
    let resolved = crate::utils::resolve_cwd(&partial);

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

// ═══════════════════════════════════════════════════════════════════════
// Resources (agents, rules, skills) — cross-profile management
// ═══════════════════════════════════════════════════════════════════════

/// Validate all config dirs in a list of ProfileInfo.
fn validate_profile_infos(profiles: &[crate::resources::ProfileInfo]) -> Result<(), String> {
    for p in profiles {
        if let Some(ref dir) = p.config_dir {
            if !dir.is_empty() {
                crate::utils::validate_config_dir(dir)?;
            }
        }
    }
    Ok(())
}

/// Validate that a file path is a .md file inside a resource subdirectory (agents/rules/skills).
fn validate_resource_path(file_path: &str) -> Result<String, String> {
    let home = crate::utils::get_home();
    let canonical = std::fs::canonicalize(file_path)
        .map_err(|e| format!("Cannot resolve path: {}", e))?;
    let canonical_str = canonical
        .to_str()
        .ok_or("Path not valid UTF-8")?;

    // Must be under HOME
    if !canonical_str.starts_with(&home) {
        return Err("Path must be under HOME".to_string());
    }

    // Must end with .md
    if !canonical_str.ends_with(".md") {
        return Err("Path must be a .md file".to_string());
    }

    // Must contain a resource subdirectory
    let has_resource_dir = canonical_str.contains("/agents/")
        || canonical_str.contains("/rules/")
        || canonical_str.contains("/skills/");
    if !has_resource_dir {
        return Err("Path must be inside agents/, rules/, or skills/".to_string());
    }

    Ok(canonical_str.to_string())
}

/// Resolve config dir: empty string = default profile ~/.claude/.
fn resolve_config_dir(config_dir: &str) -> Result<String, String> {
    if config_dir.is_empty() {
        let home = crate::utils::get_home();
        Ok(format!("{}/.claude", home))
    } else {
        crate::utils::validate_config_dir(config_dir)
    }
}

#[tauri::command]
pub fn discover_resources(
    profiles: Vec<crate::resources::ProfileInfo>,
) -> Result<Vec<crate::resources::UnifiedResource>, String> {
    validate_profile_infos(&profiles)?;
    crate::resources::discover(&profiles)
}

#[tauri::command]
pub fn count_profile_resources(
    profiles: Vec<crate::resources::ProfileInfo>,
) -> Result<crate::resources::ResourceCounts, String> {
    validate_profile_infos(&profiles)?;
    crate::resources::count_resources(&profiles)
}

#[tauri::command]
pub fn copy_resource_to_profile(
    source_path: String,
    target_config_dir: String,
    resource_type: crate::resources::ResourceType,
    name: String,
    overwrite: bool,
) -> Result<bool, String> {
    let validated_source = validate_resource_path(&source_path)?;
    let validated_target = resolve_config_dir(&target_config_dir)?;
    crate::resources::copy_resource(&validated_source, &validated_target, resource_type, &name, overwrite)
}

#[tauri::command]
pub fn copy_all_resources_to_profile(
    source_profiles: Vec<crate::resources::ProfileInfo>,
    target_config_dir: String,
) -> Result<u32, String> {
    validate_profile_infos(&source_profiles)?;
    let validated_target = resolve_config_dir(&target_config_dir)?;
    crate::resources::copy_all_to_profile(&source_profiles, &validated_target)
}

#[tauri::command]
pub fn create_resource_in_profile(
    config_dir: String,
    resource_type: crate::resources::ResourceType,
    name: String,
    content: String,
) -> Result<String, String> {
    let validated = resolve_config_dir(&config_dir)?;
    crate::resources::create_resource(&validated, resource_type, &name, &content)
}

#[tauri::command]
pub fn delete_resource_file(file_path: String) -> Result<(), String> {
    let validated = validate_resource_path(&file_path)?;
    crate::resources::delete_resource(&validated)
}

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

            // Canonicalize + require result under $HOME. A planted line
            // `export CLAUDE_CONFIG_DIR=/tmp/evil` in ~/.zshrc must NOT make
            // Weplex write `mcpServers.weplex` there — that would let any
            // process touching the rc file gain MCP-loader persistence.
            let resolved = match crate::utils::validate_config_dir(&resolved) {
                Ok(p) => p,
                Err(e) => {
                    log::warn!("Skipping CLAUDE_CONFIG_DIR from {}: {}", filename, e);
                    continue;
                }
            };

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

    // Read body. The legacy resources::copy_resource handles "identical
    // → skip" via FNV-1a hashing and "different → require overwrite";
    // we replicate the contract here while routing through the lockfile.
    let body = std::fs::read_to_string(&validated_source)
        .map_err(|e| format!("Failed to read source: {}", e))?;

    let kind = resource_type_to_kind(resource_type);
    let safe_name = crate::resources::sanitize_name(&name)?;

    // If target exists and !overwrite and content differs, surface the
    // same error message the old API returned. apply_resource_mutation
    // would happily replace it.
    let target_path = format!(
        "{}/{}/{}.md",
        validated_target,
        kind.dir_name(),
        safe_name
    );
    if std::path::Path::new(&target_path).exists() {
        let existing = std::fs::read_to_string(&target_path)
            .map_err(|e| format!("Failed to read target: {}", e))?;
        if existing == body {
            // Identical content → no-op, mirror legacy return.
            return Ok(false);
        }
        if !overwrite {
            return Err("Target exists with different content".to_string());
        }
    }

    // Optional sibling sidecar — copy it through the same mutation.
    let sidecar_src = std::path::Path::new(&validated_source)
        .with_file_name(format!("{}.weplex.yaml", safe_name));
    let sidecar = if sidecar_src.exists() {
        Some(
            std::fs::read_to_string(&sidecar_src)
                .map_err(|e| format!("Failed to read sidecar: {}", e))?,
        )
    } else {
        None
    };

    crate::lockfile::apply_resource_mutation(
        &validated_target,
        kind,
        &safe_name,
        crate::lockfile::ResourceSource::User,
        crate::lockfile::MutationKind::Upsert {
            body,
            sidecar,
            pack: None,
            pack_commit_sha: None,
        },
    )
    .map_err(|e| format!("{}", e))?;
    Ok(true)
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
    let kind = resource_type_to_kind(resource_type);
    let safe_name = crate::resources::sanitize_name(&name)?;

    // Match the legacy "already exists" error.
    let path = format!(
        "{}/{}/{}.md",
        validated,
        kind.dir_name(),
        safe_name
    );
    if std::path::Path::new(&path).exists() {
        return Err(format!("Resource already exists: {}", safe_name));
    }

    crate::lockfile::apply_resource_mutation(
        &validated,
        kind,
        &safe_name,
        crate::lockfile::ResourceSource::User,
        crate::lockfile::MutationKind::Upsert {
            body: content,
            sidecar: None,
            pack: None,
            pack_commit_sha: None,
        },
    )
    .map_err(|e| format!("{}", e))?;
    Ok(path)
}

#[tauri::command]
pub fn delete_resource_file(file_path: String) -> Result<(), String> {
    let validated = validate_resource_path(&file_path)?;

    // Locate the owning profile + resource by walking up the path. The
    // file lives at `<profile>/<kind_dir>/<name>.md` (or
    // `<profile>/skills/<name>/SKILL.md`). When we find a profile that
    // has a lockfile entry for this resource, route the delete through
    // the lockfile so the prior version lands in history. Otherwise
    // fall back to a direct unlink.
    if let Some((profile_dir, kind, name)) = derive_owner(&validated) {
        let lf = crate::lockfile::load_lockfile(&profile_dir);
        let id = format!("{}/{}", kind.dir_name(), name);
        if lf.resources.iter().any(|r| r.id == id) {
            crate::lockfile::apply_resource_mutation(
                &profile_dir,
                kind,
                &name,
                crate::lockfile::ResourceSource::User,
                crate::lockfile::MutationKind::Delete,
            )
            .map_err(|e| format!("{}", e))?;
            return Ok(());
        }
    }

    crate::resources::delete_resource(&validated)
}

/// Map a `ResourceType` (legacy) to the cross-agent `ResourceKind`.
fn resource_type_to_kind(rt: crate::resources::ResourceType) -> crate::manifest::ResourceKind {
    match rt {
        crate::resources::ResourceType::Agent => crate::manifest::ResourceKind::Agent,
        crate::resources::ResourceType::Rule => crate::manifest::ResourceKind::Rule,
        crate::resources::ResourceType::Skill => crate::manifest::ResourceKind::Skill,
    }
}

/// Best-effort owner derivation from an absolute resource file path.
/// Returns `(profile_config_dir, kind, name)` when the path matches the
/// expected `<profile>/<kind_dir>/<name>.md` (or skills) layout.
fn derive_owner(
    file_path: &str,
) -> Option<(String, crate::manifest::ResourceKind, String)> {
    let p = std::path::Path::new(file_path);
    let file_stem = p.file_stem().and_then(|s| s.to_str())?;
    let parent = p.parent()?;
    let parent_name = parent.file_name().and_then(|n| n.to_str())?;

    // Skill: <profile>/skills/<name>/SKILL.md
    if parent_name != "agents" && parent_name != "rules" && parent_name != "commands" {
        // This may be the skill's leaf dir.
        if file_stem == "SKILL" {
            let name = parent_name.to_string();
            let grand = parent.parent()?;
            let grand_name = grand.file_name().and_then(|n| n.to_str())?;
            if grand_name == "skills" {
                let profile = grand.parent()?;
                return Some((
                    profile.to_string_lossy().into_owned(),
                    crate::manifest::ResourceKind::Skill,
                    name,
                ));
            }
        }
        return None;
    }

    let kind = match parent_name {
        "agents" => crate::manifest::ResourceKind::Agent,
        "rules" => crate::manifest::ResourceKind::Rule,
        "commands" => crate::manifest::ResourceKind::Command,
        _ => return None,
    };
    let profile = parent.parent()?;
    Some((
        profile.to_string_lossy().into_owned(),
        kind,
        file_stem.to_string(),
    ))
}

/// Shared utilities: path resolution, validation, sanitization.
/// Single source of truth for HOME directory access.

/// Get the user's HOME directory. Falls back to "/" if not set.
pub fn get_home() -> String {
    std::env::var("HOME").unwrap_or_else(|_| "/".to_string())
}

/// Resolve a cwd with tilde to an absolute path.
pub fn resolve_cwd(cwd: &str) -> String {
    let home = get_home();
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

/// Sanitize a name for use as filename: replace invalid chars.
/// Returns sanitized name or error.
pub fn sanitize_name(name: &str) -> Result<String, String> {
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

/// Verify that a file path is within the expected directory (prevents path traversal).
pub fn validate_path_within(file_path: &str, expected_dir: &str) -> Result<(), String> {
    let canonical = std::fs::canonicalize(file_path)
        .map_err(|_| format!("Path does not exist: {}", file_path))?;
    let canonical_dir = std::fs::canonicalize(expected_dir)
        .map_err(|_| format!("Directory does not exist: {}", expected_dir))?;
    if !canonical.starts_with(&canonical_dir) {
        return Err("Path traversal detected".to_string());
    }
    Ok(())
}

/// Validate that config_dir is an absolute path under $HOME.
/// Resolves symlinks to prevent symlink attacks.
pub fn validate_config_dir(config_dir: &str) -> Result<String, String> {
    let home = get_home();

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

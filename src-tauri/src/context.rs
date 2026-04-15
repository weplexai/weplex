/// CLAUDE.local.md context injection for workspace awareness.

/// Inject Weplex workspace context into the project's CLAUDE.local.md.
/// This file is gitignored by design (Claude Code convention), so it won't
/// pollute the shared repo. Writes only when content has changed.
#[tauri::command]
pub fn inject_context_block(cwd: String, context_block: String) -> Result<String, String> {
    let resolved = crate::utils::resolve_cwd(&cwd);

    // Path validation: must be an existing directory, no traversal
    let canonical = std::fs::canonicalize(&resolved)
        .map_err(|_| format!("Invalid project directory: {}", resolved))?;
    if !canonical.is_dir() {
        return Err(format!("Not a directory: {}", resolved));
    }
    let resolved = canonical.to_string_lossy().to_string();

    let config_path = format!("{}/CLAUDE.local.md", resolved);

    // Ensure CLAUDE.local.md is in .gitignore
    let gitignore_path = format!("{}/.gitignore", resolved);
    if std::path::Path::new(&gitignore_path).exists() {
        let gitignore = std::fs::read_to_string(&gitignore_path).unwrap_or_default();
        if !gitignore.lines().any(|l| l.trim() == "CLAUDE.local.md") {
            let separator = if gitignore.ends_with('\n') || gitignore.is_empty() { "" } else { "\n" };
            std::fs::write(&gitignore_path, format!("{}{}{}\n", gitignore, separator, "CLAUDE.local.md"))
                .map_err(|e| format!("Failed to update .gitignore: {}", e))?;
            eprintln!("[weplex] added CLAUDE.local.md to .gitignore");
        }
    }

    // Read existing content
    let existing = std::fs::read_to_string(&config_path).unwrap_or_default();

    let new_content = if let Some((before, after)) = strip_weplex_block(&existing) {
        // Block exists — replace in place, preserve surrounding content
        match (before.is_empty(), after.is_empty()) {
            (true, true) => context_block.clone(),
            (true, false) => format!("{}\n\n{}", context_block, after),
            (false, true) => format!("{}\n\n{}", before, context_block),
            (false, false) => format!("{}\n\n{}\n\n{}", before, context_block, after),
        }
    } else if existing.trim().is_empty() {
        // New file — just the block
        context_block.clone()
    } else {
        // File exists but no block — prepend
        format!("{}\n\n{}", context_block, existing)
    };

    // Skip write if content unchanged
    if existing == new_content {
        return Ok(config_path);
    }

    std::fs::write(&config_path, &new_content)
        .map_err(|e| format!("Failed to write CLAUDE.local.md: {}", e))?;

    eprintln!("[weplex] injected context into {}", config_path);
    Ok(config_path)
}

/// Find and strip the Weplex context block delimited by HTML comments.
/// Returns Some((before, after)) with trimmed surrounding content, or None if no block found.
fn strip_weplex_block(content: &str) -> Option<(String, String)> {
    let start_marker = "<!-- wplx-ctx";
    let end_marker = "<!-- /wplx-ctx -->";

    let start = content.find(start_marker)?;

    let after_start = &content[start..];
    let end = if let Some(rel_end) = after_start.find(end_marker) {
        start + rel_end + end_marker.len()
    } else {
        // Missing end marker — strip from start to end of file
        content.len()
    };

    let before = content[..start].trim_end().to_string();
    let after = content[end..].trim_start().to_string();
    Some((before, after))
}

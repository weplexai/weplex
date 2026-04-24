/// Git integration: branch detection and file status.

#[derive(serde::Serialize)]
pub struct GitFileChange {
    pub path: String,
    pub status: String, // "M", "A", "D", "R", "?"
}

/// Get the current git branch for a directory.
#[tauri::command]
pub fn get_git_branch(cwd: String) -> Result<Option<String>, String> {
    let resolved = crate::utils::resolve_cwd(&cwd);
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(&resolved)
        .output()
        .map_err(|e| format!("Failed to run git: {}", e))?;

    if !output.status.success() {
        // Not a git repo or git not available
        return Ok(None);
    }

    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if branch.is_empty() {
        return Ok(None);
    }

    // Detached HEAD — show short commit hash instead of literal "HEAD"
    if branch == "HEAD" {
        let hash = std::process::Command::new("git")
            .args(["rev-parse", "--short", "HEAD"])
            .current_dir(&resolved)
            .output()
            .ok()
            .and_then(|h| {
                let s = String::from_utf8_lossy(&h.stdout).trim().to_string();
                if s.is_empty() { None } else { Some(format!("detached@{}", s)) }
            });
        return Ok(hash);
    }

    Ok(Some(branch))
}

/// Get git status (modified/added/deleted files) for a directory.
#[tauri::command]
pub fn get_git_status(cwd: String) -> Result<Vec<GitFileChange>, String> {
    let resolved = crate::utils::resolve_cwd(&cwd);
    let output = std::process::Command::new("git")
        .args(["status", "--porcelain", "-unormal"])
        .current_dir(&resolved)
        .output()
        .map_err(|e| format!("Failed to run git: {}", e))?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut files = Vec::new();

    for line in stdout.lines() {
        if line.len() < 4 {
            continue;
        }
        // git status --porcelain format: "XY path" where X=index, Y=worktree
        let status_char = line.chars().nth(0).unwrap_or(' ');
        let worktree_char = line.chars().nth(1).unwrap_or(' ');

        // Prefer worktree status, fallback to index status
        let status = match worktree_char {
            'M' => "M",
            'D' => "D",
            'A' => "A",
            '?' => "?",
            _ => match status_char {
                'M' => "M",
                'A' => "A",
                'D' => "D",
                'R' => "R",
                '?' => "?",
                _ => continue,
            },
        };

        let raw_path = &line[3..];
        // Renamed files: "R  old -> new" — use the new path
        let path = if status == "R" {
            raw_path.split(" -> ").last().unwrap_or(raw_path).to_string()
        } else {
            raw_path.to_string()
        };
        files.push(GitFileChange {
            path,
            status: status.to_string(),
        });
    }

    // Cap at 200 files to avoid huge payloads
    files.truncate(200);
    Ok(files)
}

#[derive(serde::Serialize)]
pub struct ProjectConfig {
    pub exists: bool,
    pub content: String,
    pub cwd: String,
    pub config_path: String,
}

/// Read a project's CLAUDE.md (checks .claude/CLAUDE.md and root CLAUDE.md).
#[tauri::command]
pub fn get_project_config(cwd: String) -> Result<ProjectConfig, String> {
    let resolved = crate::utils::resolve_cwd(&cwd);

    // Check .claude/CLAUDE.md first, then root CLAUDE.md
    for path_suffix in &[".claude/CLAUDE.md", "CLAUDE.md"] {
        let config_path = format!("{}/{}", resolved, path_suffix);
        if let Ok(content) = std::fs::read_to_string(&config_path) {
            return Ok(ProjectConfig {
                exists: true,
                content,
                cwd: resolved,
                config_path,
            });
        }
    }

    let config_path = format!("{}/.claude/CLAUDE.md", resolved);
    Ok(ProjectConfig {
        exists: false,
        content: String::new(),
        cwd: resolved,
        config_path,
    })
}

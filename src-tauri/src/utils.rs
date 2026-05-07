/// Shared utilities: path resolution, validation, sanitization.
/// Single source of truth for HOME directory access.

/// Get the user's HOME directory. Falls back to "/" if not set.
pub fn get_home() -> String {
    std::env::var("HOME").unwrap_or_else(|_| "/".to_string())
}

/// Atomically write `contents` to `path` with mode 0600 (data file).
pub fn atomic_write_owner_only(path: &str, contents: &str) -> Result<(), String> {
    atomic_write_with_mode(path, contents, 0o600)
}

/// Atomically write `contents` to `path` with mode 0700 (owner-exec file).
/// Use for shell scripts that must carry the execute bit.
pub fn atomic_write_exec_owner_only(path: &str, contents: &str) -> Result<(), String> {
    atomic_write_with_mode(path, contents, 0o700)
}

/// Atomically write `contents` to `path` with mode 0644 (user-readable).
/// Use for files the user reads in their own IDE or editor:
/// `AGENTS.md`, `.cursorrules`, `.mdc`, OpenCode skill files, etc.
///
/// These are config files for *external* tools (Codex, Cursor, OpenCode),
/// not Weplex secrets — they intentionally live in well-known locations
/// like `~/.codex/AGENTS.md` where the user (and the target tool) read
/// them. 0644 mirrors what those tools would create themselves.
pub fn atomic_write_user_readable(path: &str, contents: &str) -> Result<(), String> {
    atomic_write_with_mode(path, contents, 0o644)
}

/// Internal: atomic tmp+rename with explicit unix mode.
///
/// - Creates a sibling `<path>.<pid>.tmp`. The PID suffix makes concurrent
///   writers (different Weplex instances) use different tmp names,
///   eliminating a symlink-race window between the stale-tmp cleanup and
///   the open.
/// - Uses `create_new(true)` to refuse to open an existing file — if a
///   stale file with our exact name somehow exists, we error out rather
///   than following it (defence against symlink attacks).
/// - Renames into place; `rename(2)` is atomic on the same filesystem.
/// - On any failure, removes the stale tmp so we never leave a partial
///   copy of the contents on disk.
///
/// Callers must own the containing directory (we don't chmod it here).
/// The `_mode` argument is only honored on Unix; Windows ignores it and
/// uses default permissions.
fn atomic_write_with_mode(path: &str, contents: &str, _mode: u32) -> Result<(), String> {
    let tmp_path = format!("{}.{}.tmp", path, std::process::id());
    // Pre-clean: a prior crash by the same PID may have left a stale tmp.
    // Using a PID-suffixed name means this only removes our own past leftovers.
    let _ = std::fs::remove_file(&tmp_path);

    #[cfg(unix)]
    let write_result = {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(_mode)
            .open(&tmp_path)
            .and_then(|mut f| f.write_all(contents.as_bytes()).and_then(|_| f.sync_all()))
    };
    #[cfg(not(unix))]
    let write_result = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp_path)
        .and_then(|mut f| std::io::Write::write_all(&mut f, contents.as_bytes()));

    if let Err(e) = write_result {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(format!("atomic_write: failed to write tmp ({}): {}", tmp_path, e));
    }

    if let Err(e) = std::fs::rename(&tmp_path, path) {
        // Leave nothing behind — the tmp still contains the full payload.
        let _ = std::fs::remove_file(&tmp_path);
        return Err(format!("atomic_write: failed to rename into place ({}): {}", path, e));
    }

    Ok(())
}

/// Process-level lock for tests that mutate `$HOME`. Tests in any module
/// that override HOME via `set_var` MUST take this lock first; otherwise
/// parallel `cargo test` runs will race and produce flaky failures.
#[cfg(test)]
pub mod test_support {
    use std::sync::Mutex;
    pub static HOME_ENV_LOCK: Mutex<()> = Mutex::new(());
}

#[cfg(test)]
mod atomic_write_tests {
    use super::*;

    fn tmpdir() -> std::path::PathBuf {
        let base = std::env::temp_dir().join(format!(
            "weplex-atomic-write-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&base).unwrap();
        base
    }

    #[test]
    fn writes_content_and_removes_tmp() {
        let dir = tmpdir();
        let target = dir.join("config.json");
        atomic_write_owner_only(target.to_str().unwrap(), r#"{"ok":true}"#).unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), r#"{"ok":true}"#);
        // No tmp leftover after success.
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            assert!(
                !name.contains(".tmp"),
                "tmp file leaked: {}",
                name
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn created_file_has_owner_only_mode() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tmpdir();
        let target = dir.join("secret.json");
        atomic_write_owner_only(target.to_str().unwrap(), "x").unwrap();
        let mode = std::fs::metadata(&target).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "expected 0600, got {:o}", mode);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn exec_variant_sets_0700_mode() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tmpdir();
        let target = dir.join("hook.sh");
        atomic_write_exec_owner_only(target.to_str().unwrap(), "#!/bin/bash\nexit 0\n").unwrap();
        let mode = std::fs::metadata(&target).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "expected 0700, got {:o}", mode);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn user_readable_variant_sets_0644_mode() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tmpdir();
        let target = dir.join("AGENTS.md");
        atomic_write_user_readable(target.to_str().unwrap(), "# heading\n").unwrap();
        let mode = std::fs::metadata(&target).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o644, "expected 0644, got {:o}", mode);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn overwrites_existing_file_atomically() {
        let dir = tmpdir();
        let target = dir.join("config.json");
        std::fs::write(&target, "old").unwrap();
        atomic_write_owner_only(target.to_str().unwrap(), "new").unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fails_cleanly_when_parent_dir_missing() {
        // No cleanup needed — parent doesn't exist so tmp can't land anywhere.
        let nonexistent = "/tmp/weplex-atomic-write-no-such-dir-xyz/target.json";
        let err = atomic_write_owner_only(nonexistent, "x").unwrap_err();
        assert!(err.contains("failed to write tmp"), "unexpected: {}", err);
    }
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

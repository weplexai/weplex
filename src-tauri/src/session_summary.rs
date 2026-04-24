use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NoteEntry {
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files_changed: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decisions: Vec<String>,
    pub at: u64,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    /// Legacy field — kept for backward compat reading
    #[serde(default)]
    pub summary: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files_changed: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decisions: Vec<String>,
    pub updated_at: u64,
    /// Chronological list of activity notes appended by the agent
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<NoteEntry>,
}

/// Return the path to ~/.weplex/activity/
pub fn activity_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home).join(".weplex/activity")
}

/// Legacy pre-pivot location. Kept only so the one-shot migration on
/// startup can find old data. All new reads/writes use `activity_dir()`.
fn legacy_summaries_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home).join(".weplex/summaries")
}

/// Read a session's activity log from disk.
pub fn read_summary(session_id: &str) -> Option<SessionSummary> {
    let path = activity_dir().join(format!("{}.json", session_id));
    let content = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Remove activity files older than 7 days.
pub fn cleanup_old_activity() {
    let dir = activity_dir();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        let cutoff = std::time::SystemTime::now()
            .checked_sub(std::time::Duration::from_secs(7 * 24 * 3600));
        let Some(cutoff) = cutoff else { return };

        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if let Ok(modified) = meta.modified() {
                    if modified < cutoff {
                        let _ = std::fs::remove_file(entry.path());
                    }
                }
            }
        }
    }
}

/// Ensure the activity directory exists and is owner-only (0700).
///
/// Files inside are already 0600, but tightening the parent dir is
/// defence-in-depth: other local users can't even list the set of Weplex
/// session IDs.
pub fn ensure_activity_dir() {
    let dir = activity_dir();
    let _ = std::fs::create_dir_all(&dir);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    }
}

/// One-shot migration from the pre-pivot `~/.weplex/summaries/` layout to
/// `~/.weplex/activity/`. Idempotent and re-runs safely on every startup.
///
/// Rules:
/// - If the new `activity/` dir already exists, move any leftover files
///   from `summaries/` into it (handles mid-migration crashes) and then
///   try to remove the now-empty legacy dir.
/// - If only `summaries/` exists, attempt a single atomic `fs::rename`.
/// - If neither exists, or if only the new dir exists and legacy is gone,
///   this is a no-op.
///
/// Best-effort: any IO error is logged and the app continues. Partial state
/// is recoverable on the next startup — activity_dir() is always the source
/// of truth post-migration.
pub fn migrate_summaries_to_activity() {
    migrate_dirs(&legacy_summaries_dir(), &activity_dir());
}

/// Pure implementation of the activity migration, parameterized by paths so
/// tests can exercise it against tempdirs without touching `$HOME`.
fn migrate_dirs(old: &std::path::Path, new: &std::path::Path) {
    // Defence-in-depth: refuse to migrate if `old` is a symlink. Without this
    // check, a link pointing outside `~/.weplex/` would cause read_dir/rename
    // to operate on attacker-chosen files. The realistic threat requires an
    // attacker who already has write access to `$HOME/.weplex/` (same trust
    // boundary as the app itself), but refusing is cheaper than auditing it.
    match std::fs::symlink_metadata(old) {
        Ok(meta) if meta.file_type().is_symlink() => {
            log::warn!(
                "activity: legacy path {} is a symlink, refusing to migrate",
                old.display()
            );
            return;
        }
        _ => {}
    }

    let old_exists = old.exists();
    let new_exists = new.exists();

    if !old_exists {
        return; // Already migrated or never used.
    }

    if !new_exists {
        // Happy path: single atomic rename.
        match std::fs::rename(old, new) {
            Ok(_) => {
                log::info!("activity: migrated {} → {}", old.display(), new.display());
                return;
            }
            Err(e) => {
                // Cross-device rename is the only expected failure (EXDEV);
                // fall through to per-file copy below.
                log::warn!(
                    "activity: atomic rename failed ({}), falling back to per-file copy",
                    e
                );
            }
        }
    }

    // Either new dir already existed, or rename failed — move files one at a time.
    let _ = std::fs::create_dir_all(new);
    let entries = match std::fs::read_dir(old) {
        Ok(e) => e,
        Err(_) => return,
    };
    let mut moved = 0u32;
    let mut failed = 0u32;
    for entry in entries.flatten() {
        let src = entry.path();
        let Some(name) = src.file_name() else { continue };
        let dst = new.join(name);
        if dst.exists() {
            // Don't clobber newer content in the destination.
            continue;
        }
        match std::fs::rename(&src, &dst) {
            Ok(_) => moved += 1,
            Err(_) => match std::fs::copy(&src, &dst) {
                Ok(_) => {
                    let _ = std::fs::remove_file(&src);
                    moved += 1;
                }
                Err(e) => {
                    log::warn!("activity: failed to migrate {:?}: {}", src, e);
                    failed += 1;
                }
            },
        }
    }

    // Best-effort cleanup of now-empty legacy dir.
    let _ = std::fs::remove_dir(old);

    if moved > 0 || failed > 0 {
        log::info!(
            "activity: per-file migration moved={} failed={}",
            moved, failed
        );
    }
}

#[cfg(test)]
mod migration_tests {
    use super::*;

    fn tmpdir() -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "weplex-migration-test-{}-{}",
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
    fn happy_path_renames_legacy_dir() {
        let root = tmpdir();
        let old = root.join("summaries");
        let new = root.join("activity");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("42.json"), r#"{"updatedAt":1}"#).unwrap();

        migrate_dirs(&old, &new);

        assert!(!old.exists(), "old dir should be moved");
        assert!(new.exists(), "new dir should exist");
        assert_eq!(
            std::fs::read_to_string(new.join("42.json")).unwrap(),
            r#"{"updatedAt":1}"#
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn no_op_when_legacy_absent() {
        let root = tmpdir();
        let old = root.join("summaries");
        let new = root.join("activity");
        std::fs::create_dir_all(&new).unwrap();

        migrate_dirs(&old, &new);

        assert!(!old.exists());
        assert!(new.exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn no_op_when_neither_exists() {
        let root = tmpdir();
        let old = root.join("summaries");
        let new = root.join("activity");

        migrate_dirs(&old, &new);

        assert!(!old.exists());
        assert!(!new.exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn per_file_merge_when_both_dirs_exist() {
        let root = tmpdir();
        let old = root.join("summaries");
        let new = root.join("activity");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::create_dir_all(&new).unwrap();
        // Legacy leftover
        std::fs::write(old.join("1.json"), "old1").unwrap();
        std::fs::write(old.join("2.json"), "old2").unwrap();
        // New dir already has a file for session 2 — must NOT be clobbered.
        std::fs::write(new.join("2.json"), "new2").unwrap();

        migrate_dirs(&old, &new);

        // 1.json migrated over.
        assert_eq!(std::fs::read_to_string(new.join("1.json")).unwrap(), "old1");
        // 2.json kept the new version.
        assert_eq!(std::fs::read_to_string(new.join("2.json")).unwrap(), "new2");
        // Legacy dir cleaned up (or at least the moved file removed).
        assert!(!old.join("1.json").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn refuses_to_migrate_symlinked_legacy() {
        use std::os::unix::fs::symlink;
        let root = tmpdir();
        let real = root.join("elsewhere");
        let old = root.join("summaries"); // will be a symlink
        let new = root.join("activity");
        std::fs::create_dir_all(&real).unwrap();
        std::fs::write(real.join("sensitive.json"), "not-ours").unwrap();
        symlink(&real, &old).unwrap();

        migrate_dirs(&old, &new);

        // Nothing moved; the attacker-controlled file is still at its origin.
        assert!(!new.exists(), "new dir must not be created from a symlinked legacy");
        assert!(real.join("sensitive.json").exists(), "target must be untouched");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn migration_is_idempotent() {
        let root = tmpdir();
        let old = root.join("summaries");
        let new = root.join("activity");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("1.json"), "x").unwrap();

        migrate_dirs(&old, &new);
        // Second call must be a no-op (old is gone).
        migrate_dirs(&old, &new);

        assert_eq!(std::fs::read_to_string(new.join("1.json")).unwrap(), "x");
        let _ = std::fs::remove_dir_all(&root);
    }
}

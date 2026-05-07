use crate::notes_crypto::{self, EncryptedFile, EncryptedPayload, FORMAT_VERSION};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

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
    /// Set when the file exists on disk but cannot be decrypted (Keychain
    /// locked, key rotated, file tampered). UI uses this to render a 🔒 state
    /// instead of confusing the user with a sentinel string.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unreadable: bool,
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

/// Inner JSON layout that lives inside `EncryptedPayload.ct`. Only the
/// "secret" fields are inside the ciphertext. `updated_at` stays in the
/// envelope so the stop-hook freshness check doesn't need the Keychain.
#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct EncryptedInner {
    #[serde(default)]
    summary: String,
    #[serde(default)]
    files_changed: Vec<String>,
    #[serde(default)]
    decisions: Vec<String>,
    #[serde(default)]
    notes: Vec<NoteEntry>,
}

/// Marker used by the stop hook to recognize the encrypted envelope shape
/// even without the key. Present in every file written by this module.
const ENVELOPE_MARKER_FIELD: &str = "encrypted";

/// Result of `read_summary` returned for a file that exists but cannot be
/// decrypted right now. UI keys off `unreadable: true` rather than any
/// human-readable string.
fn unreadable_summary(updated_at: u64) -> SessionSummary {
    SessionSummary {
        summary: String::new(),
        files_changed: Vec::new(),
        decisions: Vec::new(),
        updated_at,
        notes: Vec::new(),
        unreadable: true,
    }
}

/// Same shape but with `unreadable: false` for the success path.
fn readable_summary(updated_at: u64, inner: EncryptedInner) -> SessionSummary {
    SessionSummary {
        summary: inner.summary,
        files_changed: inner.files_changed,
        decisions: inner.decisions,
        updated_at,
        notes: inner.notes,
        unreadable: false,
    }
}

/// Validate session_id against the same allowlist the IPC handler uses.
/// Defence-in-depth: the module is `pub`, and a future caller that forgets
/// to validate would otherwise enable path traversal via `../../etc/foo`.
fn validate_session_id(session_id: &str) -> Result<(), String> {
    if session_id.is_empty()
        || !session_id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(format!("Invalid session_id format"));
    }
    Ok(())
}

/// Tiny redaction so logs don't leak the user's home path or profile name —
/// returns a fixed-length fingerprint that's still useful to grep across logs.
fn redact_profile(profile_id: &str) -> String {
    let h = ring::digest::digest(&ring::digest::SHA256, profile_id.as_bytes());
    let b = h.as_ref();
    format!("profile#{:02x}{:02x}{:02x}{:02x}", b[0], b[1], b[2], b[3])
}

/// Try to parse `content` as the encrypted envelope format.
/// Returns None if the marker field isn't present.
fn parse_envelope(content: &str) -> Option<EncryptedFile> {
    let v: serde_json::Value = serde_json::from_str(content).ok()?;
    if v.get(ENVELOPE_MARKER_FIELD).is_none() {
        return None;
    }
    serde_json::from_value(v).ok()
}

/// Read a session's activity log. Encrypted envelopes are required — legacy
/// plaintext files yield `None` (caller treats that as "no notes yet"). The
/// plaintext fallback was removed because it was a prompt-injection vector:
/// any process running as the user could plant a forged `summary` and have
/// the UI render it as the user's own note, bypassing all of GCM+AAD.
///
/// Decryption failures yield `Some(unreadable=true)` so the UI can render a
/// 🔒 state instead of "no notes" — distinguishing locked from missing.
pub fn read_summary(session_id: &str, profile_id: &str) -> Option<SessionSummary> {
    if validate_session_id(session_id).is_err() {
        return None;
    }
    let path = activity_dir().join(format!("{}.json", session_id));
    let content = std::fs::read_to_string(&path).ok()?;

    let env = parse_envelope(&content)?; // legacy plaintext intentionally None

    // Read path uses get_key, NOT get_or_create_key — generating a fresh key
    // here would orphan all previously-encrypted notes for this profile.
    let key = match notes_crypto::get_key(profile_id) {
        Ok(Some(k)) => k,
        Ok(None) => {
            log::warn!(
                "notes: no Keychain key yet for {}",
                redact_profile(profile_id)
            );
            return Some(unreadable_summary(env.updated_at));
        }
        Err(e) => {
            log::warn!(
                "notes: keychain unavailable for {}: {}",
                redact_profile(profile_id),
                e
            );
            return Some(unreadable_summary(env.updated_at));
        }
    };
    let plaintext = match notes_crypto::decrypt(&env.encrypted, &key, session_id, env.updated_at)
    {
        Ok(p) => p,
        Err(e) => {
            log::warn!("notes: decrypt failed for session {}: {}", session_id, e);
            return Some(unreadable_summary(env.updated_at));
        }
    };
    let inner: EncryptedInner = match serde_json::from_slice(&plaintext) {
        Ok(i) => i,
        Err(e) => {
            log::warn!("notes: inner JSON malformed for {}: {}", session_id, e);
            return Some(unreadable_summary(env.updated_at));
        }
    };
    Some(readable_summary(env.updated_at, inner))
}

/// Lightweight read of just the envelope's `updated_at` field. Used by the
/// stop-hook freshness check, which must work without unlocking the Keychain.
///
/// Requires the envelope marker — refuses bare `{"updatedAt": <future>}`
/// plaintext files. Without that check, any local process could write such a
/// file and silence the Stop hook forever for that session id.
pub fn read_updated_at_only(session_id: &str) -> Option<u64> {
    if validate_session_id(session_id).is_err() {
        return None;
    }
    let path = activity_dir().join(format!("{}.json", session_id));
    let content = std::fs::read_to_string(&path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&content).ok()?;
    if v.get(ENVELOPE_MARKER_FIELD).is_none() {
        return None; // envelope shape required
    }
    v.get("updated_at").and_then(|x| x.as_u64())
}

/// Append a note to the session's activity log. Reads the existing file
/// (decrypts it), pushes the new entry, re-encrypts, and atomically writes.
/// Caps the notes array at 200 entries (drops oldest).
///
/// On a legacy plaintext file the read attempts a strict parse — if it
/// doesn't match `EncryptedInner`'s shape we error out rather than silently
/// overwriting unfamiliar data with `EncryptedInner::default()`.
pub fn append_note(
    session_id: &str,
    profile_id: &str,
    note: NoteEntry,
) -> Result<(), String> {
    validate_session_id(session_id)?;
    let key = notes_crypto::get_or_create_key(profile_id)?;
    let path = activity_dir().join(format!("{}.json", session_id));

    let mut inner: EncryptedInner = match std::fs::read_to_string(&path).ok().as_deref() {
        Some(content) => {
            if let Some(env) = parse_envelope(content) {
                match notes_crypto::decrypt(&env.encrypted, &key, session_id, env.updated_at) {
                    Ok(pt) => serde_json::from_slice(&pt)
                        .map_err(|e| format!("Decrypted notes JSON malformed: {}", e))?,
                    Err(e) => {
                        return Err(format!("Cannot extend unreadable notes file: {}", e));
                    }
                }
            } else {
                // Legacy plaintext upgrade — strict parse, no silent default.
                // Backs the original file up before overwrite so unexpected
                // schema doesn't get destroyed by the new encrypted write.
                let bak = path.with_extension("json.bak");
                if let Err(e) = std::fs::copy(&path, &bak) {
                    log::warn!("notes: legacy backup failed for {}: {}", session_id, e);
                }
                match serde_json::from_str::<EncryptedInner>(content) {
                    Ok(parsed) => parsed,
                    Err(e) => {
                        return Err(format!(
                            "Legacy notes file has unexpected schema (backup at {}.bak): {}",
                            path.display(),
                            e
                        ));
                    }
                }
            }
        }
        None => EncryptedInner::default(),
    };

    if inner.notes.len() >= 200 {
        inner.notes.remove(0);
    }
    inner.summary = note.text.clone();
    inner.files_changed = note.files_changed.clone();
    inner.decisions = note.decisions.clone();
    inner.notes.push(note);

    let plaintext =
        serde_json::to_vec(&inner).map_err(|e| format!("Serialize inner: {}", e))?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // updated_at must be passed to encrypt() because it's bound into AAD.
    let payload = notes_crypto::encrypt(&plaintext, &key, session_id, now)?;

    let envelope = EncryptedFile {
        v: FORMAT_VERSION,
        updated_at: now,
        encrypted: payload,
    };
    let json = serde_json::to_string_pretty(&envelope)
        .map_err(|e| format!("Serialize envelope: {}", e))?;

    crate::utils::atomic_write_owner_only(
        path.to_str().ok_or("Activity path not UTF-8")?,
        &json,
    )?;
    Ok(())
}

#[allow(dead_code)] // referenced by EncryptedPayload import — silences unused warn
const _: Option<EncryptedPayload> = None;

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

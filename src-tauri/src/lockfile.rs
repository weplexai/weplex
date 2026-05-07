//! Per-profile lockfile: resource provenance + history + cache.
//!
//! Every resource Weplex writes into a profile (agents/rules/skills/commands)
//! is tracked in `.weplex.lock.yaml` next to the profile root. The lockfile
//! holds:
//!
//! - `resources`: one entry per currently-installed resource id, with body
//!   sha256, optional sidecar sha256, source provenance (Builtin / User /
//!   Marketplace / Imported), and the list of files it owns.
//! - `history`: per-resource ring buffer of previous versions, keyed by
//!   id. Each entry remembers the cache paths under `.weplex/cache/<sha16>`
//!   so we can roll back without going off disk for the original body.
//!
//! Mutations route through ONE entry point — `apply_resource_mutation`. It
//! takes an exclusive flock on `.weplex/lockfile.lock`, snapshots the
//! current bytes into the content-addressed cache, writes the new body
//! atomically, runs cache GC, then atomic-writes the lockfile YAML.
//!
//! Phase 3, no caller in the production tree mutates profile resources
//! except through this module. Direct file writes are an architecture bug.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::manifest::ResourceKind;
use crate::utils::{
    atomic_write_user_readable, get_home, sanitize_name, sha256_file, sha256_hex,
    validate_config_dir,
};

// ─── Constants ──────────────────────────────────────────────────────────

pub const LOCKFILE_VERSION: u32 = 1;
pub const LOCKFILE_NAME: &str = ".weplex.lock.yaml";
pub const CACHE_DIR: &str = ".weplex/cache";
pub const LOCK_FILE: &str = ".weplex/lockfile.lock";
// Used in commits D/H. Public for future use.
#[allow(dead_code)]
pub const LEGACY_FLAG: &str = ".weplex/legacy-weplex-migrated.flag";

pub const MAX_HISTORY_PER_RESOURCE: usize = 10;
pub const MAX_HISTORY_AGE_DAYS: i64 = 30;
#[allow(dead_code)]
pub const MAX_ARCHIVE_SIZE_BYTES: u64 = 50 * 1024 * 1024;
#[allow(dead_code)]
pub const MAX_ARCHIVE_ENTRY_BYTES: u64 = 10 * 1024 * 1024;
#[allow(dead_code)]
pub const MAX_ARCHIVE_TOTAL_UNCOMPRESSED: u64 = 200 * 1024 * 1024;

// ─── Errors ─────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum LockfileError {
    Io(String),
    Parse(String),
    InvalidArchive(String),
    Sha256Mismatch { expected: String, got: String },
    NotFound(String),
}

impl std::fmt::Display for LockfileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LockfileError::Io(m) => write!(f, "lockfile io error: {}", m),
            LockfileError::Parse(m) => write!(f, "lockfile parse error: {}", m),
            LockfileError::InvalidArchive(m) => write!(f, "invalid archive: {}", m),
            LockfileError::Sha256Mismatch { expected, got } => {
                write!(f, "sha256 mismatch: expected {}, got {}", expected, got)
            }
            LockfileError::NotFound(m) => write!(f, "not found: {}", m),
        }
    }
}

impl std::error::Error for LockfileError {}

// ─── Schema ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResourceSource {
    Builtin,
    User,
    Marketplace,
    Imported,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LockfileEntry {
    /// Stable id: `<kind_dir>/<name>`, e.g. `agents/architect`.
    pub id: String,
    pub kind: ResourceKind,
    pub source: ResourceSource,
    #[serde(default)]
    pub version: Option<String>,
    pub sha256: String,
    #[serde(default)]
    pub sidecar_sha256: Option<String>,
    /// Files owned by this entry, relative to `profile_config_dir`.
    pub files: Vec<String>,
    pub installed_at: DateTime<Utc>,
    pub installed_by: String,
    /// Set by `reconcile_on_load` when on-disk bytes diverge from the
    /// recorded sha. Never persisted.
    #[serde(skip)]
    pub drifted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LockfileHistoryEntry {
    #[serde(default)]
    pub version: Option<String>,
    pub sha256: String,
    #[serde(default)]
    pub sidecar_sha256: Option<String>,
    pub source: ResourceSource,
    pub installed_at: DateTime<Utc>,
    /// Cache paths relative to `profile_config_dir`, e.g.
    /// `.weplex/cache/abcd0123abcd0123/agents/architect.md`.
    pub cache_paths: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Lockfile {
    pub version: u32,
    #[serde(default = "default_generated_by")]
    pub generated_by: String,
    #[serde(default)]
    pub resources: Vec<LockfileEntry>,
    #[serde(default)]
    pub history: BTreeMap<String, Vec<LockfileHistoryEntry>>,
}

fn default_generated_by() -> String {
    "weplex".to_string()
}

// ─── Mutation API ───────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum MutationKind {
    Upsert {
        body: String,
        sidecar: Option<String>,
    },
    /// Delete is wired up by callers in commits E/F/G.
    #[allow(dead_code)]
    Delete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MutationReport {
    pub resource_id: String,
    pub previous_sha256: Option<String>,
    pub new_sha256: Option<String>,
    pub history_added: bool,
    pub cache_paths: Vec<String>,
    pub no_op: bool,
}

// ─── Helpers ────────────────────────────────────────────────────────────

fn lockfile_path(profile_config_dir: &str) -> PathBuf {
    PathBuf::from(profile_config_dir).join(LOCKFILE_NAME)
}

fn lock_path(profile_config_dir: &str) -> PathBuf {
    PathBuf::from(profile_config_dir).join(LOCK_FILE)
}

fn cache_root(profile_config_dir: &str) -> PathBuf {
    PathBuf::from(profile_config_dir).join(CACHE_DIR)
}

fn resource_id(kind: ResourceKind, name: &str) -> String {
    format!("{}/{}", kind.dir_name(), name)
}

/// Files owned by a resource, relative to the profile directory.
/// Skills live in their own subdir; everything else is `<kind>/<name>.md`
/// plus an optional sidecar `<name>.weplex.yaml`.
fn resource_files(kind: ResourceKind, name: &str, has_sidecar: bool) -> Vec<String> {
    match kind {
        ResourceKind::Skill => {
            // Skills are directories: `skills/<name>/SKILL.md`. The body
            // path lives at SKILL.md; sidecars beside it.
            let mut v = vec![format!("skills/{}/SKILL.md", name)];
            if has_sidecar {
                v.push(format!("skills/{}/SKILL.weplex.yaml", name));
            }
            v
        }
        _ => {
            let mut v = vec![format!("{}/{}.md", kind.dir_name(), name)];
            if has_sidecar {
                v.push(format!("{}/{}.weplex.yaml", kind.dir_name(), name));
            }
            v
        }
    }
}

/// Body file path for a resource, relative to profile dir.
fn body_rel_path(kind: ResourceKind, name: &str) -> String {
    match kind {
        ResourceKind::Skill => format!("skills/{}/SKILL.md", name),
        _ => format!("{}/{}.md", kind.dir_name(), name),
    }
}

/// Sidecar manifest file path, relative to profile dir.
fn sidecar_rel_path(kind: ResourceKind, name: &str) -> String {
    match kind {
        ResourceKind::Skill => format!("skills/{}/SKILL.weplex.yaml", name),
        _ => format!("{}/{}.weplex.yaml", kind.dir_name(), name),
    }
}

/// Open the lockfile lock with an exclusive flock. Returns the open file;
/// drop it to release.
fn acquire_lockfile_lock(profile_config_dir: &str) -> Result<std::fs::File, LockfileError> {
    use fs2::FileExt;
    let path = lock_path(profile_config_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| LockfileError::Io(format!("create lock dir {}: {}", parent.display(), e)))?;
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&path)
        .map_err(|e| LockfileError::Io(format!("open lock {}: {}", path.display(), e)))?;
    file.try_lock_exclusive().map_err(|e| {
        LockfileError::Io(format!(
            "lockfile already in use for {}: {}",
            profile_config_dir, e
        ))
    })?;
    Ok(file)
}

fn current_user() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}

// ─── Load / Save ────────────────────────────────────────────────────────

/// Load the lockfile for a profile. Tolerant: missing file → empty
/// Lockfile; malformed YAML → empty Lockfile + warn-log. Drift is NOT
/// computed here — call `reconcile_on_load` separately.
pub fn load_lockfile(profile_config_dir: &str) -> Lockfile {
    let path = lockfile_path(profile_config_dir);
    let raw = match std::fs::read_to_string(&path) {
        Ok(r) => r,
        Err(_) => {
            return Lockfile {
                version: LOCKFILE_VERSION,
                generated_by: default_generated_by(),
                ..Default::default()
            };
        }
    };
    match serde_yml::from_str::<Lockfile>(&raw) {
        Ok(mut lf) => {
            if lf.version == 0 {
                lf.version = LOCKFILE_VERSION;
            }
            if lf.generated_by.is_empty() {
                lf.generated_by = default_generated_by();
            }
            lf
        }
        Err(e) => {
            log::warn!(
                "lockfile at {} is malformed, treating as empty: {}",
                path.display(),
                e
            );
            Lockfile {
                version: LOCKFILE_VERSION,
                generated_by: default_generated_by(),
                ..Default::default()
            }
        }
    }
}

fn save_lockfile(profile_config_dir: &str, lock: &Lockfile) -> Result<(), LockfileError> {
    let path = lockfile_path(profile_config_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            LockfileError::Io(format!("create lockfile parent {}: {}", parent.display(), e))
        })?;
    }
    let yaml = serde_yml::to_string(lock)
        .map_err(|e| LockfileError::Parse(format!("serialize lockfile: {}", e)))?;
    let path_str = path.to_str().ok_or_else(|| {
        LockfileError::Io(format!("lockfile path not UTF-8: {}", path.display()))
    })?;
    atomic_write_user_readable(path_str, &yaml).map_err(LockfileError::Io)
}

// ─── apply_resource_mutation ────────────────────────────────────────────

/// THE central entry point for all resource mutations on a profile.
///
/// Order of operations:
/// 1. acquire exclusive lockfile lock
/// 2. read current lockfile (tolerant)
/// 3. compute new sha256 of body and optional sidecar
/// 4. lookup existing entry by id
/// 5. if same sha → no-op, return
/// 6. if different (or Delete): snapshot existing files into cache, push history
/// 7. write new body+sidecar atomically (Upsert) or delete files (Delete)
/// 8. update or remove the entry
/// 9. run cache GC (best-effort)
/// 10. atomic-write lockfile
pub fn apply_resource_mutation(
    profile_config_dir: &str,
    kind: ResourceKind,
    name: &str,
    source: ResourceSource,
    mutation: MutationKind,
) -> Result<MutationReport, LockfileError> {
    let safe_name = sanitize_name(name).map_err(LockfileError::Parse)?;
    let id = resource_id(kind, &safe_name);

    let _lock = acquire_lockfile_lock(profile_config_dir)?;
    let mut lf = load_lockfile(profile_config_dir);

    // Find existing entry index (if any).
    let existing_idx = lf.resources.iter().position(|e| e.id == id);

    // Compute new shas (None for Delete).
    let (new_body_sha, new_sidecar_sha, new_body, new_sidecar) = match &mutation {
        MutationKind::Upsert { body, sidecar } => {
            let bsha = sha256_hex(body.as_bytes());
            let ssha = sidecar.as_ref().map(|s| sha256_hex(s.as_bytes()));
            (Some(bsha), ssha, Some(body.clone()), sidecar.clone())
        }
        MutationKind::Delete => (None, None, None, None),
    };

    // Same-sha no-op short-circuit.
    if let Some(idx) = existing_idx {
        if let MutationKind::Upsert { .. } = &mutation {
            let entry = &lf.resources[idx];
            if Some(&entry.sha256) == new_body_sha.as_ref()
                && entry.sidecar_sha256 == new_sidecar_sha
            {
                return Ok(MutationReport {
                    resource_id: id,
                    previous_sha256: Some(entry.sha256.clone()),
                    new_sha256: new_body_sha,
                    history_added: false,
                    cache_paths: Vec::new(),
                    no_op: true,
                });
            }
        }
    }

    // Snapshot existing files into cache (before overwriting/deleting).
    let mut history_added = false;
    let mut snapshot_paths: Vec<String> = Vec::new();
    let previous_sha256 = existing_idx.map(|i| lf.resources[i].sha256.clone());

    if let Some(idx) = existing_idx {
        let entry = lf.resources[idx].clone();
        let sha16: String = entry.sha256.chars().take(16).collect();
        let cache_dir = cache_root(profile_config_dir).join(&sha16);

        for rel in &entry.files {
            let src = PathBuf::from(profile_config_dir).join(rel);
            if !src.exists() {
                continue;
            }
            let dst = cache_dir.join(rel);
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    LockfileError::Io(format!("create cache parent {}: {}", parent.display(), e))
                })?;
            }
            std::fs::copy(&src, &dst).map_err(|e| {
                LockfileError::Io(format!(
                    "snapshot {} → {}: {}",
                    src.display(),
                    dst.display(),
                    e
                ))
            })?;
            snapshot_paths.push(format!("{}/{}/{}", CACHE_DIR, sha16, rel));
        }

        if !snapshot_paths.is_empty() {
            let hist = LockfileHistoryEntry {
                version: entry.version.clone(),
                sha256: entry.sha256.clone(),
                sidecar_sha256: entry.sidecar_sha256.clone(),
                source: entry.source,
                installed_at: entry.installed_at,
                cache_paths: snapshot_paths.clone(),
            };
            lf.history.entry(id.clone()).or_default().push(hist);
            history_added = true;
        }
    }

    // Apply the mutation on disk.
    match &mutation {
        MutationKind::Upsert { .. } => {
            let body = new_body.as_ref().expect("Upsert body present");
            let body_rel = body_rel_path(kind, &safe_name);
            let body_abs = PathBuf::from(profile_config_dir).join(&body_rel);
            if let Some(parent) = body_abs.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    LockfileError::Io(format!(
                        "create body parent {}: {}",
                        parent.display(),
                        e
                    ))
                })?;
            }
            let body_abs_str = body_abs.to_str().ok_or_else(|| {
                LockfileError::Io(format!("body path not UTF-8: {}", body_abs.display()))
            })?;
            atomic_write_user_readable(body_abs_str, body).map_err(LockfileError::Io)?;

            if let Some(sidecar) = &new_sidecar {
                let s_rel = sidecar_rel_path(kind, &safe_name);
                let s_abs = PathBuf::from(profile_config_dir).join(&s_rel);
                if let Some(parent) = s_abs.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| {
                        LockfileError::Io(format!(
                            "create sidecar parent {}: {}",
                            parent.display(),
                            e
                        ))
                    })?;
                }
                let s_abs_str = s_abs.to_str().ok_or_else(|| {
                    LockfileError::Io(format!("sidecar path not UTF-8: {}", s_abs.display()))
                })?;
                atomic_write_user_readable(s_abs_str, sidecar).map_err(LockfileError::Io)?;
            } else {
                // No new sidecar requested: if an old one exists, leave
                // it alone unless this is a clean overwrite where the
                // previous entry HAD a sidecar (then drop it).
                if let Some(idx) = existing_idx
                    && lf.resources[idx].sidecar_sha256.is_some()
                {
                    let s_rel = sidecar_rel_path(kind, &safe_name);
                    let s_abs = PathBuf::from(profile_config_dir).join(&s_rel);
                    let _ = std::fs::remove_file(&s_abs);
                }
            }

            let now = Utc::now();
            let new_entry = LockfileEntry {
                id: id.clone(),
                kind,
                source,
                version: None,
                sha256: new_body_sha.clone().expect("upsert body sha"),
                sidecar_sha256: new_sidecar_sha,
                files: resource_files(kind, &safe_name, new_sidecar.is_some()),
                installed_at: now,
                installed_by: current_user(),
                drifted: false,
            };
            if let Some(idx) = existing_idx {
                lf.resources[idx] = new_entry;
            } else {
                lf.resources.push(new_entry);
            }
        }
        MutationKind::Delete => {
            if let Some(idx) = existing_idx {
                let entry = lf.resources.remove(idx);
                for rel in &entry.files {
                    let abs = PathBuf::from(profile_config_dir).join(rel);
                    let _ = std::fs::remove_file(&abs);
                }
                // For skills, attempt to remove the now-empty skill dir.
                if matches!(kind, ResourceKind::Skill) {
                    let sd = PathBuf::from(profile_config_dir)
                        .join(format!("skills/{}", safe_name));
                    let _ = std::fs::remove_dir(&sd);
                }
            } else {
                return Err(LockfileError::NotFound(format!(
                    "resource {} not in lockfile",
                    id
                )));
            }
        }
    }

    // Prune the in-memory lockfile in place, then save it. We DON'T call
    // `run_cache_gc` here because that function loads from disk — and our
    // in-memory `lf` is the authoritative new state. Calling it would
    // create a save/save race that overwrites the GC pruning. Instead,
    // mutate `lf` directly and rely on `gc_prune_history` + the on-disk
    // sweep below.
    gc_prune_history_in_memory(&mut lf);

    save_lockfile(profile_config_dir, &lf)?;

    // Sweep cache directories not referenced by current+history. Best
    // effort: we already saved the lockfile, so failure here doesn't
    // corrupt state.
    if let Err(e) = sweep_cache_dirs(profile_config_dir, &lf) {
        log::warn!("cache sweep failed for {}: {}", profile_config_dir, e);
    }

    Ok(MutationReport {
        resource_id: id,
        previous_sha256,
        new_sha256: new_body_sha,
        history_added,
        cache_paths: snapshot_paths,
        no_op: false,
    })
}

// ─── Restore (rollback) ─────────────────────────────────────────────────

/// Restore a previous version of a resource from cache.
///
/// Verifies the cached body sha256 BEFORE touching disk. The current
/// version is pushed to history first via the normal mutation flow.
pub fn restore_resource(
    profile_config_dir: &str,
    resource_id_str: &str,
    target_sha256: &str,
) -> Result<MutationReport, LockfileError> {
    let lf = load_lockfile(profile_config_dir);

    let history = lf
        .history
        .get(resource_id_str)
        .ok_or_else(|| LockfileError::NotFound(format!("no history for {}", resource_id_str)))?;
    let target = history
        .iter()
        .find(|h| h.sha256 == target_sha256)
        .ok_or_else(|| {
            LockfileError::NotFound(format!(
                "no history entry with sha {} for {}",
                target_sha256, resource_id_str
            ))
        })?;

    // Locate the cache body file (the .md, not the sidecar).
    let body_cache = target
        .cache_paths
        .iter()
        .find(|p| !p.ends_with(".weplex.yaml"))
        .ok_or_else(|| {
            LockfileError::InvalidArchive(format!(
                "history entry for {} has no body cache path",
                resource_id_str
            ))
        })?;

    let body_abs = PathBuf::from(profile_config_dir).join(body_cache);
    let body_abs_str = body_abs.to_str().ok_or_else(|| {
        LockfileError::Io(format!("cache body path not UTF-8: {}", body_abs.display()))
    })?;
    let actual = sha256_file(body_abs_str).map_err(LockfileError::Io)?;
    if actual != target_sha256 {
        return Err(LockfileError::Sha256Mismatch {
            expected: target_sha256.to_string(),
            got: actual,
        });
    }

    let body_bytes = std::fs::read_to_string(body_abs_str).map_err(|e| {
        LockfileError::Io(format!("read cache body {}: {}", body_abs.display(), e))
    })?;

    // Sidecar (optional): same verification.
    let sidecar_cache = target
        .cache_paths
        .iter()
        .find(|p| p.ends_with(".weplex.yaml"));
    let sidecar_bytes = if let Some(sc) = sidecar_cache {
        let abs = PathBuf::from(profile_config_dir).join(sc);
        let abs_str = abs.to_str().ok_or_else(|| {
            LockfileError::Io(format!("cache sidecar path not UTF-8: {}", abs.display()))
        })?;
        let actual = sha256_file(abs_str).map_err(LockfileError::Io)?;
        if let Some(expected) = target.sidecar_sha256.as_deref()
            && actual != expected
        {
            return Err(LockfileError::Sha256Mismatch {
                expected: expected.to_string(),
                got: actual,
            });
        }
        Some(
            std::fs::read_to_string(abs_str).map_err(|e| {
                LockfileError::Io(format!("read cache sidecar {}: {}", abs.display(), e))
            })?,
        )
    } else {
        None
    };

    // Recover (kind, name) from the resource id (`<kind_dir>/<name>`).
    let (kind, name) = parse_resource_id(resource_id_str)?;

    apply_resource_mutation(
        profile_config_dir,
        kind,
        &name,
        target.source,
        MutationKind::Upsert {
            body: body_bytes,
            sidecar: sidecar_bytes,
        },
    )
}

fn parse_resource_id(id: &str) -> Result<(ResourceKind, String), LockfileError> {
    let (dir, name) = id
        .split_once('/')
        .ok_or_else(|| LockfileError::Parse(format!("invalid resource id: {}", id)))?;
    let kind = match dir {
        "agents" => ResourceKind::Agent,
        "rules" => ResourceKind::Rule,
        "skills" => ResourceKind::Skill,
        "commands" => ResourceKind::Command,
        other => {
            return Err(LockfileError::Parse(format!(
                "unknown resource kind dir: {}",
                other
            )));
        }
    };
    Ok((kind, name.to_string()))
}

// ─── Cache GC ───────────────────────────────────────────────────────────

/// Prune history in-memory according to the GC policy:
/// - drop entries older than MAX_HISTORY_AGE_DAYS
/// - per id: keep at most MAX_HISTORY_PER_RESOURCE most-recent
/// - drop empty history buckets entirely
fn gc_prune_history_in_memory(lf: &mut Lockfile) {
    let cutoff = Utc::now() - chrono::Duration::days(MAX_HISTORY_AGE_DAYS);
    for entries in lf.history.values_mut() {
        entries.retain(|e| e.installed_at >= cutoff);
        if entries.len() > MAX_HISTORY_PER_RESOURCE {
            entries.sort_by(|a, b| a.installed_at.cmp(&b.installed_at));
            let drop_count = entries.len() - MAX_HISTORY_PER_RESOURCE;
            entries.drain(..drop_count);
        }
    }
    lf.history.retain(|_, v| !v.is_empty());
}

/// Walk the cache dir; remove sha16 subdirs not referenced by any current
/// or history entry. Returns deleted count.
fn sweep_cache_dirs(profile_config_dir: &str, lf: &Lockfile) -> Result<u32, LockfileError> {
    let mut live: std::collections::HashSet<String> = std::collections::HashSet::new();
    for r in &lf.resources {
        let sha16: String = r.sha256.chars().take(16).collect();
        if !sha16.is_empty() {
            live.insert(sha16);
        }
    }
    for entries in lf.history.values() {
        for e in entries {
            let sha16: String = e.sha256.chars().take(16).collect();
            if !sha16.is_empty() {
                live.insert(sha16);
            }
        }
    }
    let cache = cache_root(profile_config_dir);
    let mut deleted = 0u32;
    if let Ok(entries) = std::fs::read_dir(&cache) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = entry.metadata() else { continue };
            if !meta.is_dir() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
            if !live.contains(name) {
                if let Err(e) = std::fs::remove_dir_all(&path) {
                    log::warn!("gc: remove {} failed: {}", path.display(), e);
                } else {
                    deleted += 1;
                }
            }
        }
    }
    Ok(deleted)
}

/// Run cache garbage collection. Returns count of deleted sha16 dirs.
///
/// Policy:
/// - per id: keep at most MAX_HISTORY_PER_RESOURCE most-recent entries
/// - drop entries older than MAX_HISTORY_AGE_DAYS
/// - the current entry (in `resources`) is NEVER GC'd
/// - after pruning history in-memory, rewrite the lockfile and remove
///   any sha16 directory under `.weplex/cache/` not referenced by any
///   live entry or remaining history entry
///
/// Standalone version: takes the lockfile lock to avoid stomping a
/// concurrent mutation.
pub fn run_cache_gc(profile_config_dir: &str) -> Result<u32, LockfileError> {
    let _lock = acquire_lockfile_lock(profile_config_dir)?;
    let mut lf = load_lockfile(profile_config_dir);
    gc_prune_history_in_memory(&mut lf);
    save_lockfile(profile_config_dir, &lf)?;
    sweep_cache_dirs(profile_config_dir, &lf)
}

// ─── Drift detection ────────────────────────────────────────────────────

/// Compare each entry's recorded sha against the current on-disk sha.
/// Marks `entry.drifted = true` when bytes diverge. The `drifted` field
/// is `#[serde(skip)]` and never persists.
pub fn reconcile_on_load(lock: &mut Lockfile, profile_config_dir: &str) {
    for entry in lock.resources.iter_mut() {
        let mut drifted = false;
        let body_rel = entry
            .files
            .iter()
            .find(|f| !f.ends_with(".weplex.yaml"))
            .cloned();
        if let Some(body) = body_rel {
            let abs = PathBuf::from(profile_config_dir).join(&body);
            match sha256_file(abs.to_string_lossy().as_ref()) {
                Ok(actual) if actual != entry.sha256 => drifted = true,
                Err(_) => drifted = true,
                _ => {}
            }
        }
        if !drifted
            && let Some(expected) = entry.sidecar_sha256.as_deref()
        {
            let sidecar_rel = entry.files.iter().find(|f| f.ends_with(".weplex.yaml"));
            match sidecar_rel {
                Some(sc) => {
                    let abs = PathBuf::from(profile_config_dir).join(sc);
                    match sha256_file(abs.to_string_lossy().as_ref()) {
                        Ok(actual) if actual != expected => drifted = true,
                        Err(_) => drifted = true,
                        _ => {}
                    }
                }
                None => drifted = true,
            }
        }
        entry.drifted = drifted;
    }
}

// ─── Tauri command: read_lockfile ───────────────────────────────────────

/// Tauri command: load the lockfile (with reconcile/drift markers).
/// Returns `Result<Lockfile, String>`.
#[tauri::command]
pub fn read_lockfile(profile_config_dir: String) -> Result<Lockfile, String> {
    let dir = validate_config_dir(&profile_config_dir).map_err(|e| redact_home(&e))?;
    let mut lf = load_lockfile(&dir);
    reconcile_on_load(&mut lf, &dir);
    Ok(lf)
}

/// Tauri command: restore a resource to a previous sha.
#[tauri::command]
pub fn restore_resource_version(
    profile_config_dir: String,
    resource_id: String,
    target_sha256: String,
) -> Result<MutationReport, String> {
    let dir = validate_config_dir(&profile_config_dir).map_err(|e| redact_home(&e))?;
    restore_resource(&dir, &resource_id, &target_sha256)
        .map_err(|e| redact_home(&format!("{}", e)))
}

// ─── Path redaction ─────────────────────────────────────────────────────

/// Replace a leading $HOME with `~` so error strings don't leak the
/// user's home path. Mirrors `guard::redact_home`.
fn redact_home(s: &str) -> String {
    let home = get_home();
    if !home.is_empty() && s.starts_with(&home) {
        format!("~{}", &s[home.len()..])
    } else {
        s.to_string()
    }
}

// ─── Export / Import / Migration (stubs filled in subsequent commits) ───
//
// These are declared here so the public surface in the architect plan is
// reserved. The actual implementations land in the next commits to keep
// each commit focused. `#[allow(dead_code)]` is intentional here — the
// types are wired up in commits D/H.

#[allow(dead_code)]
pub struct ExportReport {
    pub archive_path: String,
    pub bytes: u64,
    pub resource_count: usize,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveInspection {
    pub schema_version: u32,
    pub generated_by: String,
    pub resource_count: usize,
    pub conflicts: Vec<ConflictItem>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictItem {
    pub resource_id: String,
    pub existing_sha256: String,
    pub incoming_sha256: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConflictPolicy {
    OverwriteAll,
    SkipAll,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub installed: usize,
    pub skipped: usize,
    pub overwritten: usize,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationReport {
    pub already_done: bool,
    pub migrated_agents: u32,
    pub migrated_skills: u32,
}

// ─── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(label: &str) -> std::path::PathBuf {
        let base = std::env::temp_dir().join(format!(
            "weplex-lockfile-test-{}-{}-{}",
            label,
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
    fn lockfile_v1_parses() {
        let now = Utc::now();
        let lf = Lockfile {
            version: 1,
            generated_by: "weplex".into(),
            resources: vec![LockfileEntry {
                id: "agents/architect".into(),
                kind: ResourceKind::Agent,
                source: ResourceSource::Builtin,
                version: Some("1.0.0".into()),
                sha256: "a".repeat(64),
                sidecar_sha256: None,
                files: vec!["agents/architect.md".into()],
                installed_at: now,
                installed_by: "tester".into(),
                drifted: false,
            }],
            history: BTreeMap::new(),
        };
        let yaml = serde_yml::to_string(&lf).unwrap();
        let parsed: Lockfile = serde_yml::from_str(&yaml).unwrap();
        assert_eq!(parsed.version, 1);
        assert_eq!(parsed.resources.len(), 1);
        assert_eq!(parsed.resources[0].id, "agents/architect");
    }

    #[test]
    fn lockfile_missing_file_returns_empty() {
        let dir = tmpdir("missing");
        let lf = load_lockfile(dir.to_str().unwrap());
        assert_eq!(lf.version, 1);
        assert!(lf.resources.is_empty());
        assert!(lf.history.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn lockfile_malformed_returns_empty_logs_warn() {
        let dir = tmpdir("malformed");
        let path = dir.join(LOCKFILE_NAME);
        std::fs::write(&path, "{[not yaml at all").unwrap();
        let lf = load_lockfile(dir.to_str().unwrap());
        assert!(lf.resources.is_empty());
        assert!(lf.history.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_mutation_first_write_creates_entry_no_history() {
        let dir = tmpdir("first-write");
        let report = apply_resource_mutation(
            dir.to_str().unwrap(),
            ResourceKind::Agent,
            "architect",
            ResourceSource::User,
            MutationKind::Upsert {
                body: "# architect".into(),
                sidecar: None,
            },
        )
        .unwrap();
        assert!(!report.no_op);
        assert!(!report.history_added);
        assert_eq!(report.previous_sha256, None);
        assert!(report.new_sha256.is_some());

        // File was written.
        assert!(dir.join("agents/architect.md").exists());
        // Lockfile was written.
        let lf = load_lockfile(dir.to_str().unwrap());
        assert_eq!(lf.resources.len(), 1);
        assert_eq!(lf.resources[0].id, "agents/architect");
        assert_eq!(lf.resources[0].source, ResourceSource::User);
        assert!(lf.history.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_mutation_same_sha_is_noop() {
        let dir = tmpdir("noop");
        let body = "same-content".to_string();
        apply_resource_mutation(
            dir.to_str().unwrap(),
            ResourceKind::Agent,
            "a1",
            ResourceSource::User,
            MutationKind::Upsert {
                body: body.clone(),
                sidecar: None,
            },
        )
        .unwrap();
        let report = apply_resource_mutation(
            dir.to_str().unwrap(),
            ResourceKind::Agent,
            "a1",
            ResourceSource::User,
            MutationKind::Upsert {
                body,
                sidecar: None,
            },
        )
        .unwrap();
        assert!(report.no_op);
        assert!(!report.history_added);
        let lf = load_lockfile(dir.to_str().unwrap());
        assert!(lf.history.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_mutation_different_sha_pushes_history_to_cache() {
        let dir = tmpdir("hist");
        apply_resource_mutation(
            dir.to_str().unwrap(),
            ResourceKind::Agent,
            "a1",
            ResourceSource::User,
            MutationKind::Upsert {
                body: "v1".into(),
                sidecar: None,
            },
        )
        .unwrap();
        let report = apply_resource_mutation(
            dir.to_str().unwrap(),
            ResourceKind::Agent,
            "a1",
            ResourceSource::User,
            MutationKind::Upsert {
                body: "v2".into(),
                sidecar: None,
            },
        )
        .unwrap();
        assert!(report.history_added);
        assert_eq!(report.cache_paths.len(), 1);
        assert!(report.cache_paths[0].starts_with(".weplex/cache/"));
        // Cache file actually exists on disk.
        let cached = dir.join(&report.cache_paths[0]);
        assert!(cached.exists());
        // Body now reflects v2.
        assert_eq!(
            std::fs::read_to_string(dir.join("agents/a1.md")).unwrap(),
            "v2"
        );
        let lf = load_lockfile(dir.to_str().unwrap());
        assert_eq!(lf.history.get("agents/a1").unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_mutation_delete_keeps_history() {
        let dir = tmpdir("delete");
        apply_resource_mutation(
            dir.to_str().unwrap(),
            ResourceKind::Agent,
            "a1",
            ResourceSource::User,
            MutationKind::Upsert {
                body: "v1".into(),
                sidecar: None,
            },
        )
        .unwrap();
        let report = apply_resource_mutation(
            dir.to_str().unwrap(),
            ResourceKind::Agent,
            "a1",
            ResourceSource::User,
            MutationKind::Delete,
        )
        .unwrap();
        assert!(report.history_added);
        // Body file is gone.
        assert!(!dir.join("agents/a1.md").exists());
        let lf = load_lockfile(dir.to_str().unwrap());
        // Resource removed from current list.
        assert!(lf.resources.is_empty());
        // History keeps it.
        assert_eq!(lf.history.get("agents/a1").unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cache_files_match_history_paths() {
        let dir = tmpdir("cache-match");
        apply_resource_mutation(
            dir.to_str().unwrap(),
            ResourceKind::Agent,
            "a1",
            ResourceSource::User,
            MutationKind::Upsert {
                body: "v1".into(),
                sidecar: None,
            },
        )
        .unwrap();
        apply_resource_mutation(
            dir.to_str().unwrap(),
            ResourceKind::Agent,
            "a1",
            ResourceSource::User,
            MutationKind::Upsert {
                body: "v2".into(),
                sidecar: None,
            },
        )
        .unwrap();
        let lf = load_lockfile(dir.to_str().unwrap());
        for entries in lf.history.values() {
            for e in entries {
                for p in &e.cache_paths {
                    assert!(dir.join(p).exists(), "missing cache path: {}", p);
                }
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn restore_verifies_cache_sha256_writes_atomically() {
        let dir = tmpdir("restore-ok");
        apply_resource_mutation(
            dir.to_str().unwrap(),
            ResourceKind::Agent,
            "a1",
            ResourceSource::User,
            MutationKind::Upsert {
                body: "v1".into(),
                sidecar: None,
            },
        )
        .unwrap();
        let v1_sha = sha256_hex("v1".as_bytes());
        apply_resource_mutation(
            dir.to_str().unwrap(),
            ResourceKind::Agent,
            "a1",
            ResourceSource::User,
            MutationKind::Upsert {
                body: "v2".into(),
                sidecar: None,
            },
        )
        .unwrap();
        // Restore back to v1.
        let report = restore_resource(dir.to_str().unwrap(), "agents/a1", &v1_sha).unwrap();
        assert!(!report.no_op);
        assert_eq!(
            std::fs::read_to_string(dir.join("agents/a1.md")).unwrap(),
            "v1"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn restore_with_tampered_cache_returns_sha_mismatch_no_disk_write() {
        let dir = tmpdir("restore-tamper");
        apply_resource_mutation(
            dir.to_str().unwrap(),
            ResourceKind::Agent,
            "a1",
            ResourceSource::User,
            MutationKind::Upsert {
                body: "v1".into(),
                sidecar: None,
            },
        )
        .unwrap();
        let v1_sha = sha256_hex("v1".as_bytes());
        apply_resource_mutation(
            dir.to_str().unwrap(),
            ResourceKind::Agent,
            "a1",
            ResourceSource::User,
            MutationKind::Upsert {
                body: "v2".into(),
                sidecar: None,
            },
        )
        .unwrap();
        // Tamper with the cache.
        let lf = load_lockfile(dir.to_str().unwrap());
        let cache_path = lf.history.get("agents/a1").unwrap()[0].cache_paths[0].clone();
        std::fs::write(dir.join(&cache_path), "tampered").unwrap();

        let result = restore_resource(dir.to_str().unwrap(), "agents/a1", &v1_sha);
        assert!(matches!(result, Err(LockfileError::Sha256Mismatch { .. })));
        // v2 still on disk (not overwritten).
        assert_eq!(
            std::fs::read_to_string(dir.join("agents/a1.md")).unwrap(),
            "v2"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn restore_pushes_current_to_history_before_restoring() {
        let dir = tmpdir("restore-history");
        apply_resource_mutation(
            dir.to_str().unwrap(),
            ResourceKind::Agent,
            "a1",
            ResourceSource::User,
            MutationKind::Upsert {
                body: "v1".into(),
                sidecar: None,
            },
        )
        .unwrap();
        let v1_sha = sha256_hex("v1".as_bytes());
        apply_resource_mutation(
            dir.to_str().unwrap(),
            ResourceKind::Agent,
            "a1",
            ResourceSource::User,
            MutationKind::Upsert {
                body: "v2".into(),
                sidecar: None,
            },
        )
        .unwrap();
        // Two history entries before restore: just v1. After restore: v1 + v2.
        restore_resource(dir.to_str().unwrap(), "agents/a1", &v1_sha).unwrap();
        let lf = load_lockfile(dir.to_str().unwrap());
        let h = lf.history.get("agents/a1").unwrap();
        assert!(h.len() >= 2, "expected at least 2 history entries, got {}", h.len());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reconcile_marks_externally_modified_drifted() {
        let dir = tmpdir("drift");
        apply_resource_mutation(
            dir.to_str().unwrap(),
            ResourceKind::Agent,
            "a1",
            ResourceSource::User,
            MutationKind::Upsert {
                body: "v1".into(),
                sidecar: None,
            },
        )
        .unwrap();
        // Modify the body file behind our back.
        std::fs::write(dir.join("agents/a1.md"), "tampered").unwrap();
        let mut lf = load_lockfile(dir.to_str().unwrap());
        reconcile_on_load(&mut lf, dir.to_str().unwrap());
        assert!(lf.resources[0].drifted);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn drift_state_does_not_persist_through_save_load() {
        let dir = tmpdir("drift-noserialize");
        apply_resource_mutation(
            dir.to_str().unwrap(),
            ResourceKind::Agent,
            "a1",
            ResourceSource::User,
            MutationKind::Upsert {
                body: "v1".into(),
                sidecar: None,
            },
        )
        .unwrap();
        let mut lf = load_lockfile(dir.to_str().unwrap());
        lf.resources[0].drifted = true;
        save_lockfile(dir.to_str().unwrap(), &lf).unwrap();
        let lf2 = load_lockfile(dir.to_str().unwrap());
        assert!(!lf2.resources[0].drifted);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn gc_keeps_10_entries_drops_older() {
        let dir = tmpdir("gc-10");
        // Push 12 versions; expect 10 retained in history (the 11th is current).
        for i in 0..12 {
            apply_resource_mutation(
                dir.to_str().unwrap(),
                ResourceKind::Agent,
                "a1",
                ResourceSource::User,
                MutationKind::Upsert {
                    body: format!("body-{}", i),
                    sidecar: None,
                },
            )
            .unwrap();
        }
        let lf = load_lockfile(dir.to_str().unwrap());
        let h = lf.history.get("agents/a1").unwrap();
        assert!(
            h.len() <= MAX_HISTORY_PER_RESOURCE,
            "history len {}, cap {}",
            h.len(),
            MAX_HISTORY_PER_RESOURCE
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn gc_drops_entries_older_than_30d() {
        let dir = tmpdir("gc-age");
        apply_resource_mutation(
            dir.to_str().unwrap(),
            ResourceKind::Agent,
            "a1",
            ResourceSource::User,
            MutationKind::Upsert {
                body: "v1".into(),
                sidecar: None,
            },
        )
        .unwrap();
        apply_resource_mutation(
            dir.to_str().unwrap(),
            ResourceKind::Agent,
            "a1",
            ResourceSource::User,
            MutationKind::Upsert {
                body: "v2".into(),
                sidecar: None,
            },
        )
        .unwrap();
        // Doctor the history entry to be 31 days old.
        let mut lf = load_lockfile(dir.to_str().unwrap());
        for e in lf.history.values_mut().flatten() {
            e.installed_at = Utc::now() - chrono::Duration::days(31);
        }
        save_lockfile(dir.to_str().unwrap(), &lf).unwrap();
        run_cache_gc(dir.to_str().unwrap()).unwrap();
        let lf = load_lockfile(dir.to_str().unwrap());
        // History fully pruned by age cutoff.
        assert!(lf.history.is_empty(), "expected empty, got {:?}", lf.history);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn gc_never_drops_current_resource() {
        let dir = tmpdir("gc-current");
        apply_resource_mutation(
            dir.to_str().unwrap(),
            ResourceKind::Agent,
            "a1",
            ResourceSource::User,
            MutationKind::Upsert {
                body: "v1".into(),
                sidecar: None,
            },
        )
        .unwrap();
        run_cache_gc(dir.to_str().unwrap()).unwrap();
        let lf = load_lockfile(dir.to_str().unwrap());
        assert_eq!(lf.resources.len(), 1);
        assert!(dir.join("agents/a1.md").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

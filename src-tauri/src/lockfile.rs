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
/// concurrent mutation. Called at startup for every discoverable profile.
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

// ─── Export / Import ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportReport {
    pub archive_path: String,
    pub bytes: u64,
    pub resource_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveInspection {
    pub schema_version: u32,
    pub generated_by: String,
    pub resource_count: usize,
    pub conflicts: Vec<ConflictItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictItem {
    pub resource_id: String,
    pub existing_sha256: String,
    pub incoming_sha256: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConflictPolicy {
    OverwriteAll,
    SkipAll,
}

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

// ─── Excluded paths ─────────────────────────────────────────────────────

/// Paths under `.weplex/` that MUST NEVER be exported. Encrypted notes,
/// override locks, internal ledgers, session maps. Imports also reject
/// these paths to prevent a malicious archive from planting one.
const WEPLEX_INTERNAL_EXCLUDED: &[&str] = &[
    ".weplex/lockfile.lock",
    ".weplex/overrides.json",
    ".weplex/compile-ledger.json",
    ".weplex/legacy-weplex-migrated.flag",
    ".weplex/session-map",
    ".weplex/activity",
];

/// Returns true if the given relative path should be EXCLUDED from the
/// export tarball.
fn is_excluded_from_export(rel: &str) -> bool {
    for prefix in WEPLEX_INTERNAL_EXCLUDED {
        if rel == *prefix || rel.starts_with(&format!("{}/", prefix)) {
            return true;
        }
    }
    false
}

/// Returns true if a relative path is one of the canonical resource
/// directories or .weplex/cache. The lockfile sits at the root.
fn is_archive_path_allowed(rel: &str) -> bool {
    if rel == LOCKFILE_NAME {
        return true;
    }
    let allowed_prefixes = [
        "agents/",
        "rules/",
        "skills/",
        "commands/",
        ".weplex/cache/",
    ];
    for p in &allowed_prefixes {
        if rel.starts_with(p) {
            return true;
        }
    }
    false
}

// ─── Export ─────────────────────────────────────────────────────────────

/// Export every tracked resource + cache into a gzipped tarball.
/// The lockfile is the first archive entry so callers can stream-inspect.
pub fn export_profile_to_archive(
    profile_config_dir: &str,
    output_path: &str,
) -> Result<ExportReport, LockfileError> {
    let lf = load_lockfile(profile_config_dir);
    let resource_count = lf.resources.len();

    // Collect every (rel_path, abs_path) we want to put in the tarball,
    // in stable lexicographic order for reproducibility.
    let mut entries: Vec<(String, PathBuf)> = Vec::new();

    let lockfile_abs = PathBuf::from(profile_config_dir).join(LOCKFILE_NAME);
    if !lockfile_abs.exists() {
        // Write a fresh empty lockfile to disk first so the archive
        // always carries one. (Round-trip importers expect it.)
        save_lockfile(profile_config_dir, &lf)?;
    }
    entries.push((LOCKFILE_NAME.to_string(), lockfile_abs));

    // Resource files.
    let mut resource_paths: Vec<String> = Vec::new();
    for r in &lf.resources {
        for f in &r.files {
            if !is_excluded_from_export(f) {
                resource_paths.push(f.clone());
            }
        }
    }
    resource_paths.sort();
    resource_paths.dedup();
    for rel in resource_paths {
        let abs = PathBuf::from(profile_config_dir).join(&rel);
        if abs.exists() {
            entries.push((rel, abs));
        }
    }

    // Cache files referenced by history entries.
    let mut cache_paths: Vec<String> = Vec::new();
    for entries_vec in lf.history.values() {
        for e in entries_vec {
            for cp in &e.cache_paths {
                if !is_excluded_from_export(cp) {
                    cache_paths.push(cp.clone());
                }
            }
        }
    }
    cache_paths.sort();
    cache_paths.dedup();
    for rel in cache_paths {
        let abs = PathBuf::from(profile_config_dir).join(&rel);
        if abs.exists() {
            entries.push((rel, abs));
        }
    }

    // Build the tarball.
    use flate2::Compression;
    use flate2::GzBuilder;
    use std::fs::File;

    let out_file = File::create(output_path).map_err(|e| {
        LockfileError::Io(format!("create archive {}: {}", output_path, e))
    })?;
    // Zero the gzip header MTIME so two exports of the same lockfile state
    // produce byte-identical archives (the default `GzEncoder::new` writes
    // wall-clock time, breaking reproducibility).
    let enc = GzBuilder::new()
        .mtime(0)
        .write(out_file, Compression::default());
    let mut tar_builder = tar::Builder::new(enc);

    for (rel, abs) in &entries {
        if is_excluded_from_export(rel) {
            continue;
        }
        let mut f = std::fs::File::open(abs).map_err(|e| {
            LockfileError::Io(format!("open {}: {}", abs.display(), e))
        })?;
        let meta = f.metadata().map_err(|e| {
            LockfileError::Io(format!("meta {}: {}", abs.display(), e))
        })?;
        let mut header = tar::Header::new_gnu();
        header.set_size(meta.len());
        header.set_mode(0o644);
        header.set_mtime(0);
        header.set_cksum();
        tar_builder
            .append_data(&mut header, rel, &mut f)
            .map_err(|e| LockfileError::Io(format!("append {}: {}", rel, e)))?;
    }

    let enc = tar_builder
        .into_inner()
        .map_err(|e| LockfileError::Io(format!("close tar: {}", e)))?;
    enc.finish()
        .map_err(|e| LockfileError::Io(format!("gzip finish: {}", e)))?;

    let bytes = std::fs::metadata(output_path)
        .map(|m| m.len())
        .unwrap_or(0);

    Ok(ExportReport {
        archive_path: output_path.to_string(),
        bytes,
        resource_count,
    })
}

// ─── Inspect ────────────────────────────────────────────────────────────

/// Inspect an archive WITHOUT writing anything. Validates the archive
/// header, returns the lockfile metadata + a list of conflicts against
/// the current target. Caller decides whether to import.
pub fn inspect_profile_archive(
    archive_path: &str,
) -> Result<ArchiveInspection, LockfileError> {
    let lock = read_archive_lockfile(archive_path)?;
    Ok(ArchiveInspection {
        schema_version: lock.version,
        generated_by: lock.generated_by,
        resource_count: lock.resources.len(),
        conflicts: Vec::new(),
    })
}

/// Inspect an archive against a specific target — populates conflicts.
pub fn inspect_profile_archive_against(
    archive_path: &str,
    target_config_dir: &str,
) -> Result<ArchiveInspection, LockfileError> {
    let incoming = read_archive_lockfile(archive_path)?;
    let existing = load_lockfile(target_config_dir);
    let mut conflicts = Vec::new();
    for r in &incoming.resources {
        if let Some(e) = existing.resources.iter().find(|x| x.id == r.id)
            && e.sha256 != r.sha256
        {
            conflicts.push(ConflictItem {
                resource_id: r.id.clone(),
                existing_sha256: e.sha256.clone(),
                incoming_sha256: r.sha256.clone(),
            });
        }
    }
    Ok(ArchiveInspection {
        schema_version: incoming.version,
        generated_by: incoming.generated_by,
        resource_count: incoming.resources.len(),
        conflicts,
    })
}

/// Read just the lockfile from an archive (validates archive size).
fn read_archive_lockfile(archive_path: &str) -> Result<Lockfile, LockfileError> {
    use flate2::read::GzDecoder;
    use std::fs::File;

    let meta = std::fs::metadata(archive_path)
        .map_err(|e| LockfileError::Io(format!("stat {}: {}", archive_path, e)))?;
    if meta.len() > MAX_ARCHIVE_SIZE_BYTES {
        return Err(LockfileError::InvalidArchive(format!(
            "archive too large: {} bytes (max {})",
            meta.len(),
            MAX_ARCHIVE_SIZE_BYTES
        )));
    }

    let f = File::open(archive_path)
        .map_err(|e| LockfileError::Io(format!("open {}: {}", archive_path, e)))?;
    let dec = GzDecoder::new(f);
    let mut ar = tar::Archive::new(dec);

    let entries = ar.entries().map_err(|e| {
        LockfileError::InvalidArchive(format!("read tar entries: {}", e))
    })?;

    for entry_res in entries {
        let mut entry = entry_res.map_err(|e| {
            LockfileError::InvalidArchive(format!("read tar entry: {}", e))
        })?;
        let path = entry
            .path()
            .map_err(|e| LockfileError::InvalidArchive(format!("entry path: {}", e)))?;
        let path_str = path.to_string_lossy().to_string();

        if path_str == LOCKFILE_NAME {
            let header_size = entry.header().size().map_err(|e| {
                LockfileError::InvalidArchive(format!("entry size: {}", e))
            })?;
            if header_size > MAX_ARCHIVE_ENTRY_BYTES {
                return Err(LockfileError::InvalidArchive(format!(
                    "lockfile entry too large: {} bytes",
                    header_size
                )));
            }
            let mut buf = String::new();
            use std::io::Read;
            entry.read_to_string(&mut buf).map_err(|e| {
                LockfileError::InvalidArchive(format!("read lockfile entry: {}", e))
            })?;
            let lock: Lockfile = serde_yml::from_str(&buf).map_err(|e| {
                LockfileError::InvalidArchive(format!("parse lockfile: {}", e))
            })?;
            if lock.version > LOCKFILE_VERSION {
                return Err(LockfileError::InvalidArchive(format!(
                    "lockfile version {} > supported {}",
                    lock.version, LOCKFILE_VERSION
                )));
            }
            return Ok(lock);
        }

        // Encountered some other entry first. The export contract puts
        // .weplex.lock.yaml as the first entry; refuse archives that
        // don't conform — it makes streaming validation possible.
        return Err(LockfileError::InvalidArchive(
            "first archive entry must be .weplex.lock.yaml".into(),
        ));
    }

    Err(LockfileError::InvalidArchive(
        "archive contains no entries".into(),
    ))
}

// ─── Import ─────────────────────────────────────────────────────────────

/// Apply the archive to the target profile.
///
/// Two-phase: `inspect_profile_archive_against` is the read-only first
/// half. This function re-validates EVERY entry (path traversal, size
/// caps, allowed prefixes) before any disk write. Conflicts route
/// through `apply_resource_mutation` so existing versions land in
/// history.
pub fn import_profile_from_archive(
    target_config_dir: &str,
    archive_path: &str,
    policy: ConflictPolicy,
) -> Result<ImportReport, LockfileError> {
    use flate2::read::GzDecoder;
    use std::fs::File;
    use std::io::Read;

    let archive_meta = std::fs::metadata(archive_path)
        .map_err(|e| LockfileError::Io(format!("stat archive {}: {}", archive_path, e)))?;
    if archive_meta.len() > MAX_ARCHIVE_SIZE_BYTES {
        return Err(LockfileError::InvalidArchive(format!(
            "archive too large: {} bytes (max {})",
            archive_meta.len(),
            MAX_ARCHIVE_SIZE_BYTES
        )));
    }

    // Pass 1: collect entries into memory after path/size validation.
    let f = File::open(archive_path)
        .map_err(|e| LockfileError::Io(format!("open archive: {}", e)))?;
    let dec = GzDecoder::new(f);
    let mut ar = tar::Archive::new(dec);

    let entries_iter = ar
        .entries()
        .map_err(|e| LockfileError::InvalidArchive(format!("read entries: {}", e)))?;

    let mut total_uncompressed: u64 = 0;
    let mut incoming_files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut incoming_lock: Option<Lockfile> = None;
    let mut first = true;

    for entry_res in entries_iter {
        let mut entry = entry_res.map_err(|e| {
            LockfileError::InvalidArchive(format!("read entry: {}", e))
        })?;

        // Reject symlinks, hard links, char/block devs.
        let entry_type = entry.header().entry_type();
        if !matches!(
            entry_type,
            tar::EntryType::Regular | tar::EntryType::Directory
        ) {
            return Err(LockfileError::InvalidArchive(format!(
                "disallowed entry type {:?}",
                entry_type
            )));
        }

        let path = entry
            .path()
            .map_err(|e| LockfileError::InvalidArchive(format!("entry path: {}", e)))?
            .into_owned();

        // Reject any non-Normal component (absolute, ParentDir, RootDir,
        // Prefix, CurDir).
        for c in path.components() {
            match c {
                std::path::Component::Normal(_) => {}
                _ => {
                    return Err(LockfileError::InvalidArchive(format!(
                        "disallowed path component in {}",
                        path.display()
                    )));
                }
            }
        }

        let rel = path.to_string_lossy().to_string();
        if !is_archive_path_allowed(&rel) {
            return Err(LockfileError::InvalidArchive(format!(
                "path not in allow-list: {}",
                rel
            )));
        }
        if is_excluded_from_export(&rel) {
            return Err(LockfileError::InvalidArchive(format!(
                "path is in deny-list: {}",
                rel
            )));
        }

        // Directories: nothing to read.
        if matches!(entry_type, tar::EntryType::Directory) {
            continue;
        }

        let size = entry
            .header()
            .size()
            .map_err(|e| LockfileError::InvalidArchive(format!("entry size: {}", e)))?;
        if size > MAX_ARCHIVE_ENTRY_BYTES {
            return Err(LockfileError::InvalidArchive(format!(
                "entry {} too large: {} bytes",
                rel, size
            )));
        }
        total_uncompressed = total_uncompressed.saturating_add(size);
        if total_uncompressed > MAX_ARCHIVE_TOTAL_UNCOMPRESSED {
            return Err(LockfileError::InvalidArchive(format!(
                "uncompressed total too large: {}",
                total_uncompressed
            )));
        }

        let mut buf = Vec::with_capacity(size as usize);
        entry
            .read_to_end(&mut buf)
            .map_err(|e| LockfileError::InvalidArchive(format!("read body: {}", e)))?;

        if first {
            // First entry MUST be the lockfile.
            if rel != LOCKFILE_NAME {
                return Err(LockfileError::InvalidArchive(
                    "first archive entry must be .weplex.lock.yaml".into(),
                ));
            }
            let s = String::from_utf8(buf.clone()).map_err(|e| {
                LockfileError::InvalidArchive(format!("lockfile not UTF-8: {}", e))
            })?;
            let lf: Lockfile = serde_yml::from_str(&s)
                .map_err(|e| LockfileError::InvalidArchive(format!("lockfile: {}", e)))?;
            if lf.version > LOCKFILE_VERSION {
                return Err(LockfileError::InvalidArchive(format!(
                    "lockfile version {} > supported {}",
                    lf.version, LOCKFILE_VERSION
                )));
            }
            incoming_lock = Some(lf);
            first = false;
        } else {
            incoming_files.push((rel, buf));
        }
    }

    let incoming_lock = incoming_lock.ok_or_else(|| {
        LockfileError::InvalidArchive("archive missing lockfile".into())
    })?;

    // Cross-check: every LockfileEntry.files must appear in the archive.
    let archived_paths: std::collections::HashSet<&str> = incoming_files
        .iter()
        .map(|(p, _)| p.as_str())
        .collect();
    for r in &incoming_lock.resources {
        for f in &r.files {
            if !archived_paths.contains(f.as_str()) {
                return Err(LockfileError::InvalidArchive(format!(
                    "lockfile references missing file: {}",
                    f
                )));
            }
        }
    }
    for entries_vec in incoming_lock.history.values() {
        for e in entries_vec {
            for cp in &e.cache_paths {
                if !archived_paths.contains(cp.as_str()) {
                    return Err(LockfileError::InvalidArchive(format!(
                        "history references missing cache path: {}",
                        cp
                    )));
                }
            }
        }
    }

    // Index incoming files for quick lookup.
    let mut by_path: std::collections::HashMap<String, Vec<u8>> = incoming_files
        .into_iter()
        .collect();

    let existing = load_lockfile(target_config_dir);

    let mut report = ImportReport {
        installed: 0,
        skipped: 0,
        overwritten: 0,
    };

    for r in &incoming_lock.resources {
        let existing_entry = existing.resources.iter().find(|e| e.id == r.id);
        let is_conflict = matches!(existing_entry, Some(e) if e.sha256 != r.sha256);

        if is_conflict {
            match policy {
                ConflictPolicy::SkipAll => {
                    report.skipped += 1;
                    continue;
                }
                ConflictPolicy::OverwriteAll => {
                    // fall through to apply
                }
            }
        }

        // Body file path.
        let body_rel = r
            .files
            .iter()
            .find(|f| !f.ends_with(".weplex.yaml"))
            .ok_or_else(|| {
                LockfileError::InvalidArchive(format!(
                    "resource {} has no body file",
                    r.id
                ))
            })?;
        let body_bytes = by_path
            .remove(body_rel)
            .ok_or_else(|| LockfileError::InvalidArchive(format!("missing body: {}", body_rel)))?;
        let body = String::from_utf8(body_bytes).map_err(|e| {
            LockfileError::InvalidArchive(format!("body not UTF-8: {}", e))
        })?;

        let sidecar_rel = r.files.iter().find(|f| f.ends_with(".weplex.yaml"));
        let sidecar = if let Some(sr) = sidecar_rel {
            let bytes = by_path.remove(sr).ok_or_else(|| {
                LockfileError::InvalidArchive(format!("missing sidecar: {}", sr))
            })?;
            Some(String::from_utf8(bytes).map_err(|e| {
                LockfileError::InvalidArchive(format!("sidecar not UTF-8: {}", e))
            })?)
        } else {
            None
        };

        let (kind, name) = parse_resource_id(&r.id)?;

        apply_resource_mutation(
            target_config_dir,
            kind,
            &name,
            r.source,
            MutationKind::Upsert { body, sidecar },
        )?;

        if is_conflict {
            report.overwritten += 1;
        } else {
            report.installed += 1;
        }
    }

    Ok(report)
}

// ─── Tauri commands for export/import ───────────────────────────────────

#[tauri::command]
pub fn export_profile(
    profile_config_dir: String,
    output_path: String,
) -> Result<ExportReport, String> {
    let dir = validate_config_dir(&profile_config_dir).map_err(|e| redact_home(&e))?;
    // Output path must be absolute. We DON'T require it under HOME — the
    // user may want to save to /tmp or an external mount — but we still
    // refuse traversal-y inputs.
    if !output_path.starts_with('/') {
        return Err("output path must be absolute".into());
    }
    export_profile_to_archive(&dir, &output_path).map_err(|e| redact_home(&format!("{}", e)))
}

#[tauri::command]
pub fn inspect_profile_archive_cmd(
    archive_path: String,
    target_config_dir: Option<String>,
) -> Result<ArchiveInspection, String> {
    if !archive_path.starts_with('/') {
        return Err("archive path must be absolute".into());
    }
    match target_config_dir {
        Some(t) => {
            let dir = validate_config_dir(&t).map_err(|e| redact_home(&e))?;
            inspect_profile_archive_against(&archive_path, &dir)
                .map_err(|e| redact_home(&format!("{}", e)))
        }
        None => inspect_profile_archive(&archive_path).map_err(|e| redact_home(&format!("{}", e))),
    }
}

#[tauri::command]
pub fn import_profile(
    target_config_dir: String,
    archive_path: String,
    policy: ConflictPolicy,
) -> Result<ImportReport, String> {
    let dir = validate_config_dir(&target_config_dir).map_err(|e| redact_home(&e))?;
    if !archive_path.starts_with('/') {
        return Err("archive path must be absolute".into());
    }
    import_profile_from_archive(&dir, &archive_path, policy)
        .map_err(|e| redact_home(&format!("{}", e)))
}

// ─── Legacy migration ───────────────────────────────────────────────────

/// Migrate the pre-Phase-3 `~/.weplex/agents/` and `~/.weplex/skills/`
/// directories into a target profile.
///
/// Idempotent: writes a flag file `<target>/.weplex/legacy-weplex-migrated.flag`
/// after a successful run; subsequent invocations short-circuit.
///
/// Source files are NOT deleted. Each migrated resource is recorded in
/// the target profile's lockfile with `source: imported`.
pub fn migrate_legacy_weplex_dir(
    target_config_dir: &str,
) -> Result<MigrationReport, LockfileError> {
    let flag_path = PathBuf::from(target_config_dir).join(LEGACY_FLAG);
    if flag_path.exists() {
        return Ok(MigrationReport {
            already_done: true,
            migrated_agents: 0,
            migrated_skills: 0,
        });
    }

    let home = get_home();
    let mut report = MigrationReport {
        already_done: false,
        migrated_agents: 0,
        migrated_skills: 0,
    };

    // Agents: ~/.weplex/agents/*.md
    let agents_dir = format!("{}/.weplex/agents", home);
    if let Ok(entries) = std::fs::read_dir(&agents_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if !p.is_file() {
                continue;
            }
            if p.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let stem = match p.file_stem().and_then(|s| s.to_str()) {
                Some(s) => s.to_string(),
                None => continue,
            };
            let body = match std::fs::read_to_string(&p) {
                Ok(b) => b,
                Err(e) => {
                    log::warn!("legacy migrate: read {} failed: {}", p.display(), e);
                    continue;
                }
            };
            // Optional sidecar.
            let sidecar_path = p.with_file_name(format!("{}.weplex.yaml", stem));
            let sidecar = if sidecar_path.exists() {
                std::fs::read_to_string(&sidecar_path).ok()
            } else {
                None
            };
            match apply_resource_mutation(
                target_config_dir,
                ResourceKind::Agent,
                &stem,
                ResourceSource::Imported,
                MutationKind::Upsert { body, sidecar },
            ) {
                Ok(r) if !r.no_op => report.migrated_agents += 1,
                Ok(_) => {}
                Err(e) => log::warn!("legacy migrate agent {}: {}", stem, e),
            }
        }
    }

    // Skills: ~/.weplex/skills/<name>/SKILL.md
    let skills_dir = format!("{}/.weplex/skills", home);
    if let Ok(entries) = std::fs::read_dir(&skills_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if !p.is_dir() {
                continue;
            }
            let name = match p.file_name().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            let skill_md = p.join("SKILL.md");
            if !skill_md.exists() {
                continue;
            }
            let body = match std::fs::read_to_string(&skill_md) {
                Ok(b) => b,
                Err(e) => {
                    log::warn!("legacy migrate: read {} failed: {}", skill_md.display(), e);
                    continue;
                }
            };
            let sidecar_path = p.join("SKILL.weplex.yaml");
            let sidecar = if sidecar_path.exists() {
                std::fs::read_to_string(&sidecar_path).ok()
            } else {
                None
            };
            match apply_resource_mutation(
                target_config_dir,
                ResourceKind::Skill,
                &name,
                ResourceSource::Imported,
                MutationKind::Upsert { body, sidecar },
            ) {
                Ok(r) if !r.no_op => report.migrated_skills += 1,
                Ok(_) => {}
                Err(e) => log::warn!("legacy migrate skill {}: {}", name, e),
            }
        }
    }

    // Write the flag file last (idempotence). Even if no resources
    // existed in ~/.weplex/, we still write the flag — there's nothing
    // to migrate, so don't make subsequent runs scan again.
    if let Some(parent) = flag_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            LockfileError::Io(format!("create flag parent {}: {}", parent.display(), e))
        })?;
    }
    std::fs::write(&flag_path, b"1").map_err(|e| {
        LockfileError::Io(format!("write flag {}: {}", flag_path.display(), e))
    })?;

    Ok(report)
}

#[tauri::command]
pub fn migrate_legacy_weplex(
    target_config_dir: String,
) -> Result<MigrationReport, String> {
    let dir = validate_config_dir(&target_config_dir).map_err(|e| redact_home(&e))?;
    migrate_legacy_weplex_dir(&dir).map_err(|e| redact_home(&format!("{}", e)))
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

    // ─── Export / Import ─────────────────────────────────────────────

    fn build_archive(temp: &std::path::Path, files: &[(&str, &[u8])]) -> std::path::PathBuf {
        use flate2::Compression;
        use flate2::write::GzEncoder;
        use std::fs::File;
        let path = temp.join("archive.tar.gz");
        let f = File::create(&path).unwrap();
        let enc = GzEncoder::new(f, Compression::default());
        let mut tb = tar::Builder::new(enc);
        for (rel, body) in files {
            let mut h = tar::Header::new_gnu();
            h.set_size(body.len() as u64);
            h.set_mode(0o644);
            h.set_mtime(0);
            // The tar crate's `set_path` and `append_data` reject `..`
            // and absolute paths defensively. We're TESTING that the
            // import path catches malicious paths, so bypass via the
            // raw path bytes in the GNU header. `set_path` exposes
            // exactly that for old-name-style paths up to 100 bytes.
            h.set_path(*rel).unwrap_or_else(|_| {
                // Fallback for paths the high-level setter rejects:
                // poke the name field directly.
                let raw = h.as_old_mut();
                let bytes = rel.as_bytes();
                let len = bytes.len().min(raw.name.len());
                raw.name[..len].copy_from_slice(&bytes[..len]);
            });
            h.set_cksum();
            use std::io::Write;
            tb.get_mut().write_all(h.as_bytes()).unwrap();
            tb.get_mut().write_all(body).unwrap();
            // Pad to 512-byte tar block boundary.
            let pad = (512 - (body.len() % 512)) % 512;
            if pad > 0 {
                tb.get_mut().write_all(&vec![0u8; pad]).unwrap();
            }
        }
        let enc = tb.into_inner().unwrap();
        enc.finish().unwrap();
        path
    }

    #[test]
    fn export_round_trip_byte_identical() {
        let src = tmpdir("export-src");
        let dst = tmpdir("export-dst");
        apply_resource_mutation(
            src.to_str().unwrap(),
            ResourceKind::Agent,
            "a1",
            ResourceSource::User,
            MutationKind::Upsert {
                body: "hello".into(),
                sidecar: None,
            },
        )
        .unwrap();
        let archive = src.join("out.tar.gz");
        let report = export_profile_to_archive(
            src.to_str().unwrap(),
            archive.to_str().unwrap(),
        )
        .unwrap();
        assert_eq!(report.resource_count, 1);
        assert!(report.bytes > 0);

        let imp = import_profile_from_archive(
            dst.to_str().unwrap(),
            archive.to_str().unwrap(),
            ConflictPolicy::OverwriteAll,
        )
        .unwrap();
        assert_eq!(imp.installed, 1);
        assert_eq!(
            std::fs::read_to_string(dst.join("agents/a1.md")).unwrap(),
            "hello"
        );

        let _ = std::fs::remove_dir_all(&src);
        let _ = std::fs::remove_dir_all(&dst);
    }

    #[test]
    fn export_two_runs_byte_identical() {
        // W2 regression: two exports of the same lockfile state must
        // produce byte-identical archives. The gzip header MTIME must be
        // zero — wall-clock time would break reproducibility.
        let src = tmpdir("export-reproducible");
        apply_resource_mutation(
            src.to_str().unwrap(),
            ResourceKind::Agent,
            "a1",
            ResourceSource::User,
            MutationKind::Upsert {
                body: "hello".into(),
                sidecar: None,
            },
        )
        .unwrap();
        let archive_a = src.join("out-a.tar.gz");
        let archive_b = src.join("out-b.tar.gz");
        export_profile_to_archive(
            src.to_str().unwrap(),
            archive_a.to_str().unwrap(),
        )
        .unwrap();
        // Sleep briefly to guarantee a wall-clock tick — if MTIME were
        // wall-clock based, this would surface a difference.
        std::thread::sleep(std::time::Duration::from_secs(1));
        export_profile_to_archive(
            src.to_str().unwrap(),
            archive_b.to_str().unwrap(),
        )
        .unwrap();
        let bytes_a = std::fs::read(&archive_a).unwrap();
        let bytes_b = std::fs::read(&archive_b).unwrap();
        assert_eq!(
            bytes_a, bytes_b,
            "two exports of the same lockfile state must be byte-identical"
        );
        let _ = std::fs::remove_dir_all(&src);
    }

    #[test]
    fn export_excludes_overrides_json() {
        let src = tmpdir("export-overrides");
        apply_resource_mutation(
            src.to_str().unwrap(),
            ResourceKind::Agent,
            "a1",
            ResourceSource::User,
            MutationKind::Upsert {
                body: "x".into(),
                sidecar: None,
            },
        )
        .unwrap();
        // Plant a fake overrides file.
        let weplex = src.join(".weplex");
        std::fs::create_dir_all(&weplex).unwrap();
        std::fs::write(weplex.join("overrides.json"), "secret").unwrap();
        let archive = src.join("out.tar.gz");
        export_profile_to_archive(src.to_str().unwrap(), archive.to_str().unwrap()).unwrap();

        // Open and walk archive: must not contain .weplex/overrides.json.
        use flate2::read::GzDecoder;
        use std::fs::File;
        let f = File::open(&archive).unwrap();
        let dec = GzDecoder::new(f);
        let mut ar = tar::Archive::new(dec);
        for e in ar.entries().unwrap() {
            let e = e.unwrap();
            let p = e.path().unwrap().to_string_lossy().to_string();
            assert_ne!(p, ".weplex/overrides.json");
        }
        let _ = std::fs::remove_dir_all(&src);
    }

    #[test]
    fn export_excludes_activity_dir() {
        let src = tmpdir("export-activity");
        apply_resource_mutation(
            src.to_str().unwrap(),
            ResourceKind::Agent,
            "a1",
            ResourceSource::User,
            MutationKind::Upsert {
                body: "x".into(),
                sidecar: None,
            },
        )
        .unwrap();
        let activity = src.join(".weplex").join("activity");
        std::fs::create_dir_all(&activity).unwrap();
        std::fs::write(activity.join("session.json"), "encrypted").unwrap();
        let archive = src.join("out.tar.gz");
        export_profile_to_archive(src.to_str().unwrap(), archive.to_str().unwrap()).unwrap();

        use flate2::read::GzDecoder;
        use std::fs::File;
        let f = File::open(&archive).unwrap();
        let dec = GzDecoder::new(f);
        let mut ar = tar::Archive::new(dec);
        for e in ar.entries().unwrap() {
            let e = e.unwrap();
            let p = e.path().unwrap().to_string_lossy().to_string();
            assert!(!p.starts_with(".weplex/activity"), "leaked: {}", p);
        }
        let _ = std::fs::remove_dir_all(&src);
    }

    #[test]
    fn import_rejects_path_traversal_dotdot() {
        let dir = tmpdir("import-traversal");
        let archive = build_archive(
            &dir,
            &[
                (LOCKFILE_NAME, b"version: 1\nresources: []\nhistory: {}\n"),
                ("../escape.md", b"x"),
            ],
        );
        let target = tmpdir("import-traversal-target");
        let r = import_profile_from_archive(
            target.to_str().unwrap(),
            archive.to_str().unwrap(),
            ConflictPolicy::OverwriteAll,
        );
        assert!(matches!(r, Err(LockfileError::InvalidArchive(_))), "got {:?}", r);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&target);
    }

    #[test]
    fn import_rejects_absolute_paths() {
        let dir = tmpdir("import-abs");
        let archive = build_archive(
            &dir,
            &[
                (LOCKFILE_NAME, b"version: 1\nresources: []\nhistory: {}\n"),
                ("/etc/passwd", b"x"),
            ],
        );
        let target = tmpdir("import-abs-target");
        let r = import_profile_from_archive(
            target.to_str().unwrap(),
            archive.to_str().unwrap(),
            ConflictPolicy::OverwriteAll,
        );
        assert!(matches!(r, Err(LockfileError::InvalidArchive(_))), "got {:?}", r);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&target);
    }

    #[test]
    fn import_rejects_symlinks() {
        let dir = tmpdir("import-symlink");
        // Build a manual archive with a symlink entry.
        use flate2::Compression;
        use flate2::write::GzEncoder;
        use std::fs::File;
        let archive_path = dir.join("a.tar.gz");
        let f = File::create(&archive_path).unwrap();
        let enc = GzEncoder::new(f, Compression::default());
        let mut tb = tar::Builder::new(enc);
        // Lockfile first.
        let lf_body = b"version: 1\nresources: []\nhistory: {}\n";
        let mut h = tar::Header::new_gnu();
        h.set_size(lf_body.len() as u64);
        h.set_mode(0o644);
        h.set_mtime(0);
        h.set_cksum();
        tb.append_data(&mut h, LOCKFILE_NAME, &lf_body[..]).unwrap();
        // Symlink entry.
        let mut sh = tar::Header::new_gnu();
        sh.set_size(0);
        sh.set_entry_type(tar::EntryType::Symlink);
        sh.set_mtime(0);
        sh.set_link_name("/etc/passwd").unwrap();
        sh.set_cksum();
        tb.append_data(&mut sh, "agents/evil.md", &[][..]).unwrap();
        let enc = tb.into_inner().unwrap();
        enc.finish().unwrap();

        let target = tmpdir("import-symlink-target");
        let r = import_profile_from_archive(
            target.to_str().unwrap(),
            archive_path.to_str().unwrap(),
            ConflictPolicy::OverwriteAll,
        );
        assert!(matches!(r, Err(LockfileError::InvalidArchive(_))), "got {:?}", r);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&target);
    }

    #[test]
    fn import_rejects_oversized_archive_50mb() {
        let dir = tmpdir("import-oversize");
        let archive_path = dir.join("big.tar.gz");
        // Touch a file that pretends to be 51 MB on disk by setting len.
        // Use a sparse-style trick: write 51 MB of zeros (not great for IO,
        // but still bounded).
        let big = vec![0u8; (MAX_ARCHIVE_SIZE_BYTES + 1) as usize];
        std::fs::write(&archive_path, &big).unwrap();

        let target = tmpdir("import-oversize-target");
        let r = import_profile_from_archive(
            target.to_str().unwrap(),
            archive_path.to_str().unwrap(),
            ConflictPolicy::OverwriteAll,
        );
        assert!(matches!(r, Err(LockfileError::InvalidArchive(_))), "got {:?}", r);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&target);
    }

    #[test]
    fn import_skip_all_leaves_existing_untouched() {
        let src = tmpdir("import-skip-src");
        let dst = tmpdir("import-skip-dst");
        // Source: a1=v1
        apply_resource_mutation(
            src.to_str().unwrap(),
            ResourceKind::Agent,
            "a1",
            ResourceSource::User,
            MutationKind::Upsert {
                body: "from-source".into(),
                sidecar: None,
            },
        )
        .unwrap();
        let archive = src.join("a.tar.gz");
        export_profile_to_archive(src.to_str().unwrap(), archive.to_str().unwrap()).unwrap();
        // Destination already has a1=v2 (different content → conflict).
        apply_resource_mutation(
            dst.to_str().unwrap(),
            ResourceKind::Agent,
            "a1",
            ResourceSource::User,
            MutationKind::Upsert {
                body: "from-dest".into(),
                sidecar: None,
            },
        )
        .unwrap();
        let report = import_profile_from_archive(
            dst.to_str().unwrap(),
            archive.to_str().unwrap(),
            ConflictPolicy::SkipAll,
        )
        .unwrap();
        assert_eq!(report.skipped, 1);
        assert_eq!(report.installed, 0);
        assert_eq!(report.overwritten, 0);
        assert_eq!(
            std::fs::read_to_string(dst.join("agents/a1.md")).unwrap(),
            "from-dest"
        );
        let _ = std::fs::remove_dir_all(&src);
        let _ = std::fs::remove_dir_all(&dst);
    }

    #[test]
    fn import_overwrite_all_pushes_existing_to_history() {
        let src = tmpdir("import-overwrite-src");
        let dst = tmpdir("import-overwrite-dst");
        apply_resource_mutation(
            src.to_str().unwrap(),
            ResourceKind::Agent,
            "a1",
            ResourceSource::User,
            MutationKind::Upsert {
                body: "incoming".into(),
                sidecar: None,
            },
        )
        .unwrap();
        let archive = src.join("a.tar.gz");
        export_profile_to_archive(src.to_str().unwrap(), archive.to_str().unwrap()).unwrap();
        apply_resource_mutation(
            dst.to_str().unwrap(),
            ResourceKind::Agent,
            "a1",
            ResourceSource::User,
            MutationKind::Upsert {
                body: "existing".into(),
                sidecar: None,
            },
        )
        .unwrap();
        let report = import_profile_from_archive(
            dst.to_str().unwrap(),
            archive.to_str().unwrap(),
            ConflictPolicy::OverwriteAll,
        )
        .unwrap();
        assert_eq!(report.overwritten, 1);
        let lf = load_lockfile(dst.to_str().unwrap());
        assert!(lf.history.contains_key("agents/a1"));
        assert_eq!(
            std::fs::read_to_string(dst.join("agents/a1.md")).unwrap(),
            "incoming"
        );
        let _ = std::fs::remove_dir_all(&src);
        let _ = std::fs::remove_dir_all(&dst);
    }

    #[test]
    fn inspect_archive_returns_conflicts_without_writing() {
        let src = tmpdir("inspect-src");
        let dst = tmpdir("inspect-dst");
        apply_resource_mutation(
            src.to_str().unwrap(),
            ResourceKind::Agent,
            "a1",
            ResourceSource::User,
            MutationKind::Upsert {
                body: "incoming".into(),
                sidecar: None,
            },
        )
        .unwrap();
        let archive = src.join("a.tar.gz");
        export_profile_to_archive(src.to_str().unwrap(), archive.to_str().unwrap()).unwrap();
        apply_resource_mutation(
            dst.to_str().unwrap(),
            ResourceKind::Agent,
            "a1",
            ResourceSource::User,
            MutationKind::Upsert {
                body: "existing".into(),
                sidecar: None,
            },
        )
        .unwrap();
        let inspection = inspect_profile_archive_against(
            archive.to_str().unwrap(),
            dst.to_str().unwrap(),
        )
        .unwrap();
        assert_eq!(inspection.resource_count, 1);
        assert_eq!(inspection.conflicts.len(), 1);
        assert_eq!(inspection.conflicts[0].resource_id, "agents/a1");
        // Inspection must not write — destination still has "existing".
        assert_eq!(
            std::fs::read_to_string(dst.join("agents/a1.md")).unwrap(),
            "existing"
        );
        let _ = std::fs::remove_dir_all(&src);
        let _ = std::fs::remove_dir_all(&dst);
    }

    // ─── Migration ───────────────────────────────────────────────────

    #[test]
    fn migrate_legacy_weplex_idempotent_via_flag_file() {
        use crate::utils::test_support::HOME_ENV_LOCK as ENV_LOCK;
        let _g = ENV_LOCK.lock().unwrap();
        let home = tmpdir("legacy-flag");
        let canon = std::fs::canonicalize(&home).unwrap();
        let prev = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", &canon); }

        // Plant a legacy agent.
        let legacy_agents = canon.join(".weplex/agents");
        std::fs::create_dir_all(&legacy_agents).unwrap();
        std::fs::write(legacy_agents.join("legacy.md"), "from old").unwrap();

        let target = canon.join(".claude");
        std::fs::create_dir_all(&target).unwrap();

        let r1 = migrate_legacy_weplex_dir(target.to_str().unwrap()).unwrap();
        assert!(!r1.already_done);
        assert_eq!(r1.migrated_agents, 1);

        // Second call: flag file blocks re-scan.
        let r2 = migrate_legacy_weplex_dir(target.to_str().unwrap()).unwrap();
        assert!(r2.already_done);
        assert_eq!(r2.migrated_agents, 0);

        // Even after deleting the migrated file, the flag prevents rerun.
        std::fs::remove_file(target.join("agents/legacy.md")).unwrap();
        let r3 = migrate_legacy_weplex_dir(target.to_str().unwrap()).unwrap();
        assert!(r3.already_done);

        if let Some(p) = prev {
            unsafe { std::env::set_var("HOME", p); }
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn migrate_legacy_weplex_marks_source_imported() {
        use crate::utils::test_support::HOME_ENV_LOCK as ENV_LOCK;
        let _g = ENV_LOCK.lock().unwrap();
        let home = tmpdir("legacy-source");
        let canon = std::fs::canonicalize(&home).unwrap();
        let prev = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", &canon); }

        let legacy_agents = canon.join(".weplex/agents");
        std::fs::create_dir_all(&legacy_agents).unwrap();
        std::fs::write(legacy_agents.join("a1.md"), "agent body").unwrap();

        let legacy_skills = canon.join(".weplex/skills/s1");
        std::fs::create_dir_all(&legacy_skills).unwrap();
        std::fs::write(legacy_skills.join("SKILL.md"), "skill body").unwrap();

        let target = canon.join(".claude");
        std::fs::create_dir_all(&target).unwrap();
        let r = migrate_legacy_weplex_dir(target.to_str().unwrap()).unwrap();
        assert_eq!(r.migrated_agents, 1);
        assert_eq!(r.migrated_skills, 1);

        let lf = load_lockfile(target.to_str().unwrap());
        assert_eq!(lf.resources.len(), 2);
        for entry in &lf.resources {
            assert_eq!(entry.source, ResourceSource::Imported);
        }
        assert!(target.join("agents/a1.md").exists());
        assert!(target.join("skills/s1/SKILL.md").exists());
        // Source unchanged.
        assert!(legacy_agents.join("a1.md").exists());

        if let Some(p) = prev {
            unsafe { std::env::set_var("HOME", p); }
        }
        let _ = std::fs::remove_dir_all(&home);
    }
}

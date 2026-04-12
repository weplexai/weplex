//! Resource management: discovery, distribution, and manifest tracking
//! for agents, rules, skills, and commands across Claude profiles.
//!
//! Architecture: copy-based distribution from ~/.weplex/ (source of truth)
//! to profile config dirs. Manifest tracks what was distributed where.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ─── Types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResourceType {
    Agent,
    Rule,
    Skill,
}

impl ResourceType {
    /// Subdirectory name within a config dir or ~/.weplex/.
    pub fn dir_name(&self) -> &'static str {
        match self {
            ResourceType::Agent => "agents",
            ResourceType::Rule => "rules",
            ResourceType::Skill => "skills",
        }
    }

    pub fn all() -> &'static [ResourceType] {
        &[ResourceType::Agent, ResourceType::Rule, ResourceType::Skill]
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResourceOrigin {
    /// Lives only in a specific profile, not managed by Weplex.
    ProfileLocal,
    /// Managed by Weplex, distributed to profiles as copies.
    WeplexManaged,
    /// Installed from marketplace, managed by Weplex.
    Marketplace,
}

/// A discovered resource (agent, rule, or skill) from any source.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resource {
    pub name: String,
    pub resource_type: ResourceType,
    pub origin: ResourceOrigin,
    /// Profile name (for ProfileLocal resources).
    pub profile_name: Option<String>,
    /// Profile config dir (for ProfileLocal resources).
    pub profile_config_dir: Option<String>,
    /// Absolute file path.
    pub file_path: String,
    /// FNV-1a hash of file content (16 hex chars). For change detection, not cryptographic.
    pub content_hash: String,
    /// Description extracted from frontmatter (agents only).
    pub description: String,
    /// Marketplace package ID (for Marketplace resources).
    pub marketplace_id: Option<String>,
    /// Marketplace version (for Marketplace resources).
    pub marketplace_version: Option<String>,
    /// True if profile copy differs from Weplex source.
    pub is_outdated: bool,
}

/// A name conflict: same resource name in multiple profiles with different content.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Conflict {
    pub name: String,
    pub resource_type: ResourceType,
    pub versions: Vec<ConflictVersion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictVersion {
    pub profile_name: String,
    pub profile_config_dir: String,
    pub content_hash: String,
}

/// A resource whose profile copy has been locally modified.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriftEntry {
    pub name: String,
    pub resource_type: ResourceType,
    pub profile_name: String,
    pub profile_config_dir: String,
    pub expected_hash: String,
    pub actual_hash: String,
}

/// Profile info passed from frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileInfo {
    pub id: String,
    pub name: String,
    pub config_dir: Option<String>,
}

// ─── Manifest ───────────────────────────────────────────────────────────

const MANIFEST_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    #[serde(default = "default_manifest_version")]
    pub version: u32,
    pub resources: Vec<ManifestEntry>,
}

fn default_manifest_version() -> u32 {
    MANIFEST_VERSION
}

impl Default for Manifest {
    fn default() -> Self {
        Self {
            version: MANIFEST_VERSION,
            resources: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestEntry {
    pub name: String,
    pub resource_type: ResourceType,
    pub source_path: String,
    pub content_hash: String,
    pub origin: ResourceOrigin,
    pub marketplace_id: Option<String>,
    pub marketplace_version: Option<String>,
    pub distributed_to: Vec<DistributionTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DistributionTarget {
    pub profile_config_dir: String,
    pub target_path: String,
    pub synced_hash: String,
    pub synced_at: String,
}

fn manifest_path() -> Result<String, String> {
    let home = std::env::var("HOME")
        .map_err(|_| "HOME environment variable not set".to_string())?;
    Ok(format!("{}/.weplex/manifest.json", home))
}

pub fn load_manifest() -> Manifest {
    let path = match manifest_path() {
        Ok(p) => p,
        Err(_) => return Manifest::default(),
    };
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_default()
}

pub fn save_manifest(manifest: &Manifest) -> Result<(), String> {
    let path = manifest_path()?;
    let json = serde_json::to_string_pretty(manifest).map_err(|e| e.to_string())?;
    // Atomic write
    let tmp = format!("{}.tmp", path);
    let _ = std::fs::remove_file(&tmp);
    std::fs::write(&tmp, &json).map_err(|e| format!("Failed to write manifest: {}", e))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("Failed to rename manifest: {}", e))?;
    Ok(())
}

// ─── Helpers ────────────────────────────────────────────────────────────

/// Content hash for comparison — FNV-1a, no external crate needed.
/// Not cryptographic, but fast and sufficient for detecting changes.
pub fn compute_hash(content: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in content.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", h)
}

fn now_iso() -> String {
    // ISO 8601 timestamp without chrono crate
    let duration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Days since 1970-01-01
    let mut y = 1970i64;
    let mut remaining = days as i64;
    loop {
        let days_in_year = if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) { 366 } else { 365 };
        if remaining < days_in_year { break; }
        remaining -= days_in_year;
        y += 1;
    }
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let month_days = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut m = 0usize;
    for md in &month_days {
        if remaining < *md as i64 { break; }
        remaining -= *md as i64;
        m += 1;
    }
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, m + 1, remaining + 1, hours, minutes, seconds)
}

/// Extract description from .md file frontmatter (first `description:` line).
fn extract_description(content: &str) -> String {
    let content = content.trim_start_matches('\n');
    if !content.starts_with("---") {
        return String::new();
    }
    let rest = &content[3..];
    let end = match rest.find("\n---") {
        Some(e) => e,
        None => return String::new(),
    };
    let frontmatter = &rest[..end];
    for line in frontmatter.lines() {
        if let Some((key, value)) = line.split_once(':') {
            if key.trim() == "description" {
                let v = value.trim();
                // Strip YAML quotes
                if (v.starts_with('"') && v.ends_with('"'))
                    || (v.starts_with('\'') && v.ends_with('\''))
                {
                    return v[1..v.len() - 1].to_string();
                }
                return v.to_string();
            }
        }
    }
    String::new()
}

/// Read .md resource files from a directory.
fn read_resources_from_dir(
    dir_path: &str,
    resource_type: ResourceType,
    origin: ResourceOrigin,
    profile_name: Option<&str>,
    profile_config_dir: Option<&str>,
    manifest: &Manifest,
) -> Vec<Resource> {
    let dir = match std::fs::read_dir(dir_path) {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };

    let mut resources = Vec::new();
    for entry in dir.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let file_path = path.to_string_lossy().to_string();
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        if name.is_empty() {
            continue;
        }

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let hash = compute_hash(&content);
        let description = extract_description(&content);

        // Check if this is a Weplex-managed copy (exists in manifest)
        let (actual_origin, marketplace_id, marketplace_version, is_outdated) =
            if origin == ResourceOrigin::ProfileLocal {
                // Check if manifest tracks this as a distributed copy
                if let Some(entry) = manifest.resources.iter().find(|e| {
                    e.name == name && e.resource_type == resource_type
                }) {
                    let is_dist = entry.distributed_to.iter().any(|d| {
                        d.target_path == file_path
                            || (profile_config_dir.is_some()
                                && d.profile_config_dir
                                    == profile_config_dir.unwrap_or(""))
                    });
                    if is_dist {
                        let outdated = entry.content_hash != hash;
                        (
                            entry.origin.clone(),
                            entry.marketplace_id.clone(),
                            entry.marketplace_version.clone(),
                            outdated,
                        )
                    } else {
                        (ResourceOrigin::ProfileLocal, None, None, false)
                    }
                } else {
                    (ResourceOrigin::ProfileLocal, None, None, false)
                }
            } else {
                // Weplex source dir — check manifest for marketplace info
                let me = manifest.resources.iter().find(|e| {
                    e.name == name && e.resource_type == resource_type
                });
                (
                    origin.clone(),
                    me.and_then(|e| e.marketplace_id.clone()),
                    me.and_then(|e| e.marketplace_version.clone()),
                    false,
                )
            };

        resources.push(Resource {
            name,
            resource_type,
            origin: actual_origin,
            profile_name: profile_name.map(|s| s.to_string()),
            profile_config_dir: profile_config_dir.map(|s| s.to_string()),
            file_path,
            content_hash: hash,
            description,
            marketplace_id,
            marketplace_version,
            is_outdated,
        });
    }
    resources
}

// ─── Discovery ──────────────────────────────────────────────────────────

/// Discover all resources from all profiles + ~/.weplex/.
/// Returns a unified list with proper origin/state for each resource.
pub fn discover_all_resources(profiles: &[ProfileInfo]) -> Result<Vec<Resource>, String> {
    let home = std::env::var("HOME")
        .map_err(|_| "HOME environment variable not set".to_string())?;
    let manifest = load_manifest();
    let mut all_resources = Vec::new();

    // 1. Scan Weplex shared resources
    for rt in ResourceType::all() {
        let weplex_dir = format!("{}/.weplex/{}", home, rt.dir_name());
        let resources = read_resources_from_dir(
            &weplex_dir,
            *rt,
            ResourceOrigin::WeplexManaged,
            None,
            None,
            &manifest,
        );
        all_resources.extend(resources);
    }

    // 2. Scan each profile
    let default_config_dir = format!("{}/.claude", home);
    for profile in profiles {
        let config_dir = profile
            .config_dir
            .as_deref()
            .unwrap_or(&default_config_dir);

        for rt in ResourceType::all() {
            let profile_dir = format!("{}/{}", config_dir, rt.dir_name());
            let resources = read_resources_from_dir(
                &profile_dir,
                *rt,
                ResourceOrigin::ProfileLocal,
                Some(&profile.name),
                Some(config_dir),
                &manifest,
            );

            for r in resources {
                if r.origin == ResourceOrigin::ProfileLocal {
                    // Profile-local: add to list
                    all_resources.push(r);
                } else if r.is_outdated {
                    // Weplex-managed copy with local modifications:
                    // mark the source resource as outdated
                    if let Some(source) = all_resources.iter_mut().find(|s| {
                        s.name == r.name && s.resource_type == r.resource_type
                            && (s.origin == ResourceOrigin::WeplexManaged
                                || s.origin == ResourceOrigin::Marketplace)
                    }) {
                        source.is_outdated = true;
                    }
                }
                // Non-outdated Weplex copies are skipped (source already in list)
            }
        }
    }

    // Sort by name
    all_resources.sort_by(|a, b| {
        a.resource_type
            .dir_name()
            .cmp(b.resource_type.dir_name())
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Ok(all_resources)
}

/// Detect name conflicts: same name + type in multiple profiles with different content.
pub fn detect_conflicts(resources: &[Resource]) -> Vec<Conflict> {
    let mut by_key: HashMap<(String, String), Vec<&Resource>> = HashMap::new();

    for r in resources {
        if r.origin != ResourceOrigin::ProfileLocal {
            continue;
        }
        let key = (r.name.clone(), r.resource_type.dir_name().to_string());
        by_key.entry(key).or_default().push(r);
    }

    let mut conflicts = Vec::new();
    for ((name, _), versions) in &by_key {
        if versions.len() < 2 {
            continue;
        }
        // Check if content actually differs
        let hashes: std::collections::HashSet<&str> =
            versions.iter().map(|r| r.content_hash.as_str()).collect();
        if hashes.len() < 2 {
            continue; // Same content, not a real conflict
        }

        conflicts.push(Conflict {
            name: name.clone(),
            resource_type: versions[0].resource_type,
            versions: versions
                .iter()
                .map(|r| ConflictVersion {
                    profile_name: r.profile_name.clone().unwrap_or_default(),
                    profile_config_dir: r.profile_config_dir.clone().unwrap_or_default(),
                    content_hash: r.content_hash.clone(),
                })
                .collect(),
        });
    }
    conflicts
}

// ─── Distribution ───────────────────────────────────────────────────────

/// Share a profile-local resource: copy to ~/.weplex/ and distribute to all profiles.
pub fn share_resource(
    source_path: &str,
    name: &str,
    resource_type: ResourceType,
    profile_config_dirs: &[String],
) -> Result<(), String> {
    let home = std::env::var("HOME")
        .map_err(|_| "HOME environment variable not set".to_string())?;

    // Read source content
    let content = std::fs::read_to_string(source_path)
        .map_err(|e| format!("Failed to read source: {}", e))?;
    let hash = compute_hash(&content);

    // Copy to ~/.weplex/{type}/
    let weplex_dir = format!("{}/.weplex/{}", home, resource_type.dir_name());
    std::fs::create_dir_all(&weplex_dir)
        .map_err(|e| format!("Failed to create dir: {}", e))?;
    let weplex_path = format!("{}/{}.md", weplex_dir, name);
    std::fs::write(&weplex_path, &content)
        .map_err(|e| format!("Failed to write to weplex: {}", e))?;

    // Distribute copies to all profiles
    let mut distributed_to = Vec::new();
    for config_dir in profile_config_dirs {
        let target = distribute_single(name, resource_type, &content, config_dir)?;
        distributed_to.push(target);
    }

    // Update manifest
    let mut manifest = load_manifest();
    // Remove existing entry if any
    manifest.resources.retain(|e| {
        !(e.name == name && e.resource_type == resource_type)
    });
    manifest.resources.push(ManifestEntry {
        name: name.to_string(),
        resource_type,
        source_path: weplex_path,
        content_hash: hash,
        origin: ResourceOrigin::WeplexManaged,
        marketplace_id: None,
        marketplace_version: None,
        distributed_to,
    });
    save_manifest(&manifest)?;

    eprintln!("[weplex] shared resource {}/{}", resource_type.dir_name(), name);
    Ok(())
}

/// Distribute a single resource to a single profile. Returns the distribution target.
fn distribute_single(
    name: &str,
    resource_type: ResourceType,
    content: &str,
    config_dir: &str,
) -> Result<DistributionTarget, String> {
    let target_dir = format!("{}/{}", config_dir, resource_type.dir_name());
    std::fs::create_dir_all(&target_dir)
        .map_err(|e| format!("Failed to create dir {}: {}", target_dir, e))?;

    let target_path = format!("{}/{}.md", target_dir, name);

    // Atomic write
    let tmp = format!("{}.tmp", target_path);
    let _ = std::fs::remove_file(&tmp);
    std::fs::write(&tmp, content)
        .map_err(|e| format!("Failed to write {}: {}", target_path, e))?;
    std::fs::rename(&tmp, &target_path)
        .map_err(|e| format!("Failed to rename {}: {}", target_path, e))?;

    Ok(DistributionTarget {
        profile_config_dir: config_dir.to_string(),
        target_path,
        synced_hash: compute_hash(content),
        synced_at: now_iso(),
    })
}

/// Distribute all Weplex-managed resources to a single profile.
/// Used when creating a new profile.
pub fn distribute_all_to_profile(config_dir: &str) -> Result<(), String> {
    let home = std::env::var("HOME")
        .map_err(|_| "HOME environment variable not set".to_string())?;
    let mut manifest = load_manifest();

    for entry in &mut manifest.resources {
        // Read source content
        let content = match std::fs::read_to_string(&entry.source_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!(
                    "[weplex] skipping {}/{}: {}",
                    entry.resource_type.dir_name(),
                    entry.name,
                    e
                );
                continue;
            }
        };

        // Skip if already distributed to this profile
        if entry
            .distributed_to
            .iter()
            .any(|d| d.profile_config_dir == config_dir)
        {
            continue;
        }

        match distribute_single(&entry.name, entry.resource_type, &content, config_dir) {
            Ok(target) => entry.distributed_to.push(target),
            Err(e) => eprintln!(
                "[weplex] failed to distribute {}/{} to {}: {}",
                entry.resource_type.dir_name(),
                entry.name,
                config_dir,
                e
            ),
        }
    }

    save_manifest(&manifest)?;
    eprintln!("[weplex] distributed all resources to {}", config_dir);
    Ok(())
}

// ─── CRUD ───────────────────────────────────────────────────────────────

/// Create a new Weplex-managed resource and distribute to all profiles.
pub fn create_resource(
    name: &str,
    resource_type: ResourceType,
    content: &str,
    profile_config_dirs: &[String],
) -> Result<(), String> {
    let home = std::env::var("HOME")
        .map_err(|_| "HOME environment variable not set".to_string())?;

    // Validate name
    let safe_name = sanitize_resource_name(name)?;
    let hash = compute_hash(content);

    // Write to ~/.weplex/
    let weplex_dir = format!("{}/.weplex/{}", home, resource_type.dir_name());
    std::fs::create_dir_all(&weplex_dir)
        .map_err(|e| format!("Failed to create dir: {}", e))?;
    let weplex_path = format!("{}/{}.md", weplex_dir, safe_name);
    std::fs::write(&weplex_path, content)
        .map_err(|e| format!("Failed to write resource: {}", e))?;

    // Distribute to all profiles
    let mut distributed_to = Vec::new();
    for config_dir in profile_config_dirs {
        match distribute_single(&safe_name, resource_type, content, config_dir) {
            Ok(target) => distributed_to.push(target),
            Err(e) => eprintln!("[weplex] failed to distribute to {}: {}", config_dir, e),
        }
    }

    // Update manifest
    let mut manifest = load_manifest();
    manifest.resources.retain(|e| {
        !(e.name == safe_name && e.resource_type == resource_type)
    });
    manifest.resources.push(ManifestEntry {
        name: safe_name.clone(),
        resource_type,
        source_path: weplex_path,
        content_hash: hash,
        origin: ResourceOrigin::WeplexManaged,
        marketplace_id: None,
        marketplace_version: None,
        distributed_to,
    });
    save_manifest(&manifest)?;

    eprintln!("[weplex] created resource {}/{}", resource_type.dir_name(), safe_name);
    Ok(())
}

/// Validate that a path from manifest is within $HOME (defense against manifest poisoning).
fn validate_manifest_path(path: &str) -> bool {
    let home = match std::env::var("HOME") {
        Ok(h) => h,
        Err(_) => return false,
    };
    // Resolve symlinks if path exists
    let canonical = std::fs::canonicalize(path)
        .unwrap_or_else(|_| std::path::PathBuf::from(path));
    canonical.starts_with(&home)
}

/// Update a Weplex-managed resource and re-distribute to all profiles.
pub fn update_resource(
    name: &str,
    resource_type: ResourceType,
    content: &str,
) -> Result<(), String> {
    let mut manifest = load_manifest();
    let entry = manifest
        .resources
        .iter_mut()
        .find(|e| e.name == name && e.resource_type == resource_type)
        .ok_or_else(|| format!("Resource not found: {}/{}", resource_type.dir_name(), name))?;

    // Validate source path from manifest
    if !validate_manifest_path(&entry.source_path) {
        return Err(format!("Manifest source path outside HOME: {}", entry.source_path));
    }

    let hash = compute_hash(content);

    // Update source file
    std::fs::write(&entry.source_path, content)
        .map_err(|e| format!("Failed to write source: {}", e))?;
    entry.content_hash = hash;

    // Re-distribute to all existing targets (validate paths from manifest)
    for target in &mut entry.distributed_to {
        if !validate_manifest_path(&target.target_path) {
            eprintln!("[weplex] skipping invalid manifest path: {}", target.target_path);
            continue;
        }
        let tmp = format!("{}.tmp", target.target_path);
        let _ = std::fs::remove_file(&tmp);
        if let Err(e) = std::fs::write(&tmp, content) {
            eprintln!("[weplex] failed to update {}: {}", target.target_path, e);
            continue;
        }
        if let Err(e) = std::fs::rename(&tmp, &target.target_path) {
            eprintln!("[weplex] failed to rename {}: {}", target.target_path, e);
            continue;
        }
        target.synced_hash = compute_hash(content);
        target.synced_at = now_iso();
    }

    save_manifest(&manifest)?;
    eprintln!("[weplex] updated resource {}/{}", resource_type.dir_name(), name);
    Ok(())
}

/// Delete a Weplex-managed resource and remove all distributed copies.
pub fn delete_resource(name: &str, resource_type: ResourceType) -> Result<(), String> {
    let mut manifest = load_manifest();

    let entry = match manifest
        .resources
        .iter()
        .find(|e| e.name == name && e.resource_type == resource_type)
    {
        Some(e) => e.clone(),
        None => {
            return Err(format!(
                "Resource not found in manifest: {}/{}",
                resource_type.dir_name(),
                name
            ))
        }
    };

    // Remove source file (validate path from manifest)
    if validate_manifest_path(&entry.source_path) {
        let _ = std::fs::remove_file(&entry.source_path);
    } else {
        eprintln!("[weplex] skipping invalid manifest source: {}", entry.source_path);
    }

    // Remove distributed copies (validate paths from manifest)
    for target in &entry.distributed_to {
        if validate_manifest_path(&target.target_path) {
            let _ = std::fs::remove_file(&target.target_path);
        } else {
            eprintln!("[weplex] skipping invalid manifest target: {}", target.target_path);
        }
    }

    // Remove from manifest
    manifest.resources.retain(|e| {
        !(e.name == name && e.resource_type == resource_type)
    });
    save_manifest(&manifest)?;

    eprintln!("[weplex] deleted resource {}/{}", resource_type.dir_name(), name);
    Ok(())
}

// ─── Drift Detection ────────────────────────────────────────────────────

/// Check for local modifications in profile copies (hash mismatch with source).
pub fn check_drift(profile_config_dirs: &[String]) -> Vec<DriftEntry> {
    let manifest = load_manifest();
    let mut drifts = Vec::new();

    for entry in &manifest.resources {
        for target in &entry.distributed_to {
            // Only check profiles we know about
            if !profile_config_dirs.contains(&target.profile_config_dir) {
                continue;
            }

            let actual_content = match std::fs::read_to_string(&target.target_path) {
                Ok(c) => c,
                Err(_) => continue, // File missing — will be re-created on sync
            };

            let actual_hash = compute_hash(&actual_content);
            if actual_hash != entry.content_hash {
                drifts.push(DriftEntry {
                    name: entry.name.clone(),
                    resource_type: entry.resource_type,
                    profile_name: String::new(), // Filled by caller
                    profile_config_dir: target.profile_config_dir.clone(),
                    expected_hash: entry.content_hash.clone(),
                    actual_hash,
                });
            }
        }
    }

    drifts
}

// ─── Helpers ────────────────────────────────────────────────────────────

/// Public wrapper for Tauri command validation.
pub fn sanitize_resource_name_public(name: &str) -> Result<String, String> {
    sanitize_resource_name(name)
}

/// Sanitize resource name for filesystem use.
fn sanitize_resource_name(name: &str) -> Result<String, String> {
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
    if sanitized.contains("..") || sanitized.contains('/') || sanitized.contains('\\') {
        return Err("Invalid name".to_string());
    }
    Ok(sanitized)
}

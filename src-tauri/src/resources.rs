//! Resource management: discovery and copy operations for agents, rules,
//! and skills across Claude Code profiles.
//!
//! Architecture: Profile-first. Profile directories are source of truth.
//! Weplex is a read-only viewer + copy tool. No master copy, no manifest,
//! no sync, no drift detection.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ─── Types ──────────────────────────────────────────────────────────────

/// Adding a new resource type? Update ALL of these:
/// 1. Add variant here
/// 2. Add arm in `dir_name()` below
/// 3. Add element in `all()` below
/// 4. Add field in `ResourceCounts` struct
/// 5. Add arm in `count_resources()` match
/// 6. Update frontend types.ts `ResourceType`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResourceType {
    Agent,
    Rule,
    Skill,
}

impl ResourceType {
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

/// A discovered resource from a profile directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resource {
    pub name: String,
    pub resource_type: ResourceType,
    pub profile_id: String,
    pub profile_name: String,
    pub file_path: String,
    /// FNV-1a hash for deduplication/comparison.
    pub content_hash: String,
    pub description: String,
}

/// A unified resource entry: one name+type across profiles.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnifiedResource {
    pub name: String,
    pub resource_type: ResourceType,
    pub description: String,
    pub profiles: Vec<ResourceProfile>,
    /// True if the resource exists in 2+ profiles with different content.
    pub differs: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceProfile {
    pub profile_id: String,
    pub profile_name: String,
    pub file_path: String,
    pub content_hash: String,
}

/// Profile info passed from frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileInfo {
    pub id: String,
    pub name: String,
    pub config_dir: Option<String>,
}

/// Counts of resources by type (for import dialog).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceCounts {
    pub agents: u32,
    pub rules: u32,
    pub skills: u32,
}

// ─── Discovery ──────────────────────────────────────────────────────────

/// Discover all resources from all profiles and return a unified view.
pub fn discover(profiles: &[ProfileInfo]) -> Result<Vec<UnifiedResource>, String> {
    let home = crate::utils::get_home();
    let default_dir = format!("{}/.claude", home);

    // 1. Scan all profiles, collect flat list
    let mut all: Vec<Resource> = Vec::new();
    for profile in profiles {
        let config_dir = profile
            .config_dir
            .as_deref()
            .unwrap_or(&default_dir);

        for rt in ResourceType::all() {
            let dir = format!("{}/{}", config_dir, rt.dir_name());
            let resources = scan_dir(&dir, *rt, &profile.id, &profile.name);
            all.extend(resources);
        }
    }

    // 2. Group by (name, type) → unified entries
    let mut groups: HashMap<(String, ResourceType), Vec<Resource>> = HashMap::new();
    for r in all {
        let key = (r.name.clone(), r.resource_type);
        groups.entry(key).or_default().push(r);
    }

    // 3. Build unified list
    let mut unified: Vec<UnifiedResource> = groups
        .into_iter()
        .map(|((name, resource_type), entries)| {
            let description = entries
                .first()
                .map(|r| r.description.clone())
                .unwrap_or_default();

            let hashes: std::collections::HashSet<&str> =
                entries.iter().map(|r| r.content_hash.as_str()).collect();
            let differs = hashes.len() > 1;

            let profiles = entries
                .into_iter()
                .map(|r| ResourceProfile {
                    profile_id: r.profile_id,
                    profile_name: r.profile_name,
                    file_path: r.file_path,
                    content_hash: r.content_hash,
                })
                .collect();

            UnifiedResource {
                name,
                resource_type,
                description,
                profiles,
                differs,
            }
        })
        .collect();

    // Sort: by type, then alphabetically
    unified.sort_by(|a, b| {
        a.resource_type
            .dir_name()
            .cmp(b.resource_type.dir_name())
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Ok(unified)
}

/// Count resources by type for a set of profiles (for import dialog).
pub fn count_resources(profiles: &[ProfileInfo]) -> Result<ResourceCounts, String> {
    let home = crate::utils::get_home();
    let default_dir = format!("{}/.claude", home);

    let mut counts = ResourceCounts {
        agents: 0,
        rules: 0,
        skills: 0,
    };

    for profile in profiles {
        let config_dir = profile
            .config_dir
            .as_deref()
            .unwrap_or(&default_dir);

        for rt in ResourceType::all() {
            let dir = format!("{}/{}", config_dir, rt.dir_name());
            let count = count_md_files(&dir);
            match rt {
                ResourceType::Agent => counts.agents += count,
                ResourceType::Rule => counts.rules += count,
                ResourceType::Skill => counts.skills += count,
            }
        }
    }

    Ok(counts)
}

// ─── Copy ───────────────────────────────────────────────────────────────

/// Copy a resource file from one profile to another.
/// Returns Ok(true) if copied, Ok(false) if skipped (identical).
pub fn copy_resource(
    source_path: &str,
    target_config_dir: &str,
    resource_type: ResourceType,
    name: &str,
    overwrite: bool,
) -> Result<bool, String> {
    // Sanitize name to prevent path traversal (e.g. "../../.ssh/key")
    let safe_name = sanitize_name(name)?;

    let content = std::fs::read_to_string(source_path)
        .map_err(|e| format!("Failed to read source: {}", e))?;

    let target_dir = format!("{}/{}", target_config_dir, resource_type.dir_name());
    std::fs::create_dir_all(&target_dir)
        .map_err(|e| format!("Failed to create dir: {}", e))?;

    let target_path = format!("{}/{}.md", target_dir, safe_name);

    // Check if target exists
    if std::path::Path::new(&target_path).exists() {
        let existing = std::fs::read_to_string(&target_path)
            .map_err(|e| format!("Failed to read target: {}", e))?;

        if compute_hash(&content) == compute_hash(&existing) {
            return Ok(false); // Identical, skip
        }

        if !overwrite {
            return Err("Target exists with different content".to_string());
        }
    }

    // Atomic write
    atomic_write(&target_path, &content)?;

    log::info!("copied {}/{} to {}", resource_type.dir_name(), name, target_config_dir);
    Ok(true)
}

/// Copy all resources from source profiles to a target profile.
/// Skips files that already exist in target.
pub fn copy_all_to_profile(
    source_profiles: &[ProfileInfo],
    target_config_dir: &str,
) -> Result<u32, String> {
    let home = crate::utils::get_home();
    let default_dir = format!("{}/.claude", home);
    let mut copied = 0u32;

    for profile in source_profiles {
        let config_dir = profile
            .config_dir
            .as_deref()
            .unwrap_or(&default_dir);

        // Don't copy from target to itself
        if config_dir == target_config_dir {
            continue;
        }

        for rt in ResourceType::all() {
            let dir = format!("{}/{}", config_dir, rt.dir_name());
            let resources = scan_dir(&dir, *rt, &profile.id, &profile.name);

            for r in resources {
                let target_path = format!(
                    "{}/{}/{}.md",
                    target_config_dir,
                    rt.dir_name(),
                    r.name
                );

                // Skip if already exists in target
                if std::path::Path::new(&target_path).exists() {
                    continue;
                }

                match copy_resource(&r.file_path, target_config_dir, *rt, &r.name, false) {
                    Ok(true) => copied += 1,
                    Ok(false) => {} // identical, skipped
                    Err(e) => log::warn!("copy failed: {}", e),
                }
            }
        }
    }

    Ok(copied)
}

// ─── Create / Delete ────────────────────────────────────────────────────

/// Create a new resource file in a specific profile.
///
/// Non-lockfile-aware primitive kept for tests and rare callers that need
/// a direct write; production paths go through the lockfile.
#[allow(dead_code)]
pub fn create_resource(
    config_dir: &str,
    resource_type: ResourceType,
    name: &str,
    content: &str,
) -> Result<String, String> {
    let safe_name = sanitize_name(name)?;
    let target_dir = format!("{}/{}", config_dir, resource_type.dir_name());
    std::fs::create_dir_all(&target_dir)
        .map_err(|e| format!("Failed to create dir: {}", e))?;

    let path = format!("{}/{}.md", target_dir, safe_name);

    if std::path::Path::new(&path).exists() {
        return Err(format!("Resource already exists: {}", safe_name));
    }

    atomic_write(&path, content)?;

    log::info!("created {}/{} in {}", resource_type.dir_name(), safe_name, config_dir);
    Ok(path)
}

/// Delete a resource file from a specific profile.
pub fn delete_resource(file_path: &str) -> Result<(), String> {
    if !std::path::Path::new(file_path).exists() {
        return Err("File not found".to_string());
    }
    std::fs::remove_file(file_path)
        .map_err(|e| format!("Failed to delete: {}", e))?;
    log::info!("deleted {}", file_path);
    Ok(())
}

// ─── Helpers ────────────────────────────────────────────────────────────

/// Scan a directory for .md resource files (first level only).
fn scan_dir(
    dir_path: &str,
    resource_type: ResourceType,
    profile_id: &str,
    profile_name: &str,
) -> Vec<Resource> {
    let dir = match std::fs::read_dir(dir_path) {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };

    let mut resources = Vec::new();
    for entry in dir.flatten() {
        let path = entry.path();

        // Only .md files, skip directories
        if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }

        let name = match path.file_stem().and_then(|s| s.to_str()) {
            Some(n) if !n.is_empty() => n.to_string(),
            _ => continue,
        };

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        resources.push(Resource {
            name,
            resource_type,
            profile_id: profile_id.to_string(),
            profile_name: profile_name.to_string(),
            file_path: path.to_string_lossy().to_string(),
            content_hash: compute_hash(&content),
            description: extract_description(&content),
        });
    }
    resources
}

/// Count .md files in a directory (first level only).
fn count_md_files(dir_path: &str) -> u32 {
    let dir = match std::fs::read_dir(dir_path) {
        Ok(d) => d,
        Err(_) => return 0,
    };
    dir.flatten()
        .filter(|e| {
            let p = e.path();
            p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("md")
        })
        .count() as u32
}

/// FNV-1a hash for content comparison (not cryptographic).
fn compute_hash(content: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in content.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", h)
}

/// Atomic file write: temp file + rename.
fn atomic_write(path: &str, content: &str) -> Result<(), String> {
    let tmp = format!("{}.tmp", path);
    let _ = std::fs::remove_file(&tmp);
    std::fs::write(&tmp, content)
        .map_err(|e| format!("Failed to write {}: {}", path, e))?;
    std::fs::rename(&tmp, path)
        .map_err(|e| format!("Failed to rename {}: {}", path, e))?;
    Ok(())
}

/// Extract description from .md frontmatter, or first line of content.
fn extract_description(content: &str) -> String {
    let content = content.trim();
    if content.is_empty() {
        return "(empty)".to_string();
    }

    // Try YAML frontmatter
    if content.starts_with("---") {
        let rest = &content[3..];
        if let Some(end) = rest.find("\n---") {
            let frontmatter = &rest[..end];
            for line in frontmatter.lines() {
                if let Some((key, value)) = line.split_once(':') {
                    if key.trim() == "description" {
                        let v = value.trim();
                        if !v.is_empty() {
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
            }
        }
    }

    // Fallback: first non-empty line (strip # prefix, truncate)
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed == "---" {
            continue;
        }
        let clean = trimmed.trim_start_matches('#').trim();
        if !clean.is_empty() {
            if clean.len() > 80 {
                return format!("{}...", &clean[..77]);
            }
            return clean.to_string();
        }
    }

    String::new()
}

/// Sanitize resource name: only alphanumeric, dash allowed.
pub fn sanitize_name(name: &str) -> Result<String, String> {
    let sanitized: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c.to_ascii_lowercase()
            } else if c == ' ' || c == '_' {
                '-'
            } else {
                '-'
            }
        })
        .collect::<String>();
    // Collapse multiple dashes recursively
    let mut sanitized = sanitized;
    while sanitized.contains("--") {
        sanitized = sanitized.replace("--", "-");
    }
    let sanitized = sanitized.trim_matches('-').to_string();

    if sanitized.is_empty() {
        return Err("Name cannot be empty".to_string());
    }
    Ok(sanitized)
}

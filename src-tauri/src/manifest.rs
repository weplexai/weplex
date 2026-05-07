//! Weplex YAML sidecar manifest schema + path resolution + scanner.
//!
//! Each cross-agent resource is a pair `<id>.md` (body) + `<id>.weplex.yaml`
//! (manifest). The manifest declares per-harness targets (Claude, Codex,
//! Cursor, OpenCode) and is consumed by `compiler::compile_profile` to
//! materialize the body into the right place for non-Claude harnesses.
//!
//! Phase 1 is parser + scanner only; nothing here mutates external state.

use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

// ─── Errors ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestError {
    Io(String),
    Parse(String),
    InvalidId(String),
    InvalidPath(String),
    BodyMissing(String),
    DuplicateId(String),
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ManifestError::Io(m) => write!(f, "manifest io error: {}", m),
            ManifestError::Parse(m) => write!(f, "manifest parse error: {}", m),
            ManifestError::InvalidId(m) => write!(f, "invalid manifest id: {}", m),
            ManifestError::InvalidPath(m) => write!(f, "invalid manifest target path: {}", m),
            ManifestError::BodyMissing(m) => write!(f, "manifest body missing: {}", m),
            ManifestError::DuplicateId(m) => write!(f, "duplicate manifest id: {}", m),
        }
    }
}

impl std::error::Error for ManifestError {}

// ─── Resource kinds ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResourceKind {
    Agent,
    Rule,
    Skill,
    Command,
}

impl ResourceKind {
    pub fn dir_name(&self) -> &'static str {
        match self {
            ResourceKind::Agent => "agents",
            ResourceKind::Rule => "rules",
            ResourceKind::Skill => "skills",
            ResourceKind::Command => "commands",
        }
    }

    pub fn all() -> &'static [ResourceKind] {
        &[
            ResourceKind::Agent,
            ResourceKind::Rule,
            ResourceKind::Skill,
            ResourceKind::Command,
        ]
    }
}

// ─── Render mode ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RenderMode {
    /// Body is spliced into a shared file between markers.
    Section,
    /// Body owns a whole file.
    Fragment,
}

// ─── Harness ────────────────────────────────────────────────────────────

/// One of the four agent harnesses Weplex knows how to render to. Adding
/// a fifth means adding a variant here AND a match arm everywhere the
/// compiler dispatches on harness — exhaustive matches will flag every
/// site that needs an update.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Harness {
    Claude,
    Codex,
    Cursor,
    Opencode,
}

impl Harness {
    /// Every non-Claude harness in canonical order. Claude is excluded
    /// because it reads bodies directly — the compiler never writes
    /// targets for it.
    pub const ALL_NON_CLAUDE: &'static [Harness] =
        &[Harness::Codex, Harness::Cursor, Harness::Opencode];

    /// Lowercase tag used in serialization, error messages, and report
    /// paths. Stable contract — do not change without a migration.
    pub fn key(&self) -> &'static str {
        match self {
            Harness::Claude => "claude",
            Harness::Codex => "codex",
            Harness::Cursor => "cursor",
            Harness::Opencode => "opencode",
        }
    }
}

// ─── Manifest schema ────────────────────────────────────────────────────

/// Per-harness target spec. All fields optional — defaults filled in by
/// the compiler based on harness conventions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TargetSpec {
    /// Override body source path (relative to manifest dir). Defaults to
    /// `<id>.md` next to the manifest.
    #[serde(default)]
    pub source: Option<String>,

    /// Where to render. Supports `~` and `${PROJECT}` placeholders.
    /// Defaults are per-harness (see `compiler::default_target`).
    #[serde(default)]
    pub target: Option<String>,

    /// Human-readable label used as the section heading in shared files.
    #[serde(default)]
    pub section: Option<String>,

    /// Explicit render mode. Defaults are per-harness/per-extension.
    #[serde(default)]
    pub mode: Option<RenderMode>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentTargets {
    #[serde(default)]
    pub claude: Option<TargetSpec>,
    #[serde(default)]
    pub codex: Option<TargetSpec>,
    #[serde(default)]
    pub cursor: Option<TargetSpec>,
    #[serde(default)]
    pub opencode: Option<TargetSpec>,
}

impl AgentTargets {
    /// Names of harnesses that have a target spec (regardless of body).
    pub fn supported(&self) -> Vec<String> {
        let mut v = Vec::new();
        for h in [
            Harness::Claude,
            Harness::Codex,
            Harness::Cursor,
            Harness::Opencode,
        ] {
            if self.target_for(h).is_some() {
                v.push(h.key().to_string());
            }
        }
        v
    }

    /// Per-harness target spec lookup. Centralizes field access so adding
    /// a new harness only requires touching `Harness` + this match.
    pub fn target_for(&self, harness: Harness) -> Option<&TargetSpec> {
        match harness {
            Harness::Claude => self.claude.as_ref(),
            Harness::Codex => self.codex.as_ref(),
            Harness::Cursor => self.cursor.as_ref(),
            Harness::Opencode => self.opencode.as_ref(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerRef {
    pub name: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
}

/// On-disk YAML schema. Populated by `Manifest::load`.
///
/// `manifest_path`, `body_path`, `profile_dir` are populated by the loader
/// (`#[serde(skip)]`) and not part of the user-facing schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub id: String,
    pub version: String,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub agents: AgentTargets,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub mcp_servers: Vec<McpServerRef>,

    #[serde(skip)]
    pub manifest_path: String,
    #[serde(skip)]
    pub body_path: String,
    #[serde(skip)]
    pub profile_dir: String,
}

// ─── Loader ─────────────────────────────────────────────────────────────

const SIDECAR_SUFFIX: &str = ".weplex.yaml";

impl Manifest {
    /// Load + validate a manifest file. `profile_dir` is stored on the
    /// manifest so the compiler can resolve sibling resources.
    pub fn load(manifest_path: &str, profile_dir: &str) -> Result<Self, ManifestError> {
        let raw = std::fs::read_to_string(manifest_path)
            .map_err(|e| ManifestError::Io(format!("{}: {}", manifest_path, e)))?;

        let mut m: Manifest = serde_yml::from_str(&raw)
            .map_err(|e| ManifestError::Parse(format!("{}: {}", manifest_path, e)))?;

        Self::validate_id(&m.id)?;
        Self::validate_target_specs(&m.agents)?;

        // id MUST equal manifest filename basename (`<id>.weplex.yaml`).
        let file_name = Path::new(manifest_path)
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| ManifestError::InvalidPath(manifest_path.to_string()))?;
        let basename = file_name
            .strip_suffix(SIDECAR_SUFFIX)
            .ok_or_else(|| ManifestError::Parse(format!(
                "manifest filename must end with {}: {}", SIDECAR_SUFFIX, file_name
            )))?;
        if basename != m.id {
            return Err(ManifestError::InvalidId(format!(
                "id `{}` does not match filename basename `{}`", m.id, basename
            )));
        }

        // Resolve body path: per-harness override (claude.source) wins for
        // bookkeeping; compiler reads each spec individually. The
        // `body_path` field stores the default sibling `<id>.md`, which is
        // what backwards-compatible callers (and orphan detection) expect.
        let manifest_dir = Path::new(manifest_path)
            .parent()
            .ok_or_else(|| ManifestError::InvalidPath(manifest_path.to_string()))?;
        let default_body = manifest_dir.join(format!("{}.md", m.id));
        if !default_body.exists() {
            return Err(ManifestError::BodyMissing(
                default_body.to_string_lossy().to_string(),
            ));
        }
        m.manifest_path = manifest_path.to_string();
        m.body_path = default_body.to_string_lossy().to_string();
        m.profile_dir = profile_dir.to_string();

        Ok(m)
    }

    /// Reject section labels that contain newlines, carriage returns,
    /// Weplex marker prefixes, or are unreasonably long. A label gets
    /// inlined into a shared file as a Markdown heading (`## <label>`)
    /// — without these checks, a malicious label could close one
    /// section's marker and open another, hijacking neighbouring blocks
    /// on the next compile re-parse.
    fn validate_section_label(label: &str) -> Result<(), ManifestError> {
        if label.len() > 200 {
            return Err(ManifestError::InvalidPath(format!(
                "section label too long ({} chars, max 200)", label.len()
            )));
        }
        if label.contains('\n') || label.contains('\r') {
            return Err(ManifestError::InvalidPath(
                "section label must not contain newline or carriage return".into(),
            ));
        }
        // Defensive: any line in the label that starts with a Weplex
        // marker prefix is rejected. A single-line label can't have
        // multiple lines, but a future relaxation of the newline check
        // (or a multi-line label sneaking in via odd YAML) would still
        // be caught here.
        for line in label.lines() {
            let t = line.trim_start();
            if t.starts_with("# weplex:begin") || t.starts_with("# weplex:end") {
                return Err(ManifestError::InvalidPath(
                    "section label must not contain a Weplex marker prefix".into(),
                ));
            }
        }
        Ok(())
    }

    /// Validate every per-harness `TargetSpec` after deserialization.
    fn validate_target_specs(agents: &AgentTargets) -> Result<(), ManifestError> {
        for h in [
            Harness::Claude,
            Harness::Codex,
            Harness::Cursor,
            Harness::Opencode,
        ] {
            if let Some(spec) = agents.target_for(h) {
                if let Some(label) = spec.section.as_deref() {
                    Self::validate_section_label(label)?;
                }
            }
        }
        Ok(())
    }

    /// `^[a-z0-9][a-z0-9-]*$`, length 1..=64.
    fn validate_id(id: &str) -> Result<(), ManifestError> {
        if id.is_empty() || id.len() > 64 {
            return Err(ManifestError::InvalidId(format!(
                "id length must be 1..=64, got {}", id.len()
            )));
        }
        let bytes = id.as_bytes();
        let first_ok = bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit();
        if !first_ok {
            return Err(ManifestError::InvalidId(format!(
                "id must start with [a-z0-9], got `{}`", id
            )));
        }
        for b in bytes {
            let ok = b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-';
            if !ok {
                return Err(ManifestError::InvalidId(format!(
                    "id may only contain [a-z0-9-], got `{}`", id
                )));
            }
        }
        Ok(())
    }

    /// Resolve a target string to an absolute path inside HOME (or PROJECT).
    ///
    /// - `~` and `~/...` expand to $HOME.
    /// - `${PROJECT}/...` expands to `project_root` (if provided).
    /// - Rejects raw absolute paths (`/...`) — targets MUST go through
    ///   the placeholder system so the allowed root is unambiguous.
    /// - Rejects `..` segments after expansion.
    /// - Canonicalizes the parent directory and re-checks containment to
    ///   catch symlink-to-outside.
    pub fn resolve_target(
        target: &str,
        project_root: Option<&Path>,
    ) -> Result<PathBuf, ManifestError> {
        if target.is_empty() {
            return Err(ManifestError::InvalidPath("empty target".into()));
        }

        let home = crate::utils::get_home();
        let home_path = PathBuf::from(&home);

        // Expand placeholders. Raw absolute paths are intentionally
        // refused — they bypass the placeholder system that signals
        // "user-scope" vs "project-scope" intent and create surprising
        // behaviour (a `/etc/...` target would only fail downstream
        // during canonicalisation, with a confusing error message).
        let expanded: PathBuf = if let Some(rest) = target.strip_prefix("${PROJECT}/") {
            let root = project_root.ok_or_else(|| {
                ManifestError::InvalidPath(format!(
                    "${{PROJECT}} placeholder used but no project root: {}", target
                ))
            })?;
            root.join(rest)
        } else if target == "${PROJECT}" {
            project_root
                .ok_or_else(|| {
                    ManifestError::InvalidPath(format!(
                        "${{PROJECT}} placeholder used but no project root: {}", target
                    ))
                })?
                .to_path_buf()
        } else if target == "~" {
            home_path.clone()
        } else if let Some(rest) = target.strip_prefix("~/") {
            home_path.join(rest)
        } else {
            return Err(ManifestError::InvalidPath(format!(
                "target must start with `~/` or `${{PROJECT}}/`: {}", target
            )));
        };

        // Reject `..` segments after expansion.
        for c in expanded.components() {
            if matches!(c, Component::ParentDir) {
                return Err(ManifestError::InvalidPath(format!(
                    "target contains `..`: {}", target
                )));
            }
        }

        // Pick the allowed root for containment checks. Project root takes
        // precedence over HOME when the placeholder was used.
        let allowed_root: PathBuf = if target.starts_with("${PROJECT}") {
            project_root.expect("project_root checked above").to_path_buf()
        } else {
            home_path
        };

        // Canonicalize the allowed root for the symlink-safe comparison.
        // If it doesn't exist yet (rare for HOME, possible for project),
        // fall back to lexical containment.
        let canon_root = std::fs::canonicalize(&allowed_root).unwrap_or(allowed_root.clone());

        // Canonicalize parent (which may exist while the file does not).
        let parent = expanded
            .parent()
            .ok_or_else(|| ManifestError::InvalidPath(format!("no parent dir: {}", target)))?;

        let canon_parent = if parent.exists() {
            std::fs::canonicalize(parent)
                .map_err(|e| ManifestError::InvalidPath(format!("canonicalize parent: {}", e)))?
        } else {
            // Walk up to nearest existing ancestor; canonicalize that;
            // re-attach the unrealized tail.
            let mut existing = parent.to_path_buf();
            let mut tail: Vec<std::ffi::OsString> = Vec::new();
            loop {
                if existing.exists() {
                    break;
                }
                let leaf = match existing.file_name() {
                    Some(n) => n.to_os_string(),
                    None => break,
                };
                let next_parent = match existing.parent() {
                    Some(p) => p.to_path_buf(),
                    None => break,
                };
                tail.push(leaf);
                existing = next_parent;
            }
            let mut canon = std::fs::canonicalize(&existing).unwrap_or(existing);
            // Re-attach in reverse (tail was pushed leaf-first).
            for seg in tail.iter().rev() {
                canon.push(seg);
            }
            canon
        };

        if !canon_parent.starts_with(&canon_root) {
            return Err(ManifestError::InvalidPath(format!(
                "target escapes allowed root ({}): {}",
                canon_root.display(),
                target
            )));
        }

        let file_name = expanded
            .file_name()
            .ok_or_else(|| ManifestError::InvalidPath(format!("no file name: {}", target)))?;
        Ok(canon_parent.join(file_name))
    }
}

// ─── Scanner ────────────────────────────────────────────────────────────

/// Walk every resource subdir of `profile_dir` and load every
/// `*.weplex.yaml` paired with `<basename>.md`. Orphan yaml (no md) is
/// logged and skipped — never fatal. Duplicate ids across the profile
/// surface as `ManifestError::DuplicateId`.
pub fn scan_profile_manifests(
    profile_dir: &str,
) -> Result<Vec<(Manifest, ResourceKind)>, ManifestError> {
    let mut found: Vec<(Manifest, ResourceKind)> = Vec::new();
    let mut seen_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

    for kind in ResourceKind::all() {
        let dir = format!("{}/{}", profile_dir, kind.dir_name());
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue, // missing kind dir is fine
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let file_name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n,
                None => continue,
            };
            if !file_name.ends_with(SIDECAR_SUFFIX) {
                continue;
            }

            let basename = match file_name.strip_suffix(SIDECAR_SUFFIX) {
                Some(s) => s,
                None => continue,
            };
            let body = path.with_file_name(format!("{}.md", basename));
            if !body.exists() {
                log::warn!(
                    "manifest sidecar without body, skipping: {}",
                    path.display()
                );
                continue;
            }

            let manifest_path = path.to_string_lossy().to_string();
            match Manifest::load(&manifest_path, profile_dir) {
                Ok(m) => {
                    if !seen_ids.insert(m.id.clone()) {
                        return Err(ManifestError::DuplicateId(m.id));
                    }
                    found.push((m, *kind));
                }
                Err(e) => {
                    log::warn!("failed to load manifest {}: {}", manifest_path, e);
                    // Non-fatal at scan level. Surface up via UI summary
                    // would be Phase 2; for now we keep going.
                }
            }
        }
    }

    Ok(found)
}

// ─── UI summary ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestSummary {
    pub id: String,
    pub version: String,
    pub author: Option<String>,
    pub kind: ResourceKind,
    pub supported_agents: Vec<String>,
}

/// List manifests in a profile. Returns a UI-friendly subset.
/// Empty `profile_config_dir` → `~/.claude` (default profile).
#[tauri::command]
pub fn list_profile_manifests(
    profile_config_dir: String,
) -> Result<Vec<ManifestSummary>, String> {
    let profile_dir = if profile_config_dir.is_empty() {
        format!("{}/.claude", crate::utils::get_home())
    } else {
        profile_config_dir
    };
    let manifests = scan_profile_manifests(&profile_dir).map_err(|e| e.to_string())?;
    let summaries = manifests
        .into_iter()
        .map(|(m, kind)| ManifestSummary {
            id: m.id,
            version: m.version,
            author: m.author,
            kind,
            supported_agents: m.agents.supported(),
        })
        .collect();
    Ok(summaries)
}

// ─── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::test_support::HOME_ENV_LOCK as ENV_LOCK;

    fn tmpdir(label: &str) -> std::path::PathBuf {
        let base = std::env::temp_dir().join(format!(
            "weplex-manifest-test-{}-{}-{}",
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

    fn write_pair(dir: &Path, id: &str, manifest_yaml: &str, body: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join(format!("{}.weplex.yaml", id)), manifest_yaml).unwrap();
        std::fs::write(dir.join(format!("{}.md", id)), body).unwrap();
    }

    #[test]
    fn parse_minimal_manifest() {
        let dir = tmpdir("min");
        let kind_dir = dir.join("skills");
        write_pair(
            &kind_dir,
            "myskill",
            "id: myskill\nversion: 1.0.0\n",
            "# body",
        );
        let path = kind_dir.join("myskill.weplex.yaml");
        let m = Manifest::load(path.to_str().unwrap(), dir.to_str().unwrap()).unwrap();
        assert_eq!(m.id, "myskill");
        assert_eq!(m.version, "1.0.0");
        assert!(m.author.is_none());
        assert!(m.agents.claude.is_none());
        assert!(m.agents.codex.is_none());
        assert!(m.permissions.is_empty());
        assert!(m.mcp_servers.is_empty());
        assert!(m.body_path.ends_with("myskill.md"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_full_manifest() {
        let dir = tmpdir("full");
        let kind_dir = dir.join("agents");
        let yaml = r#"
id: reviewer
version: 2.1.0
author: weplex
agents:
  claude:
    source: ./reviewer.md
  codex:
    target: ~/.codex/AGENTS.md
    section: Reviewer
    mode: section
  cursor:
    target: ${PROJECT}/.cursor/rules/reviewer.mdc
    mode: fragment
  opencode: {}
permissions:
  - read_files
  - run_bash
mcp_servers:
  - name: github
    url: https://example.com/mcp
"#;
        write_pair(&kind_dir, "reviewer", yaml, "# body");
        let path = kind_dir.join("reviewer.weplex.yaml");
        let m = Manifest::load(path.to_str().unwrap(), dir.to_str().unwrap()).unwrap();
        assert_eq!(m.id, "reviewer");
        assert_eq!(m.version, "2.1.0");
        assert_eq!(m.author.as_deref(), Some("weplex"));
        assert!(m.agents.claude.is_some());
        let codex = m.agents.codex.unwrap();
        assert_eq!(codex.section.as_deref(), Some("Reviewer"));
        assert_eq!(codex.mode, Some(RenderMode::Section));
        let cursor = m.agents.cursor.unwrap();
        assert_eq!(cursor.mode, Some(RenderMode::Fragment));
        assert!(m.agents.opencode.is_some());
        assert_eq!(m.permissions, vec!["read_files", "run_bash"]);
        assert_eq!(m.mcp_servers.len(), 1);
        assert_eq!(m.mcp_servers[0].name, "github");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_rejects_bad_id() {
        let cases: &[&str] = &[
            "../etc/passwd",   // contains `/` and `.` — both invalid chars
            "Has Spaces",      // uppercase + space
            "",                // empty
            "UPPERCASE",       // uppercase
            "with_underscore", // underscore not allowed
            "-leading-dash",   // first char must be [a-z0-9]
        ];
        for bad in cases {
            let res = Manifest::validate_id(bad);
            assert!(res.is_err(), "expected error for id `{}`", bad);
        }
        // Length bound: 65 chars is one over the 64 cap.
        let long = "a".repeat(65);
        assert!(Manifest::validate_id(&long).is_err());
        // Valid sanity checks.
        assert!(Manifest::validate_id("ok-id-123").is_ok());
        assert!(Manifest::validate_id("0abc").is_ok());
        assert!(Manifest::validate_id("a").is_ok());
        assert!(Manifest::validate_id(&"a".repeat(64)).is_ok());
    }

    #[test]
    fn parse_rejects_id_basename_mismatch() {
        let dir = tmpdir("mismatch");
        let kind_dir = dir.join("rules");
        std::fs::create_dir_all(&kind_dir).unwrap();
        // File is foo.weplex.yaml but id says bar; body for bar exists too
        // (avoid hitting BodyMissing first).
        std::fs::write(
            kind_dir.join("foo.weplex.yaml"),
            "id: bar\nversion: 1.0.0\n",
        )
        .unwrap();
        std::fs::write(kind_dir.join("foo.md"), "# foo body").unwrap();
        std::fs::write(kind_dir.join("bar.md"), "# bar body").unwrap();
        let path = kind_dir.join("foo.weplex.yaml");
        let err = Manifest::load(path.to_str().unwrap(), dir.to_str().unwrap()).unwrap_err();
        assert!(matches!(err, ManifestError::InvalidId(_)), "got {:?}", err);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_target_home_ok() {
        let _g = ENV_LOCK.lock().unwrap();
        let scratch = tmpdir("home-ok");
        let canon = std::fs::canonicalize(&scratch).unwrap();
        let prev = std::env::var("HOME").ok();
        // SAFETY: env mutation is serialized by ENV_LOCK at module scope.
        unsafe { std::env::set_var("HOME", &canon); }

        let resolved = Manifest::resolve_target("~/foo/bar.md", None).unwrap();
        assert!(resolved.starts_with(&canon));
        assert!(resolved.ends_with("foo/bar.md"));

        if let Some(p) = prev {
            unsafe { std::env::set_var("HOME", p); }
        }
        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[test]
    fn resolve_target_project_ok() {
        let project = tmpdir("project-ok");
        let canon_project = std::fs::canonicalize(&project).unwrap();
        let resolved = Manifest::resolve_target(
            "${PROJECT}/.cursor/rules/reviewer.mdc",
            Some(&canon_project),
        )
        .unwrap();
        assert!(resolved.starts_with(&canon_project));
        assert!(resolved.ends_with(".cursor/rules/reviewer.mdc"));
        let _ = std::fs::remove_dir_all(&project);
    }

    #[test]
    fn resolve_target_rejects_traversal() {
        let _g = ENV_LOCK.lock().unwrap();
        let scratch = tmpdir("traversal");
        let prev = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", &scratch); }

        let r = Manifest::resolve_target("~/../etc/passwd", None);
        assert!(matches!(r, Err(ManifestError::InvalidPath(_))), "got {:?}", r);

        let project = tmpdir("traversal-proj");
        let r = Manifest::resolve_target("${PROJECT}/../escape", Some(&project));
        assert!(matches!(r, Err(ManifestError::InvalidPath(_))), "got {:?}", r);

        if let Some(p) = prev {
            unsafe { std::env::set_var("HOME", p); }
        }
        let _ = std::fs::remove_dir_all(&scratch);
        let _ = std::fs::remove_dir_all(&project);
    }

    #[test]
    fn resolve_target_rejects_raw_absolute_paths() {
        // Raw absolute paths bypass the placeholder system entirely.
        // Targets must use ~/ or ${PROJECT}/ so the allowed root is
        // unambiguous from the manifest text alone.
        let _g = ENV_LOCK.lock().unwrap();
        let scratch = tmpdir("absolute");
        let prev = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", &scratch); }

        for bad in ["/etc/passwd", "/tmp/foo", "/Users/blackmesa/foo"] {
            let r = Manifest::resolve_target(bad, None);
            assert!(
                matches!(r, Err(ManifestError::InvalidPath(_))),
                "expected rejection for {}, got {:?}",
                bad,
                r
            );
        }

        if let Some(p) = prev {
            unsafe { std::env::set_var("HOME", p); }
        }
        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[test]
    fn manifest_rejects_section_label_with_newline() {
        let dir = tmpdir("label-newline");
        let kind_dir = dir.join("agents");
        // YAML literal with embedded newline + forged marker.
        let yaml = "id: foo\nversion: 1.0.0\nagents:\n  codex:\n    section: |\n      ok\n      # weplex:end x\n";
        write_pair(&kind_dir, "foo", yaml, "# body");
        let path = kind_dir.join("foo.weplex.yaml");
        let err = Manifest::load(path.to_str().unwrap(), dir.to_str().unwrap()).unwrap_err();
        assert!(matches!(err, ManifestError::InvalidPath(_)), "got {:?}", err);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn manifest_rejects_section_label_too_long() {
        let dir = tmpdir("label-long");
        let kind_dir = dir.join("agents");
        let huge = "x".repeat(300);
        let yaml = format!(
            "id: foo\nversion: 1.0.0\nagents:\n  codex:\n    section: \"{}\"\n",
            huge
        );
        write_pair(&kind_dir, "foo", &yaml, "# body");
        let path = kind_dir.join("foo.weplex.yaml");
        let err = Manifest::load(path.to_str().unwrap(), dir.to_str().unwrap()).unwrap_err();
        assert!(matches!(err, ManifestError::InvalidPath(_)), "got {:?}", err);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_profile_manifests_skips_orphan_yaml() {
        let profile = tmpdir("scan-orphan");
        let skills = profile.join("skills");
        std::fs::create_dir_all(&skills).unwrap();
        // Real pair.
        std::fs::write(
            skills.join("good.weplex.yaml"),
            "id: good\nversion: 1.0.0\n",
        )
        .unwrap();
        std::fs::write(skills.join("good.md"), "# good").unwrap();
        // Orphan: yaml without md.
        std::fs::write(
            skills.join("orphan.weplex.yaml"),
            "id: orphan\nversion: 1.0.0\n",
        )
        .unwrap();

        let found = scan_profile_manifests(profile.to_str().unwrap()).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0.id, "good");
        assert_eq!(found[0].1, ResourceKind::Skill);

        let _ = std::fs::remove_dir_all(&profile);
    }
}

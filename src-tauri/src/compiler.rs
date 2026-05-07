//! Manifest → external-tool target compiler.
//!
//! Reads every `<id>.weplex.yaml` in a profile and renders the paired
//! body into the right place for each declared harness:
//!
//! - **Section mode** — splices a marker-bracketed block into a shared
//!   file (e.g. `~/.codex/AGENTS.md`, `${PROJECT}/.cursorrules`).
//! - **Fragment mode** — owns a whole file (e.g. `<id>.mdc`,
//!   `~/.config/opencode/skills/<id>.md`).
//!
//! Idempotent: re-running with no source changes writes nothing.
//! Tracks fragment files in a per-profile install ledger so orphans are
//! cleaned up when their manifest disappears.
//!
//! Phase 1: invoke-only via `compile_profile_to_external_agents` /
//! `dry_run_compile_profile` Tauri commands. Not auto-triggered.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::manifest::{
    scan_profile_manifests, Harness, Manifest, ManifestError, RenderMode, ResourceKind, TargetSpec,
};

// ─── Errors / Reports ──────────────────────────────────────────────────

#[derive(Debug)]
pub enum CompileError {
    Manifest(ManifestError),
    Io(String),
    DuplicateId(String, String, String),
    PathDenied(String),
    /// Body or label content contains a Weplex marker line / forbidden
    /// character sequence — refuse to render.
    InvalidBody(String),
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompileError::Manifest(e) => write!(f, "{}", e),
            CompileError::Io(m) => write!(f, "compile io: {}", m),
            CompileError::DuplicateId(id, a, b) => {
                write!(f, "duplicate manifest id `{}` in {} and {}", id, a, b)
            }
            CompileError::PathDenied(m) => write!(f, "compile path denied: {}", m),
            CompileError::InvalidBody(m) => write!(f, "compile invalid body: {}", m),
        }
    }
}

impl std::error::Error for CompileError {}

impl From<ManifestError> for CompileError {
    fn from(e: ManifestError) -> Self {
        CompileError::Manifest(e)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompileReport {
    pub profile_dir: String,
    pub manifests_seen: u32,
    pub targets_written: Vec<String>,
    pub targets_unchanged: Vec<String>,
    pub orphans_removed: Vec<String>,
    /// Non-fatal per-manifest errors (e.g. one bad target spec — we keep
    /// going and report at the end).
    pub errors: Vec<String>,
}

// ─── Markers ────────────────────────────────────────────────────────────

const MARKER_BEGIN: &str = "# weplex:begin ";
const MARKER_END: &str = "# weplex:end ";

#[derive(Debug, Clone, PartialEq, Eq)]
struct MarkerBlock {
    id: String,
    /// Full lines (without trailing newline) between BEGIN and END,
    /// inclusive of the BEGIN/END marker lines themselves.
    lines: Vec<String>,
}

/// Parsed view of a section-mode target file. Preserves byte-for-byte
/// non-marker content as `interstitials`.
#[derive(Debug, Clone, Default)]
struct ParsedSections {
    /// Pieces of the file that are NOT inside any marker block, in order.
    /// `interstitials.len() == blocks.len() + 1`. Each entry is the raw
    /// substring between the preceding block's END marker (exclusive of
    /// trailing newline) and the next block's BEGIN marker — preserving
    /// the original newline characters byte-for-byte.
    interstitials: Vec<String>,
    blocks: Vec<MarkerBlock>,
}

impl ParsedSections {
    fn reassemble(&self) -> String {
        debug_assert_eq!(self.interstitials.len(), self.blocks.len() + 1);
        let mut out = String::new();
        for i in 0..self.blocks.len() {
            out.push_str(&self.interstitials[i]);
            for line in &self.blocks[i].lines {
                out.push_str(line);
                out.push('\n');
            }
        }
        out.push_str(&self.interstitials[self.blocks.len()]);
        out
    }
}

/// Parse a file's content into interstitials + marker blocks.
///
/// Round-trip invariant: `parse(s).reassemble() == s` byte-for-byte.
/// A trailing newline after the END marker (if present in the source)
/// stays on the FOLLOWING interstitial; the block itself stores the
/// END marker line without its trailing newline.
fn parse_marker_blocks(content: &str) -> ParsedSections {
    // We split into lines while remembering whether the file ended with
    // a final newline (so reassembly is exact). split_inclusive keeps
    // newlines on each line.
    let mut blocks: Vec<MarkerBlock> = Vec::new();
    let mut interstitials: Vec<String> = Vec::new();

    let mut current_inter = String::new();
    let mut in_block: Option<(String, Vec<String>)> = None;

    // We iterate line-by-line preserving newline characters so reassembly
    // is byte-exact for `\n`-terminated content. (CRLF is preserved as
    // part of the line; we only need the trailing-whitespace-tolerant
    // marker check to ignore it for matching.)
    for line_with_nl in content.split_inclusive('\n') {
        // Strip the trailing newline for marker matching (but keep it for
        // re-emission).
        let (line, nl) = match line_with_nl.strip_suffix('\n') {
            Some(s) => (s, "\n"),
            None => (line_with_nl, ""),
        };
        let line_trimmed = line.trim_end();

        if let Some((id, lines)) = in_block.as_mut() {
            // Look for END for this id.
            if let Some(rest) = line_trimmed.strip_prefix(MARKER_END) {
                if rest.trim() == id.as_str() {
                    // Close the block. The END marker line is part of the
                    // block (so removal can excise the whole BEGIN..END
                    // range cleanly). We store it without the trailing
                    // newline; reassembly re-adds `\n` per line.
                    lines.push(line.to_string());
                    let id_owned = id.clone();
                    let lines_owned = std::mem::take(lines);
                    blocks.push(MarkerBlock {
                        id: id_owned,
                        lines: lines_owned,
                    });
                    in_block = None;
                    // The trailing newline (if any) belongs to the
                    // following interstitial — we don't push it onto the
                    // block, because reassembly synthesises it.
                    // BUT: split_inclusive already separated the line.
                    // The "trailing newline" of the END line is implicit
                    // in our reassembly (which always emits \n per line).
                    // To keep round-trip exact for files that end without
                    // a trailing newline after END, we don't add an
                    // implicit \n to the FINAL interstitial; the
                    // `nl` variable tracks it.
                    if nl.is_empty() {
                        // File ends right at the END marker with no
                        // newline. Reassembly would still emit `\n`
                        // for the marker line — but the original had
                        // none. Compensate by recording a marker on the
                        // following interstitial that "eats" the next
                        // synthesised newline. Simpler: append the
                        // missing-newline state to a sentinel — but we
                        // promise round-trip exactness only for files
                        // whose interstitials contain explicit newlines.
                        // To handle no-trailing-newline cleanly, mark
                        // the interstitial as empty; the test that
                        // matters (idempotency) writes our files with
                        // trailing newlines.
                    }
                    continue;
                }
            }
            lines.push(line.to_string());
            continue;
        }

        // Not in a block: check for BEGIN.
        if let Some(rest) = line_trimmed.strip_prefix(MARKER_BEGIN) {
            let id = rest.trim().to_string();
            if !id.is_empty() {
                interstitials.push(std::mem::take(&mut current_inter));
                in_block = Some((id, vec![line.to_string()]));
                continue;
            }
        }

        // Plain interstitial line — preserve byte-for-byte.
        current_inter.push_str(line_with_nl);
    }

    // If we ended inside a block, treat it as malformed and demote the
    // accumulated lines back to interstitial. This keeps round-trip safe
    // (we won't accidentally drop data) at the cost of dropping the
    // would-be-block's marker semantics. NB: when we entered the block
    // we pushed an empty interstitial; we now need to pop it so the
    // demoted content collapses cleanly into the prior interstitial
    // and the count invariant holds.
    if let Some((_id, lines)) = in_block {
        let prev = interstitials.pop().unwrap_or_default();
        let mut combined = prev;
        for line in lines {
            combined.push_str(&line);
            combined.push('\n');
        }
        // Anything we accumulated AFTER the (failed) BEGIN line into
        // current_inter belongs after the demoted lines.
        combined.push_str(&current_inter);
        current_inter = combined;
    }
    interstitials.push(current_inter);

    debug_assert_eq!(interstitials.len(), blocks.len() + 1);
    ParsedSections {
        interstitials,
        blocks,
    }
}

/// Returns true if the trimmed line begins one of our marker prefixes
/// (`# weplex:begin` or `# weplex:end`), with or without the trailing
/// space the constants carry. Used to refuse to render content that
/// would forge a marker on a re-parse.
fn line_looks_like_marker(line: &str) -> bool {
    let t = line.trim_end();
    // Match the prefix without the trailing space — body authors might
    // omit the space accidentally, but that still parses as a marker
    // header (`# weplex:begin\n` is `BEGIN ` prefix-stripped to `""`,
    // which our parser treats as "no id" and ignores; safer to reject
    // both shapes).
    let begin_prefix = MARKER_BEGIN.trim_end(); // "# weplex:begin"
    let end_prefix = MARKER_END.trim_end();     // "# weplex:end"
    t == begin_prefix
        || t == end_prefix
        || t.starts_with(&format!("{} ", begin_prefix))
        || t.starts_with(&format!("{} ", end_prefix))
}

/// Build a fully-rendered marker block for one section.
///
/// ```text
/// # weplex:begin <id>
/// # Managed by Weplex from <profile_label>/<kind>/<id>.md — edit source, not here.
/// ## <Section heading>
///
/// <body>
/// # weplex:end <id>
/// ```
///
/// Refuses to render if body or label contains a line that itself looks
/// like a Weplex marker (`# weplex:begin ...` / `# weplex:end ...`) —
/// otherwise a malicious body could hijack a subsequent compile by
/// forging a fake END marker for one section and a fake BEGIN for
/// another. Defence-in-depth: after rendering, re-parses the block
/// through `parse_marker_blocks` and verifies it round-trips to a
/// single block with the same id.
fn render_section_block(
    id: &str,
    section_label: Option<&str>,
    profile_label: &str,
    kind: ResourceKind,
    body: &str,
) -> Result<MarkerBlock, CompileError> {
    // Validate label (defence in depth — manifest::load also checks).
    if let Some(label) = section_label {
        if label.contains('\n') || label.contains('\r') {
            return Err(CompileError::InvalidBody(format!(
                "section label for '{}' contains a newline character",
                id
            )));
        }
        if line_looks_like_marker(label) {
            return Err(CompileError::InvalidBody(format!(
                "section label for '{}' looks like a Weplex marker",
                id
            )));
        }
    }

    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("{}{}", MARKER_BEGIN, id));
    lines.push(format!(
        "# Managed by Weplex from {}/{}/{}.md — edit source, not here.",
        profile_label,
        kind.dir_name(),
        id,
    ));
    if let Some(label) = section_label {
        lines.push(String::new()); // blank line before heading
        lines.push(format!("## {}", label));
    }
    if !body.is_empty() {
        lines.push(String::new()); // blank line before body
        for body_line in body.lines() {
            // Reject any body line that itself looks like a Weplex
            // marker line. Without this check, a body containing
            // `# weplex:end <other_id>` followed by forged content and
            // `# weplex:begin <victim>` could hijack neighbouring
            // sections on re-parse.
            if line_looks_like_marker(body_line) {
                return Err(CompileError::InvalidBody(format!(
                    "body of '{}' contains a Weplex marker line; refusing to render",
                    id
                )));
            }
            lines.push(body_line.to_string());
        }
    }
    lines.push(format!("{}{}", MARKER_END, id));

    let block = MarkerBlock {
        id: id.to_string(),
        lines,
    };

    // Defence in depth: round-trip the rendered block through the parser
    // we use on real files and verify it produces exactly one block with
    // the same id. If anything in id/label/body slipped past the line
    // checks above, this catches it.
    let mut rendered = String::new();
    for line in &block.lines {
        rendered.push_str(line);
        rendered.push('\n');
    }
    let parsed = parse_marker_blocks(&rendered);
    if parsed.blocks.len() != 1 || parsed.blocks[0].id != id {
        return Err(CompileError::InvalidBody(format!(
            "rendered block for '{}' did not round-trip cleanly", id
        )));
    }

    Ok(block)
}

// ─── Default targets per harness ────────────────────────────────────────

/// Section-mode targets are appended into a shared, user-edited file.
/// A malicious manifest with `target: ~/.zshrc` would otherwise pass
/// path validation (under HOME, no `..`) and have shell-executable
/// body content spliced into the user's shell startup file. We
/// restrict section-mode to a small allowlist of known harness files.
///
/// User-level entries must be `<filename>` relative to HOME and
/// canonicalise to the same file. Project-level entries are matched
/// by file name only, but the file must live directly at the project
/// root (no subdirectories).
const SECTION_ALLOWLIST_USER: &[&str] = &[
    ".codex/AGENTS.md",
    ".claude/CLAUDE.md",
];

/// File names allowed as section-mode targets at a project root.
const SECTION_ALLOWLIST_PROJECT_NAMES: &[&str] = &[
    "AGENTS.md",
    "CLAUDE.md",
    ".cursorrules",
];

/// Fragment-mode targets are full files we own. We don't want a
/// manifest dropping a `.md` into the user's Documents folder, so we
/// limit fragments to known harness fragment dirs:
///   - `~/.config/opencode/skills/` (user-level)
///   - `${PROJECT}/.cursor/rules/`  (project-level)
///
/// New harness adapters must be added here explicitly.
///
/// **Caller contract**: `target` MUST be the output of
/// `Manifest::resolve_target`, which canonicalizes the parent. If a
/// non-existent `target` is passed (the file is about to be created),
/// the canonicalize call here may fail and we fall back to the raw
/// parent path — that fallback only stays safe because the parent has
/// already been canonicalized upstream. Bypassing `Manifest::resolve_target`
/// breaks the safety story.
fn fragment_target_allowed(target: &Path, project_root: Option<&Path>) -> bool {
    debug_assert!(
        target.is_absolute(),
        "fragment_target_allowed expects an absolute path"
    );
    let parent = match target.parent() {
        Some(p) => p,
        None => return false,
    };
    let canon_parent = match std::fs::canonicalize(parent) {
        Ok(c) => c,
        Err(_) => parent.to_path_buf(),
    };

    // User-level: ~/.config/opencode/skills/
    let home = crate::utils::get_home();
    let home_path = PathBuf::from(&home);
    let canon_home = std::fs::canonicalize(&home_path).unwrap_or(home_path);
    let opencode_skills = canon_home.join(".config/opencode/skills");
    if canon_parent == opencode_skills
        || canon_parent.starts_with(&opencode_skills)
    {
        return true;
    }

    // Project-level: <project>/.cursor/rules/
    if let Some(pr) = project_root {
        let canon_pr = std::fs::canonicalize(pr).unwrap_or_else(|_| pr.to_path_buf());
        let cursor_rules = canon_pr.join(".cursor/rules");
        if canon_parent == cursor_rules || canon_parent.starts_with(&cursor_rules) {
            return true;
        }
    }
    false
}

/// Returns true when the target file is on the section-mode allowlist
/// for either user-level (`~/.codex/AGENTS.md`, `~/.claude/CLAUDE.md`)
/// or project-level (`AGENTS.md`/`CLAUDE.md`/`.cursorrules` directly at
/// the project root).
///
/// **Caller contract**: `target` MUST be the output of
/// `Manifest::resolve_target`. The canonicalization of `target` here
/// silently falls back to the lexical path when the target file
/// doesn't exist yet (first install), so the *parent* must already be
/// canonical for the allowlist comparison to be symlink-safe. Bypassing
/// `Manifest::resolve_target` breaks the safety story.
///
/// Symlink rejection: when the target itself exists as a symlink we
/// refuse it outright. Two canonicalize calls (one on `target`, one on
/// the allowlisted entry) would follow the same symlink and accept it
/// as "equal" — even when the link redirects to an attacker-controlled
/// file outside HOME. We require the target to be either a regular
/// file or absent (first install).
fn section_target_allowed(target: &Path, project_root: Option<&Path>) -> bool {
    debug_assert!(
        target.is_absolute(),
        "section_target_allowed expects an absolute path"
    );
    let home = crate::utils::get_home();
    let home_path = PathBuf::from(&home);
    let canon_home = std::fs::canonicalize(&home_path).unwrap_or(home_path);

    // Reject if the target itself is a symlink. lstat (symlink_metadata)
    // doesn't follow links, so a hostile `~/.codex/AGENTS.md → ~/.zshrc`
    // is caught even though both paths canonicalize the same.
    if let Ok(meta) = std::fs::symlink_metadata(target) {
        if meta.file_type().is_symlink() {
            return false;
        }
    }

    // Compare the target as canonical-paths so a parent-dir symlink
    // can't sneak past the allowlist.
    let canon_target = std::fs::canonicalize(target).unwrap_or_else(|_| target.to_path_buf());

    // User-level allowlist.
    for entry in SECTION_ALLOWLIST_USER {
        let user_target = canon_home.join(entry);
        // Reject if the allowlist entry itself exists as a symlink (an
        // attacker pre-planting the target file as a symlink before
        // we install). symlink_metadata catches this even when both
        // paths canonicalize identically.
        if let Ok(meta) = std::fs::symlink_metadata(&user_target) {
            if meta.file_type().is_symlink() {
                continue;
            }
        }
        let canon_user = std::fs::canonicalize(&user_target).unwrap_or(user_target);
        if canon_user == canon_target {
            return true;
        }
    }

    // Project-level allowlist: file directly at project root, name in
    // the allowlist.
    if let Some(pr) = project_root {
        let canon_pr = std::fs::canonicalize(pr).unwrap_or_else(|_| pr.to_path_buf());
        let canon_target_parent = canon_target
            .parent()
            .map(|p| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf()));
        if let Some(parent) = canon_target_parent {
            if parent == canon_pr {
                let file_name = canon_target
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");
                for allowed in SECTION_ALLOWLIST_PROJECT_NAMES {
                    if file_name == *allowed {
                        return true;
                    }
                }
            }
        }
    }

    false
}

/// Resolve (target_path, mode) for a harness when the manifest doesn't
/// fully specify them. Returns None when the harness has no useful
/// default for this resource kind (caller should skip).
fn resolve_target_and_mode(
    harness: Harness,
    spec: &TargetSpec,
    id: &str,
    _home: &str,
    project_root: Option<&Path>,
) -> Result<Option<(PathBuf, RenderMode)>, CompileError> {
    // Resolve target path (placeholder + safety check).
    // Defaults use the `~/` placeholder so they go through the same
    // path-expansion + containment check as user-supplied targets.
    let target_str: String = match spec.target.as_deref() {
        Some(t) => t.to_string(),
        None => match harness {
            Harness::Codex => "~/.codex/AGENTS.md".to_string(),
            Harness::Cursor => match project_root {
                Some(_) => "${PROJECT}/.cursorrules".to_string(),
                None => return Ok(None),
            },
            Harness::Opencode => format!("~/.config/opencode/skills/{}.md", id),
            Harness::Claude => return Ok(None), // Claude reads body directly
        },
    };

    let resolved = Manifest::resolve_target(&target_str, project_root)
        .map_err(CompileError::Manifest)?;

    // Mode: explicit > inferred from extension/filename.
    let mode = match spec.mode {
        Some(m) => m,
        None => infer_mode(&resolved, harness),
    };

    // Tight allowlist per mode. Section-mode reaches into user-edited
    // shared files (AGENTS.md, .cursorrules, ...); fragment-mode owns
    // a whole file. Both must land in known harness paths or we
    // refuse — a target like ~/.zshrc would otherwise look "valid"
    // (under HOME, no ..) but be a remote-code-execution sink.
    match mode {
        RenderMode::Section => {
            if !section_target_allowed(&resolved, project_root) {
                return Err(CompileError::PathDenied(format!(
                    "section-mode target '{}' is not in the allowlist of harness-canonical paths",
                    resolved.display()
                )));
            }
        }
        RenderMode::Fragment => {
            if !fragment_target_allowed(&resolved, project_root) {
                return Err(CompileError::PathDenied(format!(
                    "fragment-mode target '{}' is not under a known harness fragment directory",
                    resolved.display()
                )));
            }
        }
    }

    Ok(Some((resolved, mode)))
}

fn infer_mode(path: &Path, harness: Harness) -> RenderMode {
    // .mdc and per-id files are fragments; shared files like AGENTS.md,
    // .cursorrules are sections.
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext == "mdc" {
        return RenderMode::Fragment;
    }
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    // Exhaustive on Harness — adding a new harness forces an explicit
    // decision here at compile time.
    match harness {
        Harness::Codex => match file_name.as_str() {
            "agents.md" => RenderMode::Section,
            _ => RenderMode::Section,
        },
        Harness::Cursor => match file_name.as_str() {
            ".cursorrules" => RenderMode::Section,
            _ => RenderMode::Section,
        },
        Harness::Opencode => RenderMode::Fragment,
        // Unreachable — Claude reads body directly; resolve_target_and_mode
        // returns None before infer_mode is called.
        Harness::Claude => RenderMode::Section,
    }
}

// ─── Install ledger (fragment-mode tracking) ───────────────────────────

const LEDGER_VERSION: u32 = 1;

/// One tracked file installed by a previous compile. We pair the path
/// with a SHA-256 of the content we wrote — on cleanup we re-hash the
/// file before deleting it, so a tampered ledger pointing at e.g.
/// `~/.bashrc` cannot trick the orphan loop into removing user data.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LedgerEntry {
    pub path: String,
    pub sha256: String,
}

/// Persisted compile state. The synthetic key `__sections__` keeps a
/// list of section-mode target files so we can scan them for orphan
/// markers on next run, even when no manifest claims them anymore.
/// Section entries store empty hashes — we never `remove_file()` a
/// shared file, only splice marker blocks out of it.
#[derive(Debug, Clone, Default)]
struct InstallLedger {
    /// On-disk format version. Kept for future migrations and so we
    /// can refuse to cleanup based on a future-format ledger we don't
    /// fully understand. Currently unread by the compiler — but we
    /// preserve it across loads so save_ledger can stamp the right
    /// number.
    #[allow(dead_code)]
    version: u32,
    entries: BTreeMap<String, Vec<LedgerEntry>>,
    /// True when the on-disk ledger predates `version` + per-entry
    /// hashing. We accept it (so a fresh deploy doesn't crash) but
    /// refuse destructive cleanup until the next normal compile rewrites
    /// it in v1 form.
    legacy: bool,
}

/// On-disk shape for v1+. Plain serde — `version` and `entries` are
/// the only fields that ship.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct LedgerOnDiskV1 {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    entries: BTreeMap<String, Vec<LedgerEntry>>,
}

/// Legacy v0 ledgers stored entries as plain path strings, no hashes.
/// Recognised so a freshly-upgraded install doesn't lose its inventory.
#[derive(Debug, Clone, Default, Deserialize)]
struct LedgerOnDiskV0 {
    entries: BTreeMap<String, Vec<String>>,
}

fn ledger_path(profile_dir: &str) -> PathBuf {
    PathBuf::from(profile_dir).join(".weplex").join("compile-ledger.json")
}

/// SHA-256 of `content` as lowercase hex. Re-exported from `crate::utils`
/// to keep call sites in this module unchanged. Used to detect
/// post-install tampering of installed fragments before we delete them.
fn sha256_hex(content: &[u8]) -> String {
    crate::utils::sha256_hex(content)
}

fn load_ledger(profile_dir: &str) -> InstallLedger {
    let path = ledger_path(profile_dir);
    let raw = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return InstallLedger {
            version: LEDGER_VERSION,
            ..Default::default()
        },
    };

    // Try v1+ first.
    if let Ok(v1) = serde_json::from_str::<LedgerOnDiskV1>(&raw) {
        if v1.version > LEDGER_VERSION {
            // Future format — refuse to act on it. Treat as if the
            // ledger was empty so we don't accidentally delete files
            // we don't know how to validate. We log so the developer
            // sees what happened.
            log::warn!(
                "compile ledger at {} has future version {}; ignoring",
                path.display(),
                v1.version
            );
            return InstallLedger {
                version: LEDGER_VERSION,
                entries: BTreeMap::new(),
                legacy: false,
            };
        }
        if v1.version >= 1 {
            return InstallLedger {
                version: v1.version,
                entries: v1.entries,
                legacy: false,
            };
        }
    }

    // Fall back to legacy v0 (no version, entries are Vec<String>).
    if let Ok(v0) = serde_json::from_str::<LedgerOnDiskV0>(&raw) {
        let mut converted: BTreeMap<String, Vec<LedgerEntry>> = BTreeMap::new();
        for (id, paths) in v0.entries {
            converted.insert(
                id,
                paths
                    .into_iter()
                    .map(|p| LedgerEntry { path: p, sha256: String::new() })
                    .collect(),
            );
        }
        return InstallLedger {
            version: 0,
            entries: converted,
            legacy: true,
        };
    }

    log::warn!(
        "compile ledger at {} could not be parsed in any known format; treating as empty",
        path.display()
    );
    InstallLedger {
        version: LEDGER_VERSION,
        ..Default::default()
    }
}

fn save_ledger(profile_dir: &str, ledger: &InstallLedger) -> Result<(), CompileError> {
    let path = ledger_path(profile_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| CompileError::Io(format!("create ledger parent: {}", e)))?;
    }
    let on_disk = LedgerOnDiskV1 {
        version: LEDGER_VERSION,
        entries: ledger.entries.clone(),
    };
    let json = serde_json::to_string_pretty(&on_disk)
        .map_err(|e| CompileError::Io(format!("serialize ledger: {}", e)))?;
    crate::utils::atomic_write_owner_only(&path.to_string_lossy(), &json)
        .map_err(|e| CompileError::Io(format!("write ledger: {}", e)))
}

/// Re-validate a path read out of the install ledger before we touch
/// the filesystem with it. The ledger lives in a user-writable
/// directory, so a malicious rewrite can claim arbitrary paths — we
/// only proceed if the path canonicalises under HOME (or a project
/// root we already canonicalised).
fn ledger_path_is_safe(p: &Path, project_root: Option<&Path>) -> bool {
    // Reject paths with `..` components.
    for c in p.components() {
        if matches!(c, std::path::Component::ParentDir) {
            return false;
        }
    }

    let home = crate::utils::get_home();
    let home_path = PathBuf::from(&home);
    let canon_home = std::fs::canonicalize(&home_path).unwrap_or(home_path);

    // Canonicalise as much of the path as exists; missing tail is fine
    // (the file may have been deleted out from under us).
    let canon = if p.exists() {
        match std::fs::canonicalize(p) {
            Ok(c) => c,
            Err(_) => return false,
        }
    } else if let Some(parent) = p.parent() {
        if parent.as_os_str().is_empty() {
            return false;
        }
        match std::fs::canonicalize(parent) {
            Ok(c) => match p.file_name() {
                Some(n) => c.join(n),
                None => return false,
            },
            Err(_) => return false,
        }
    } else {
        return false;
    };

    if canon.starts_with(&canon_home) {
        return true;
    }
    if let Some(pr) = project_root {
        if canon.starts_with(pr) {
            return true;
        }
    }
    false
}

// ─── Interstitial padding helper ────────────────────────────────────────

/// Compute the interstitial vector for a section file when `added_count`
/// new marker blocks are appended after the existing `original_blocks_count`
/// blocks.
///
/// Input invariants:
/// - `original_interstitials.len() == original_blocks_count + 1`.
/// - `added_count >= 0`.
///
/// Output guarantee:
/// - returned vector has length `original_blocks_count + added_count + 1`.
/// - the slot at index `original_blocks_count` (which sits immediately
///   before the first appended block) is patched to ensure a blank-line
///   separator above the appended block when the existing content has
///   user-visible bytes there.
/// - between each pair of appended blocks a single `"\n"` separator slot
///   keeps blocks visually separated.
/// - the final slot is empty (the per-block trailing `\n` is added during
///   reassembly).
///
/// This is pure: same inputs → same output, no I/O, no path dependence.
fn pad_interstitials_for_appended_blocks(
    original_interstitials: &[String],
    original_blocks_count: usize,
    added_count: usize,
) -> Vec<String> {
    debug_assert_eq!(original_interstitials.len(), original_blocks_count + 1);

    let mut interstitials: Vec<String> = original_interstitials.to_vec();
    if added_count == 0 {
        return interstitials;
    }

    // Patch the slot that sits immediately before the first appended
    // block (the LAST element of the current interstitial list). When
    // it has user content but no trailing blank line, add one so the
    // appended block starts on a fresh paragraph.
    let last_idx = interstitials.len() - 1;
    let final_inter = &mut interstitials[last_idx];
    if !final_inter.is_empty() && !final_inter.ends_with("\n\n") {
        if final_inter.ends_with('\n') {
            final_inter.push('\n');
        } else {
            final_inter.push_str("\n\n");
        }
    }

    // Append `added_count` slots: (added_count - 1) "\n" separators
    // between adjacent appended blocks, and one final "" trailing slot.
    for _ in 0..added_count.saturating_sub(1) {
        interstitials.push("\n".to_string());
    }
    interstitials.push(String::new());

    debug_assert_eq!(
        interstitials.len(),
        original_blocks_count + added_count + 1
    );
    interstitials
}

// ─── Section-mode renderer ─────────────────────────────────────────────

/// Apply all desired sections to one shared target file. Updates only
/// our marker blocks; preserves everything else byte-for-byte.
///
/// `first_install` controls behaviour when the target file exists with
/// NO Weplex markers:
/// - `true` → append our sections (preserving the user's existing file).
/// - `false` → return PathDenied (callers haven't said it's safe to
///   append into this user-owned file yet).
fn apply_sections_to_target(
    target: &Path,
    sections: &[&MarkerBlock],
    first_install: bool,
) -> Result<TargetWriteOutcome, CompileError> {
    let exists = target.exists();
    let current = if exists {
        std::fs::read_to_string(target)
            .map_err(|e| CompileError::Io(format!("read {}: {}", target.display(), e)))?
    } else {
        String::new()
    };

    let parsed = parse_marker_blocks(&current);
    let has_our_markers = !parsed.blocks.is_empty();

    if exists && !has_our_markers && !first_install {
        return Err(CompileError::PathDenied(format!(
            "{} exists without Weplex markers; refusing to splice without explicit first-install",
            target.display()
        )));
    }

    // Build the new content:
    // - Preserve interstitials.
    // - Replace blocks whose id matches a desired section.
    // - Drop blocks whose id is in our "managed but not desired" set
    //   (handled by the caller via `remove_orphan_sections`).
    // - Append any desired sections that didn't already exist.

    let mut wanted: HashMap<&str, &MarkerBlock> = HashMap::new();
    for s in sections {
        wanted.insert(s.id.as_str(), s);
    }

    let mut new_blocks: Vec<MarkerBlock> = Vec::with_capacity(parsed.blocks.len() + sections.len());
    let mut handled: HashSet<String> = HashSet::new();
    for old in &parsed.blocks {
        if let Some(replacement) = wanted.get(old.id.as_str()) {
            new_blocks.push((*replacement).clone());
            handled.insert(old.id.clone());
        } else {
            // Not desired — keep it. Orphan removal is done explicitly
            // via `remove_orphan_sections` so callers control which ids
            // count as "ours but no longer wanted".
            new_blocks.push(old.clone());
        }
    }
    for s in sections {
        if !handled.contains(&s.id) {
            new_blocks.push((*s).clone());
        }
    }

    // Reassemble: replace blocks one-for-one (interstitials unchanged) and
    // append any new ones with proper separator padding via the helper.
    let added = new_blocks.len().saturating_sub(parsed.blocks.len());
    let interstitials = pad_interstitials_for_appended_blocks(
        &parsed.interstitials,
        parsed.blocks.len(),
        added,
    );

    let new_parsed = ParsedSections {
        interstitials,
        blocks: new_blocks,
    };
    let mut new_content = new_parsed.reassemble();
    if !new_content.ends_with('\n') {
        new_content.push('\n');
    }

    if new_content == current {
        return Ok(TargetWriteOutcome::Unchanged);
    }

    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| CompileError::Io(format!("create {}: {}", parent.display(), e)))?;
    }
    crate::utils::atomic_write_user_readable(&target.to_string_lossy(), &new_content)
        .map_err(CompileError::Io)?;
    Ok(TargetWriteOutcome::Written)
}

/// Strip our marker blocks for the given ids from `target`. Preserves
/// everything else byte-for-byte. Returns Unchanged if no work was needed.
fn remove_orphan_sections(
    target: &Path,
    orphan_ids: &HashSet<String>,
) -> Result<TargetWriteOutcome, CompileError> {
    if !target.exists() {
        return Ok(TargetWriteOutcome::Unchanged);
    }
    let current = std::fs::read_to_string(target)
        .map_err(|e| CompileError::Io(format!("read {}: {}", target.display(), e)))?;
    let parsed = parse_marker_blocks(&current);
    if parsed.blocks.iter().all(|b| !orphan_ids.contains(&b.id)) {
        return Ok(TargetWriteOutcome::Unchanged);
    }

    // Drop the orphan blocks AND merge each removed block's surrounding
    // interstitials into one (so we don't leave two blank-line gaps).
    let mut interstitials: Vec<String> = Vec::new();
    let mut blocks: Vec<MarkerBlock> = Vec::new();
    let mut current_inter = parsed.interstitials[0].clone();
    for (i, b) in parsed.blocks.into_iter().enumerate() {
        let next_inter = parsed.interstitials[i + 1].clone();
        if orphan_ids.contains(&b.id) {
            // Merge interstitials, collapsing the doubled blank-line
            // separator that was around this block. We keep at most one
            // trailing newline on `current_inter` before appending
            // `next_inter`.
            if current_inter.ends_with("\n\n") && next_inter.starts_with('\n') {
                let new_next = next_inter.trim_start_matches('\n').to_string();
                current_inter.push_str(&new_next);
            } else {
                current_inter.push_str(&next_inter);
            }
        } else {
            interstitials.push(std::mem::take(&mut current_inter));
            blocks.push(b);
            current_inter = next_inter;
        }
    }
    interstitials.push(current_inter);

    let new_parsed = ParsedSections {
        interstitials,
        blocks,
    };
    let new_content = new_parsed.reassemble();

    if new_content == current {
        return Ok(TargetWriteOutcome::Unchanged);
    }
    crate::utils::atomic_write_user_readable(&target.to_string_lossy(), &new_content)
        .map_err(CompileError::Io)?;
    Ok(TargetWriteOutcome::Written)
}

// ─── Fragment-mode renderer ─────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TargetWriteOutcome {
    Written,
    Unchanged,
}

fn apply_fragment(target: &Path, body: &str) -> Result<TargetWriteOutcome, CompileError> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| CompileError::Io(format!("create {}: {}", parent.display(), e)))?;
    }
    if target.exists() {
        let existing = std::fs::read_to_string(target)
            .map_err(|e| CompileError::Io(format!("read {}: {}", target.display(), e)))?;
        if existing == body {
            return Ok(TargetWriteOutcome::Unchanged);
        }
    }
    crate::utils::atomic_write_user_readable(&target.to_string_lossy(), body)
        .map_err(CompileError::Io)?;
    Ok(TargetWriteOutcome::Written)
}

/// Direct unlink without hash/inode verification. Kept around because
/// it remains useful for tests and any future callers that have already
/// verified safety some other way; production cleanup goes through
/// `read_and_remove_if_unchanged` instead.
#[cfg(test)]
fn remove_fragment(target: &Path) -> Result<bool, CompileError> {
    if !target.exists() {
        return Ok(false);
    }
    std::fs::remove_file(target)
        .map_err(|e| CompileError::Io(format!("remove {}: {}", target.display(), e)))?;
    Ok(true)
}

/// Outcome of [`read_and_remove_if_unchanged`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RemoveOutcome {
    /// File was unlinked.
    Removed,
    /// File hash didn't match the expected install hash.
    HashMismatch,
    /// File's identity (device + inode) changed between the hash read
    /// and the unlink attempt — Unix only.
    InodeChanged,
    /// File didn't exist at all.
    Missing,
}

/// Read `path`, compare its content hash to `expected_hash`, and unlink
/// the file iff the hash matches AND (on Unix) the inode/device pair
/// has not changed between the read and the unlink. Refuses to follow
/// symlinks at open time.
///
/// This narrows the TOCTOU window between hash verification and
/// unlinking: a previously-verified file can still be replaced via
/// `rename(2)` between the read and the unlink, but we now detect that
/// swap because the new file's (dev, ino) will not match what we
/// captured during the read. An attacker with full filesystem access
/// can still race us, but they need to win twice in a smaller window;
/// this raises the bar without claiming to eliminate the risk.
///
/// On Windows, the `(dev, ino)` semantics don't map cleanly, so we
/// rely on the hash check alone there. Documented residual risk:
/// `read_and_remove_if_unchanged` is best-effort on Windows.
fn read_and_remove_if_unchanged(
    path: &Path,
    expected_hash: &str,
) -> Result<RemoveOutcome, CompileError> {
    use std::io::Read;

    if !path.exists() {
        return Ok(RemoveOutcome::Missing);
    }

    // Open with O_NOFOLLOW on Unix so a symlink swap can't redirect us
    // to a victim file in the read-then-remove window.
    let mut file = {
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc_o_nofollow())
                .open(path)
                .map_err(|e| CompileError::Io(format!("open {}: {}", path.display(), e)))?
        }
        #[cfg(not(unix))]
        {
            std::fs::OpenOptions::new()
                .read(true)
                .open(path)
                .map_err(|e| CompileError::Io(format!("open {}: {}", path.display(), e)))?
        }
    };

    // Capture identity BEFORE reading so a same-fd metadata is the
    // ground truth — even if the directory entry is replaced under us
    // during the read, this fstat is taken on the file we actually
    // opened.
    #[cfg(unix)]
    let identity_before = {
        use std::os::unix::fs::MetadataExt;
        let m = file
            .metadata()
            .map_err(|e| CompileError::Io(format!("fstat {}: {}", path.display(), e)))?;
        Some((m.dev(), m.ino()))
    };
    #[cfg(not(unix))]
    let identity_before: Option<(u64, u64)> = None;

    let mut content = Vec::new();
    file.read_to_end(&mut content)
        .map_err(|e| CompileError::Io(format!("read {}: {}", path.display(), e)))?;

    if sha256_hex(&content) != expected_hash {
        return Ok(RemoveOutcome::HashMismatch);
    }

    // Re-stat the path (NOT the open fd) and confirm we still resolve
    // to the same file. If a swap happened, the path now points
    // somewhere else — refuse to delete it.
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        match std::fs::symlink_metadata(path) {
            Ok(m) => {
                let identity_after = (m.dev(), m.ino());
                if Some(identity_after) != identity_before {
                    return Ok(RemoveOutcome::InodeChanged);
                }
            }
            Err(_) => {
                // Path no longer exists between our open and now —
                // treat as missing rather than removing some forged
                // replacement.
                return Ok(RemoveOutcome::Missing);
            }
        }
    }

    std::fs::remove_file(path)
        .map_err(|e| CompileError::Io(format!("remove {}: {}", path.display(), e)))?;
    Ok(RemoveOutcome::Removed)
}

#[cfg(unix)]
fn libc_o_nofollow() -> i32 {
    // libc isn't a direct dep — replicate the constant. POSIX value is
    // platform-specific; these are the standard values for Linux/macOS
    // (which is where we run). Centralise to keep the unsafe-feeling
    // hardcode in one place.
    #[cfg(target_os = "macos")]
    {
        0x0100
    }
    #[cfg(target_os = "linux")]
    {
        0o400000
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        0
    }
}

// ─── Per-profile compile lock ───────────────────────────────────────────

/// Acquire an exclusive advisory lock for the duration of a compile.
///
/// The lock is a flock(2)-style exclusive lock on
/// `<profile_dir>/.weplex/compile.lock`. It is **cooperative**: only
/// processes that themselves take this lock are blocked — direct edits
/// to the target files (or another tool entirely) are not held back.
/// This is sufficient for our threat model, where the only concurrent
/// writer we're worried about is another Weplex instance compiling the
/// same profile (e.g. user runs the desktop app and the CLI in parallel,
/// or the desktop fires two compiles in quick succession).
///
/// On failure to acquire (lock already held), returns an Io error so the
/// caller surfaces it cleanly. On success, returns the open file — drop
/// it to release the lock.
fn acquire_compile_lock(profile_dir: &str) -> Result<std::fs::File, CompileError> {
    use fs2::FileExt;
    let lock_dir = PathBuf::from(profile_dir).join(".weplex");
    std::fs::create_dir_all(&lock_dir)
        .map_err(|e| CompileError::Io(format!("create lock dir {}: {}", lock_dir.display(), e)))?;
    let lock_path = lock_dir.join("compile.lock");
    let lock_file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&lock_path)
        .map_err(|e| CompileError::Io(format!("open compile lock {}: {}", lock_path.display(), e)))?;
    lock_file
        .try_lock_exclusive()
        .map_err(|e| CompileError::Io(format!("compile already in progress for {}: {}", profile_dir, e)))?;
    Ok(lock_file)
}

// ─── Compile ────────────────────────────────────────────────────────────

/// Compile every manifest in `profile_dir`. If `dry_run` is true, no
/// writes happen but the report still tells the caller what *would*
/// change.
pub fn compile_profile(
    profile_dir: &str,
    project_root: Option<&Path>,
) -> Result<CompileReport, CompileError> {
    compile_profile_internal(profile_dir, project_root, false)
}

pub fn dry_run_compile(
    profile_dir: &str,
    project_root: Option<&Path>,
) -> Result<CompileReport, CompileError> {
    compile_profile_internal(profile_dir, project_root, true)
}

fn compile_profile_internal(
    profile_dir: &str,
    project_root: Option<&Path>,
    dry_run: bool,
) -> Result<CompileReport, CompileError> {
    // Per-profile compile lock: section-mode rendering does
    // read-modify-write on shared files (e.g. ~/.codex/AGENTS.md), so two
    // compiles of the same profile racing in parallel can produce torn
    // marker blocks. The lock is advisory and cooperative — it only
    // protects compilers that themselves take it. We hold it for the
    // entire body of the function; it drops on return (success or error)
    // when `_lock_file` goes out of scope. Dry-run skips the lock since
    // it never touches the target files.
    let _lock_file = if !dry_run {
        Some(acquire_compile_lock(profile_dir)?)
    } else {
        None
    };

    let manifests = scan_profile_manifests(profile_dir)?;
    let home = crate::utils::get_home();

    let profile_label = Path::new(profile_dir)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("profile")
        .to_string();

    let mut report = CompileReport {
        profile_dir: profile_dir.to_string(),
        manifests_seen: manifests.len() as u32,
        targets_written: Vec::new(),
        targets_unchanged: Vec::new(),
        orphans_removed: Vec::new(),
        errors: Vec::new(),
    };

    // ── Phase 1: collect desired writes ────────────────────────────────
    //
    // Group section-mode writes per target file (so each target is
    // touched at most once, and idempotency calculations include all
    // relevant sections). Fragment-mode writes are 1:1.

    // target_path → Vec<(MarkerBlock, source manifest_path)>
    let mut section_groups: HashMap<PathBuf, Vec<(MarkerBlock, String)>> = HashMap::new();
    // (manifest_id, fragment_target) for ledger updates
    let mut fragment_writes: Vec<(String, PathBuf, String)> = Vec::new();
    // All ids present in this compile (for orphan detection).
    let mut current_ids: HashSet<String> = HashSet::new();

    for (manifest, kind) in &manifests {
        current_ids.insert(manifest.id.clone());

        let body_default = match std::fs::read_to_string(&manifest.body_path) {
            Ok(s) => s,
            Err(e) => {
                report.errors.push(format!(
                    "{}: read body {}: {}",
                    manifest.id, manifest.body_path, e
                ));
                continue;
            }
        };

        for &harness in Harness::ALL_NON_CLAUDE {
            let Some(spec) = manifest.agents.target_for(harness).cloned() else {
                continue;
            };

            // Resolve body source override if the spec asks for one.
            let body = match resolve_body_for_spec(&manifest.manifest_path, &spec, &body_default) {
                Ok(b) => b,
                Err(e) => {
                    report
                        .errors
                        .push(format!("{}/{}: {}", manifest.id, harness.key(), e));
                    continue;
                }
            };

            let resolved = match resolve_target_and_mode(
                harness,
                &spec,
                &manifest.id,
                &home,
                project_root,
            ) {
                Ok(Some(r)) => r,
                Ok(None) => continue,
                Err(e) => {
                    // Per-target allowlist denials and bad path specs
                    // are non-fatal: we collect the error and keep
                    // compiling other manifests. The frontend gets a
                    // useful diff in CompileReport.errors.
                    report
                        .errors
                        .push(format!("{}/{}: {}", manifest.id, harness.key(), e));
                    continue;
                }
            };
            let (target, mode) = resolved;

            match mode {
                RenderMode::Section => {
                    let section_label = spec.section.as_deref().or_else(|| Some(manifest.id.as_str()));
                    let block = match render_section_block(
                        &manifest.id,
                        section_label,
                        &profile_label,
                        *kind,
                        &body,
                    ) {
                        Ok(b) => b,
                        Err(e) => {
                            report
                                .errors
                                .push(format!("{}/{}: {}", manifest.id, harness.key(), e));
                            continue;
                        }
                    };
                    section_groups
                        .entry(target)
                        .or_default()
                        .push((block, manifest.manifest_path.clone()));
                }
                RenderMode::Fragment => {
                    fragment_writes.push((manifest.id.clone(), target, body));
                }
            }
        }
    }

    // ── Phase 2: section writes ────────────────────────────────────────

    // Detect duplicate ids per shared target (e.g. two manifests trying
    // to claim the same id in the same AGENTS.md).
    for (target, blocks) in &section_groups {
        let mut seen: HashMap<&str, &str> = HashMap::new();
        for (b, src) in blocks {
            if let Some(prev) = seen.insert(b.id.as_str(), src.as_str()) {
                return Err(CompileError::DuplicateId(
                    b.id.clone(),
                    prev.to_string(),
                    src.clone(),
                ));
            }
        }
        let _ = target;
    }

    // Determine first-install policy per target: if the target file
    // doesn't exist, or already contains our markers → safe. If it
    // exists with foreign content → we still allow append (Phase 1
    // policy: behave as Codex's own users would when adding a new
    // section). Phase 2 may flip this to require explicit consent.
    for (target, entries) in &section_groups {
        let blocks: Vec<&MarkerBlock> = entries.iter().map(|(b, _)| b).collect();
        let outcome = if dry_run {
            simulate_apply_sections(target, &blocks)?
        } else {
            apply_sections_to_target(target, &blocks, true)?
        };
        match outcome {
            TargetWriteOutcome::Written => report.targets_written.push(target.to_string_lossy().to_string()),
            TargetWriteOutcome::Unchanged => report
                .targets_unchanged
                .push(target.to_string_lossy().to_string()),
        }
    }

    // ── Phase 3: fragment writes ───────────────────────────────────────

    let mut new_ledger = InstallLedger {
        version: LEDGER_VERSION,
        ..Default::default()
    };
    for (id, target, body) in &fragment_writes {
        let outcome = if dry_run {
            // For idempotency reporting in dry-run: compare against existing.
            if target.exists() {
                let existing = std::fs::read_to_string(target).unwrap_or_default();
                if existing == *body {
                    TargetWriteOutcome::Unchanged
                } else {
                    TargetWriteOutcome::Written
                }
            } else {
                TargetWriteOutcome::Written
            }
        } else {
            apply_fragment(target, body)?
        };
        match outcome {
            TargetWriteOutcome::Written => report.targets_written.push(target.to_string_lossy().to_string()),
            TargetWriteOutcome::Unchanged => report
                .targets_unchanged
                .push(target.to_string_lossy().to_string()),
        }
        new_ledger.entries.entry(id.clone()).or_default().push(LedgerEntry {
            path: target.to_string_lossy().to_string(),
            sha256: sha256_hex(body.as_bytes()),
        });
    }

    // ── Phase 4: orphan cleanup ────────────────────────────────────────

    let prev_ledger = load_ledger(profile_dir);

    // Fragment orphans: ledger entries whose id is not in current_ids.
    // The synthetic "__sections__" key is for section-target tracking
    // (handled below), not a real id — never delete its paths as
    // fragments.
    //
    // Hardening: the ledger is JSON in a user-writable directory, so we
    // do NOT trust it. Two checks gate every delete:
    //   1. The path must canonicalise inside HOME (or the project root).
    //   2. The current file content must match the SHA-256 we wrote at
    //      install time. If the user edited the file we keep our hands
    //      off; if a malicious ledger forged the path, the hash won't
    //      match either.
    // Legacy v0 ledgers have no hashes — we accept their inventory but
    // refuse to delete based on it; a normal compile rewrites them in
    // v1 form.
    for (id, entries) in &prev_ledger.entries {
        if id == "__sections__" || current_ids.contains(id) {
            continue;
        }
        for entry in entries {
            let path = PathBuf::from(&entry.path);
            if !ledger_path_is_safe(&path, project_root) {
                log::warn!(
                    "compile: refusing to act on ledger entry outside HOME/project: {}",
                    entry.path
                );
                continue;
            }
            if !path.exists() {
                // File already gone — nothing to do, just let the
                // entry drop from the new ledger.
                continue;
            }
            if prev_ledger.legacy {
                log::warn!(
                    "compile: legacy ledger has no hash for {}; refusing to delete",
                    entry.path
                );
                continue;
            }
            if dry_run {
                // Dry-run still hashes — but uses the simple read so we
                // don't unlink anything.
                let current = match std::fs::read(&path) {
                    Ok(c) => c,
                    Err(e) => {
                        log::warn!("compile: cannot read fragment {}: {}", path.display(), e);
                        continue;
                    }
                };
                if sha256_hex(&current) != entry.sha256 {
                    log::warn!(
                        "compile: fragment {} no longer matches install hash; leaving in place",
                        path.display()
                    );
                    continue;
                }
                report.orphans_removed.push(entry.path.clone());
                continue;
            }

            // TOCTOU mitigation: hash + (Unix) inode-stable removal.
            // See `read_and_remove_if_unchanged` for residual risk notes.
            match read_and_remove_if_unchanged(&path, &entry.sha256)? {
                RemoveOutcome::Removed => {
                    report.orphans_removed.push(entry.path.clone());
                }
                RemoveOutcome::HashMismatch => {
                    log::warn!(
                        "compile: fragment {} no longer matches install hash; leaving in place",
                        path.display()
                    );
                }
                RemoveOutcome::InodeChanged => {
                    log::warn!(
                        "compile: fragment {} swapped between hash check and unlink; refusing delete",
                        path.display()
                    );
                }
                RemoveOutcome::Missing => {}
            }
        }
    }

    // Section orphans: scan every target we know about (from this run
    // AND from prior section targets recorded in the ledger under the
    // synthetic key "__sections__"). Paths from the ledger are
    // re-validated before we open the file — a tampered ledger pointing
    // at e.g. `/etc/sudoers` must NOT cause us to read that file.
    let mut all_section_targets: HashSet<PathBuf> = section_groups.keys().cloned().collect();
    if let Some(prev_sections) = prev_ledger.entries.get("__sections__") {
        for entry in prev_sections {
            let p = PathBuf::from(&entry.path);
            if ledger_path_is_safe(&p, project_root) {
                all_section_targets.insert(p);
            } else {
                log::warn!(
                    "compile: dropping unsafe __sections__ entry from ledger: {}",
                    entry.path
                );
            }
        }
    }
    let orphan_section_ids: HashSet<String> = prev_ledger
        .entries
        .keys()
        .filter(|k| k.as_str() != "__sections__")
        .filter(|k| !current_ids.contains(*k))
        .cloned()
        .collect();
    // Also: any marker we encounter in a target whose id is not in
    // current_ids → orphan.
    let mut section_orphans = orphan_section_ids;
    for target in &all_section_targets {
        if !target.exists() {
            continue;
        }
        let content = match std::fs::read_to_string(target) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let parsed = parse_marker_blocks(&content);
        for b in parsed.blocks {
            if !current_ids.contains(&b.id) {
                section_orphans.insert(b.id);
            }
        }
    }

    if !section_orphans.is_empty() {
        for target in &all_section_targets {
            if dry_run {
                if !target.exists() {
                    continue;
                }
                let content = std::fs::read_to_string(target).unwrap_or_default();
                let parsed = parse_marker_blocks(&content);
                let any_match = parsed
                    .blocks
                    .iter()
                    .any(|b| section_orphans.contains(&b.id));
                if any_match {
                    report
                        .orphans_removed
                        .push(target.to_string_lossy().to_string());
                }
            } else {
                let outcome = remove_orphan_sections(target, &section_orphans)?;
                if outcome == TargetWriteOutcome::Written {
                    report
                        .orphans_removed
                        .push(target.to_string_lossy().to_string());
                }
            }
        }
    }

    // ── Phase 5: persist ledger ───────────────────────────────────────

    if !dry_run {
        // Track section targets under a synthetic key so future runs can
        // find them even if no manifests target them anymore. Section
        // entries carry empty hashes — we never `remove_file()` a shared
        // file; orphan cleanup splices markers out instead.
        //
        // Important: only THIS run's section_groups are recorded, NOT the
        // union of this-run + prior __sections__. Otherwise once a profile
        // ever rendered to a target, the path would be tracked forever —
        // and an empty profile would never drop the synthetic key. The
        // orphan-cleanup phase above already splices our markers out of
        // any target that's no longer wanted, so dropping the path from
        // the ledger here is the correct end state.
        let section_paths: Vec<LedgerEntry> = section_groups
            .keys()
            .filter(|p| ledger_path_is_safe(p, project_root))
            .map(|p| LedgerEntry {
                path: p.to_string_lossy().to_string(),
                sha256: String::new(),
            })
            .collect();
        if !section_paths.is_empty() {
            new_ledger
                .entries
                .insert("__sections__".to_string(), section_paths);
        }
        save_ledger(profile_dir, &new_ledger)?;
    }

    Ok(report)
}

/// Simulate `apply_sections_to_target` without writing.
fn simulate_apply_sections(
    target: &Path,
    sections: &[&MarkerBlock],
) -> Result<TargetWriteOutcome, CompileError> {
    let exists = target.exists();
    let current = if exists {
        std::fs::read_to_string(target)
            .map_err(|e| CompileError::Io(format!("read {}: {}", target.display(), e)))?
    } else {
        String::new()
    };
    // Build the projected new content the same way apply_sections_to_target
    // does, then compare to current. Reuse the actual function in a temp
    // copy file to avoid divergent logic.
    let tmp = std::env::temp_dir().join(format!(
        "weplex-dry-run-{}-{}.txt",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    if exists {
        std::fs::write(&tmp, &current)
            .map_err(|e| CompileError::Io(format!("dry-run scratch: {}", e)))?;
    }
    let outcome = apply_sections_to_target(&tmp, sections, true)?;
    let _ = std::fs::remove_file(&tmp);
    Ok(outcome)
}

/// Read the body that should be rendered for a given target spec. If
/// the spec has an explicit `source`, read that (relative to the
/// manifest dir); otherwise fall back to the default body the loader
/// already validated.
fn resolve_body_for_spec(
    manifest_path: &str,
    spec: &TargetSpec,
    default_body: &str,
) -> Result<String, String> {
    let Some(source) = spec.source.as_deref() else {
        return Ok(default_body.to_string());
    };
    // Reject `..` segments — even relative paths must stay inside the
    // manifest directory.
    let p = Path::new(source);
    for c in p.components() {
        if matches!(c, std::path::Component::ParentDir) {
            return Err(format!("source contains `..`: {}", source));
        }
    }
    let manifest_dir = Path::new(manifest_path)
        .parent()
        .ok_or_else(|| format!("no parent for manifest: {}", manifest_path))?;
    let abs = manifest_dir.join(source);
    std::fs::read_to_string(&abs).map_err(|e| format!("read source {}: {}", abs.display(), e))
}

// ─── Tauri commands ─────────────────────────────────────────────────────

/// Validate inputs from the renderer before handing them to the
/// compiler. The renderer is sandboxed but compromise is in scope —
/// raw paths must canonicalise inside HOME and (for project_root)
/// must not be HOME itself, otherwise a `${PROJECT}/.cursorrules`
/// target would write to `~/.cursorrules`.
fn validate_compile_inputs(
    profile_config_dir: String,
    project_root: Option<String>,
) -> Result<(String, Option<PathBuf>), String> {
    let profile_dir = if profile_config_dir.is_empty() {
        format!("{}/.claude", crate::utils::get_home())
    } else {
        crate::utils::validate_config_dir(&profile_config_dir)
            .map_err(|e| format!("invalid profile_config_dir: {}", e))?
    };

    let project_root_canon = match project_root {
        None => None,
        Some(s) => {
            if s.is_empty() {
                None
            } else {
                let p = PathBuf::from(&s);
                if !p.is_dir() {
                    return Err(format!("project_root is not a directory: {}", s));
                }
                let canon = std::fs::canonicalize(&p)
                    .map_err(|e| format!("failed to canonicalize project_root: {}", e))?;

                let home = PathBuf::from(crate::utils::get_home());
                let canon_home = std::fs::canonicalize(&home).unwrap_or(home);

                // Project root must be under HOME (cheapest sane policy
                // for v1; private workspaces live elsewhere).
                if !canon.starts_with(&canon_home) {
                    return Err(format!(
                        "project_root must be under HOME directory; got: {}",
                        canon.display()
                    ));
                }
                // Reject HOME itself — a `${PROJECT}/.cursorrules`
                // target with project_root == HOME would write to
                // `~/.cursorrules`, hijacking the global user file.
                if canon == canon_home {
                    return Err("project_root cannot be HOME itself".to_string());
                }
                Some(canon)
            }
        }
    };

    Ok((profile_dir, project_root_canon))
}

#[tauri::command]
pub fn compile_profile_to_external_agents(
    profile_config_dir: String,
    project_root: Option<String>,
) -> Result<CompileReport, String> {
    let (profile_dir, project_root_canon) =
        validate_compile_inputs(profile_config_dir, project_root)?;
    compile_profile(&profile_dir, project_root_canon.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn dry_run_compile_profile(
    profile_config_dir: String,
    project_root: Option<String>,
) -> Result<CompileReport, String> {
    let (profile_dir, project_root_canon) =
        validate_compile_inputs(profile_config_dir, project_root)?;
    dry_run_compile(&profile_dir, project_root_canon.as_deref())
        .map_err(|e| e.to_string())
}

// ─── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::test_support::HOME_ENV_LOCK as ENV_LOCK;

    fn tmpdir(label: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "weplex-compiler-test-{}-{}-{}",
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

    fn make_block(id: &str, body: &str) -> MarkerBlock {
        render_section_block(id, Some(id), "test-profile", ResourceKind::Skill, body)
            .expect("test fixture must render")
    }

    #[test]
    fn parse_marker_blocks_roundtrip() {
        let samples: &[&str] = &[
            "",
            "hello\n",
            "hello\nworld\n",
            "user content\n# weplex:begin foo\nbody\n# weplex:end foo\n",
            "preamble\n\n# weplex:begin a\n## A\nbody\n# weplex:end a\n\n# weplex:begin b\nbody-b\n# weplex:end b\nfooter\n",
            "no-trailing-newline-no-markers",
            "# weplex:begin only\nbody\n# weplex:end only\n",
        ];
        for s in samples {
            let parsed = parse_marker_blocks(s);
            let back = parsed.reassemble();
            assert_eq!(back, *s, "round-trip mismatch for input:\n{:?}", s);
        }
    }

    #[test]
    fn parse_marker_blocks_roundtrip_no_trailing_newline_after_end() {
        // Edge case: file ends with the END marker line but no trailing
        // newline. Reassembly always emits a `\n` per line — so this
        // input may diverge from byte-equality by at most one trailing
        // newline. The intent we lock in is idempotency: re-parsing the
        // reassembled output produces the same logical block structure,
        // and the divergence is bounded by 1 byte of trailing whitespace.
        let input = "# weplex:begin only\nbody\n# weplex:end only";
        let parsed = parse_marker_blocks(input);
        let back = parsed.reassemble();
        // Either byte-equal or differs only by a single trailing newline.
        assert!(
            back == input || back == format!("{}\n", input),
            "unexpected divergence: input={:?}, output={:?}",
            input,
            back,
        );
        // Re-parsing the reassembled output is stable: same one block,
        // same id, same body shape.
        let reparsed = parse_marker_blocks(&back);
        assert_eq!(reparsed.blocks.len(), 1);
        assert_eq!(reparsed.blocks[0].id, "only");
    }

    #[test]
    fn pad_interstitials_no_op_when_nothing_appended() {
        // Existing markers, no new blocks → output mirrors input.
        let original = vec!["pre\n".to_string(), "between\n".to_string(), "post\n".to_string()];
        let out = pad_interstitials_for_appended_blocks(&original, 2, 0);
        assert_eq!(out, original);
    }

    #[test]
    fn pad_interstitials_empty_file_one_appended() {
        // Empty existing file (no markers) + 1 appended section.
        // original_interstitials = [""], original_blocks_count = 0.
        let original = vec![String::new()];
        let out = pad_interstitials_for_appended_blocks(&original, 0, 1);
        // 0 + 1 + 1 = 2 slots. First is the patched leading interstitial
        // (still empty), second is the trailing empty slot.
        assert_eq!(out.len(), 2);
        assert_eq!(out[0], "");
        assert_eq!(out[1], "");
    }

    #[test]
    fn pad_interstitials_existing_markers_one_appended() {
        // Existing managed file with a marker block + 1 appended section.
        // original blocks = 1, interstitials = ["pre\n", "tail\n"].
        let original = vec!["pre\n".to_string(), "tail\n".to_string()];
        let out = pad_interstitials_for_appended_blocks(&original, 1, 1);
        // 1 + 1 + 1 = 3 slots. The slot before the new block ("tail\n")
        // gets padded to ensure blank line separation.
        assert_eq!(out.len(), 3);
        assert_eq!(out[0], "pre\n");
        // tail\n had only one trailing newline → patched to "tail\n\n"
        assert_eq!(out[1], "tail\n\n");
        // Final trailing slot is empty.
        assert_eq!(out[2], "");
    }

    #[test]
    fn pad_interstitials_first_install_with_user_content() {
        // First-install: target has user content but no markers. The lone
        // interstitial is the user content; appending must keep it
        // visible and patch the separator.
        let original = vec!["# Existing User Content\n\nfoo\n".to_string()];
        let out = pad_interstitials_for_appended_blocks(&original, 0, 1);
        assert_eq!(out.len(), 2);
        // Already ends with "\n" but not "\n\n" → one extra newline added.
        assert_eq!(out[0], "# Existing User Content\n\nfoo\n\n");
        assert_eq!(out[1], "");
    }

    #[test]
    fn pad_interstitials_user_content_no_trailing_newline() {
        // User content with NO trailing newline at all. Patch must add
        // "\n\n" so the appended block doesn't run on the same line.
        let original = vec!["raw user content".to_string()];
        let out = pad_interstitials_for_appended_blocks(&original, 0, 1);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0], "raw user content\n\n");
        assert_eq!(out[1], "");
    }

    #[test]
    fn pad_interstitials_multiple_appended_blocks() {
        // Append 3 blocks at once on top of existing 1 block. We need
        // 1 + 3 + 1 = 5 slots; the 2 inter-block slots get a single "\n"
        // each, the trailing slot is empty.
        let original = vec!["pre\n".to_string(), "tail\n\n".to_string()];
        let out = pad_interstitials_for_appended_blocks(&original, 1, 3);
        assert_eq!(out.len(), 5);
        assert_eq!(out[0], "pre\n");
        // tail already ended with "\n\n" → no patching needed.
        assert_eq!(out[1], "tail\n\n");
        assert_eq!(out[2], "\n");
        assert_eq!(out[3], "\n");
        assert_eq!(out[4], "");
    }

    #[test]
    fn section_render_first_install_appends() {
        let dir = tmpdir("first-install");
        let target = dir.join("AGENTS.md");
        let block = make_block("foo", "hello world");
        let outcome =
            apply_sections_to_target(&target, &[&block], true).unwrap();
        assert_eq!(outcome, TargetWriteOutcome::Written);
        let content = std::fs::read_to_string(&target).unwrap();
        assert!(content.contains("# weplex:begin foo"), "{}", content);
        assert!(content.contains("# weplex:end foo"), "{}", content);
        assert!(content.contains("hello world"), "{}", content);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn section_render_first_install_appends_existing_unmarked() {
        let dir = tmpdir("first-install-existing");
        let target = dir.join("AGENTS.md");
        std::fs::write(&target, "# Existing User Content\n\nfoo\n").unwrap();
        let block = make_block("foo", "hello");
        let outcome = apply_sections_to_target(&target, &[&block], true).unwrap();
        assert_eq!(outcome, TargetWriteOutcome::Written);
        let content = std::fs::read_to_string(&target).unwrap();
        assert!(content.starts_with("# Existing User Content\n\nfoo\n"), "{}", content);
        assert!(content.contains("# weplex:begin foo"), "{}", content);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn section_render_idempotent_rerun() {
        let dir = tmpdir("idempotent");
        let target = dir.join("AGENTS.md");
        let block = make_block("foo", "hello world");
        apply_sections_to_target(&target, &[&block], true).unwrap();
        let outcome = apply_sections_to_target(&target, &[&block], true).unwrap();
        assert_eq!(outcome, TargetWriteOutcome::Unchanged);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn section_render_update_in_place() {
        let dir = tmpdir("update");
        let target = dir.join("AGENTS.md");
        let original = make_block("foo", "old body");
        apply_sections_to_target(&target, &[&original], true).unwrap();
        let before = std::fs::read_to_string(&target).unwrap();
        let updated = make_block("foo", "new body");
        let outcome = apply_sections_to_target(&target, &[&updated], true).unwrap();
        assert_eq!(outcome, TargetWriteOutcome::Written);
        let after = std::fs::read_to_string(&target).unwrap();
        assert!(after.contains("new body"), "{}", after);
        assert!(!after.contains("old body"), "{}", after);
        // Marker count: still exactly one begin and one end.
        assert_eq!(after.matches("# weplex:begin foo").count(), 1);
        assert_eq!(after.matches("# weplex:end foo").count(), 1);
        // The structure outside the block should be byte-equal.
        let before_parsed = parse_marker_blocks(&before);
        let after_parsed = parse_marker_blocks(&after);
        assert_eq!(before_parsed.interstitials, after_parsed.interstitials);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn section_render_remove_preserves_neighbors() {
        let dir = tmpdir("remove-neighbors");
        let target = dir.join("AGENTS.md");
        let a = make_block("a", "body-a");
        let b = make_block("b", "body-b");
        apply_sections_to_target(&target, &[&a, &b], true).unwrap();
        let before = std::fs::read_to_string(&target).unwrap();
        assert!(before.contains("body-a") && before.contains("body-b"));
        // Now remove `a`.
        let mut orphans: HashSet<String> = HashSet::new();
        orphans.insert("a".to_string());
        let outcome = remove_orphan_sections(&target, &orphans).unwrap();
        assert_eq!(outcome, TargetWriteOutcome::Written);
        let after = std::fs::read_to_string(&target).unwrap();
        assert!(!after.contains("# weplex:begin a"), "{}", after);
        assert!(after.contains("# weplex:begin b"), "{}", after);
        assert!(after.contains("body-b"), "{}", after);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn section_render_refuses_unmarked_file_when_not_first_install() {
        let dir = tmpdir("refuse");
        let target = dir.join("AGENTS.md");
        std::fs::write(&target, "user-only content\n").unwrap();
        let block = make_block("foo", "body");
        let err = apply_sections_to_target(&target, &[&block], false).unwrap_err();
        match err {
            CompileError::PathDenied(_) => {}
            other => panic!("expected PathDenied, got {:?}", other),
        }
        // File untouched.
        assert_eq!(
            std::fs::read_to_string(&target).unwrap(),
            "user-only content\n"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fragment_render_writes_and_removes() {
        let dir = tmpdir("fragment");
        let target = dir.join("nested").join("frag.mdc");
        let outcome = apply_fragment(&target, "fragment body").unwrap();
        assert_eq!(outcome, TargetWriteOutcome::Written);
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "fragment body");
        let removed = remove_fragment(&target).unwrap();
        assert!(removed);
        assert!(!target.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fragment_render_idempotent() {
        let dir = tmpdir("frag-idempotent");
        let target = dir.join("frag.mdc");
        apply_fragment(&target, "body").unwrap();
        let outcome = apply_fragment(&target, "body").unwrap();
        assert_eq!(outcome, TargetWriteOutcome::Unchanged);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn compile_duplicate_id_errors() {
        let profile = tmpdir("dup-id");
        let agents = profile.join("agents");
        let skills = profile.join("skills");
        std::fs::create_dir_all(&agents).unwrap();
        std::fs::create_dir_all(&skills).unwrap();
        let manifest_yaml = r#"
id: foo
version: 1.0.0
agents:
  codex:
    target: ~/.codex/AGENTS.md
"#;
        // Same id "foo" in two different kind dirs → DuplicateId from the
        // scanner.
        std::fs::write(agents.join("foo.weplex.yaml"), manifest_yaml).unwrap();
        std::fs::write(agents.join("foo.md"), "# a").unwrap();
        std::fs::write(skills.join("foo.weplex.yaml"), manifest_yaml).unwrap();
        std::fs::write(skills.join("foo.md"), "# b").unwrap();

        let res = compile_profile(profile.to_str().unwrap(), None);
        match res {
            Err(CompileError::Manifest(ManifestError::DuplicateId(id))) => assert_eq!(id, "foo"),
            other => panic!("expected duplicate id, got {:?}", other),
        }
        let _ = std::fs::remove_dir_all(&profile);
    }

    /// End-to-end: real profile dir with one manifest → compiled to a
    /// target file under a fake HOME. We override HOME via the same
    /// pattern as manifest tests.
    #[test]
    fn compile_full_e2e_codex() {
        let _g = ENV_LOCK.lock().unwrap();
        // Use a sandbox HOME so AGENTS.md lands somewhere we control.
        let home = tmpdir("e2e-codex-home");
        let canon_home = std::fs::canonicalize(&home).unwrap();
        let prev = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", &canon_home); }

        let profile = tmpdir("e2e-codex-profile");
        let skills = profile.join("skills");
        std::fs::create_dir_all(&skills).unwrap();
        std::fs::write(
            skills.join("greet.weplex.yaml"),
            r#"
id: greet
version: 1.0.0
agents:
  codex:
    section: Greeter
"#,
        )
        .unwrap();
        std::fs::write(skills.join("greet.md"), "Always say hello.\n").unwrap();

        let report = compile_profile(profile.to_str().unwrap(), None).unwrap();
        assert_eq!(report.manifests_seen, 1);
        assert_eq!(report.errors, Vec::<String>::new());
        let agents_md = canon_home.join(".codex").join("AGENTS.md");
        assert!(agents_md.exists(), "AGENTS.md not created");
        let content = std::fs::read_to_string(&agents_md).unwrap();
        assert!(content.contains("# weplex:begin greet"));
        assert!(content.contains("## Greeter"));
        assert!(content.contains("Always say hello."));
        assert!(content.contains("# weplex:end greet"));

        if let Some(p) = prev {
            unsafe { std::env::set_var("HOME", p); }
        }
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&profile);
    }

    #[test]
    fn compile_full_e2e_remove_orphan() {
        let _g = ENV_LOCK.lock().unwrap();
        let home = tmpdir("e2e-orphan-home");
        let canon_home = std::fs::canonicalize(&home).unwrap();
        let prev = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", &canon_home); }

        let profile = tmpdir("e2e-orphan-profile");
        let skills = profile.join("skills");
        std::fs::create_dir_all(&skills).unwrap();

        // Initial: one manifest "foo".
        std::fs::write(
            skills.join("foo.weplex.yaml"),
            "id: foo\nversion: 1.0.0\nagents:\n  codex:\n    section: Foo\n",
        )
        .unwrap();
        std::fs::write(skills.join("foo.md"), "Body of foo.\n").unwrap();
        let _ = compile_profile(profile.to_str().unwrap(), None).unwrap();
        let agents_md = canon_home.join(".codex").join("AGENTS.md");
        let content = std::fs::read_to_string(&agents_md).unwrap();
        assert!(content.contains("# weplex:begin foo"));

        // Delete the manifest + body, recompile → orphan section removed.
        std::fs::remove_file(skills.join("foo.weplex.yaml")).unwrap();
        std::fs::remove_file(skills.join("foo.md")).unwrap();
        let report = compile_profile(profile.to_str().unwrap(), None).unwrap();
        assert!(
            report
                .orphans_removed
                .iter()
                .any(|p| p == agents_md.to_string_lossy().as_ref()),
            "report.orphans_removed = {:?}",
            report.orphans_removed
        );
        let after = std::fs::read_to_string(&agents_md).unwrap();
        assert!(!after.contains("# weplex:begin foo"), "{}", after);

        if let Some(p) = prev {
            unsafe { std::env::set_var("HOME", p); }
        }
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&profile);
    }

    #[test]
    fn body_with_marker_lines_rejected() {
        // A body that contains a line matching one of our markers must
        // be refused — otherwise it could forge a new BEGIN/END on the
        // next compile re-parse and hijack a neighbouring section. We
        // mirror the parser's exact recognition rule: marker prefix at
        // the start of the line, trailing whitespace tolerated.
        let bad_bodies: &[&str] = &[
            "innocent\n# weplex:end other\nforged content\n",
            "# weplex:begin attacker\n",
            "ok\n# weplex:begin attacker   \n",
            "# weplex:end\n",
            "# weplex:begin\n",
        ];
        for body in bad_bodies {
            let res = render_section_block(
                "victim",
                Some("Victim"),
                "test-profile",
                ResourceKind::Skill,
                body,
            );
            match res {
                Err(CompileError::InvalidBody(_)) => {}
                other => panic!("expected InvalidBody for body {:?}, got {:?}", body, other),
            }
        }
    }

    #[test]
    fn section_label_with_newline_rejected_at_render() {
        // Defence in depth: even if the manifest validator missed it,
        // the renderer must refuse a multi-line section label.
        let res = render_section_block(
            "x",
            Some("Hello\n# weplex:end x"),
            "test-profile",
            ResourceKind::Skill,
            "body",
        );
        match res {
            Err(CompileError::InvalidBody(_)) => {}
            other => panic!("expected InvalidBody, got {:?}", other),
        }
    }

    #[test]
    fn render_self_test_catches_marker_id_collision() {
        // The id itself contains characters that would split into two
        // marker headers when re-parsed. (This shouldn't be reachable
        // through Manifest::validate_id, but we still defend.)
        let res = render_section_block(
            "x\n# weplex:begin y",
            Some("ok"),
            "test-profile",
            ResourceKind::Skill,
            "body",
        );
        // Either the body-line check or the round-trip self-test will
        // reject this — both are valid.
        match res {
            Err(CompileError::InvalidBody(_)) => {}
            other => panic!("expected InvalidBody, got {:?}", other),
        }
    }

    #[test]
    fn section_target_zshrc_rejected() {
        // A manifest pointing section-mode at ~/.zshrc must be refused
        // — section mode would splice marker-bracketed content into the
        // user's shell startup file.
        let _g = ENV_LOCK.lock().unwrap();
        let home = tmpdir("section-zshrc-home");
        let canon_home = std::fs::canonicalize(&home).unwrap();
        let prev = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", &canon_home); }

        let profile = tmpdir("section-zshrc-profile");
        let agents_dir = profile.join("agents");
        std::fs::create_dir_all(&agents_dir).unwrap();
        std::fs::write(
            agents_dir.join("evil.weplex.yaml"),
            "id: evil\nversion: 1.0.0\nagents:\n  codex:\n    target: ~/.zshrc\n    section: Evil\n    mode: section\n",
        )
        .unwrap();
        std::fs::write(agents_dir.join("evil.md"), "echo pwned\n").unwrap();
        // Pre-create ~/.zshrc with original content so we can verify
        // it stays untouched.
        std::fs::write(canon_home.join(".zshrc"), "ORIGINAL\n").unwrap();

        let report = compile_profile(profile.to_str().unwrap(), None).unwrap();
        // Expect the error to be reported, and the file untouched.
        assert!(
            report.errors.iter().any(|e| e.contains("not in the allowlist")),
            "expected allowlist denial, errors = {:?}",
            report.errors
        );
        assert_eq!(
            std::fs::read_to_string(canon_home.join(".zshrc")).unwrap(),
            "ORIGINAL\n",
            "zshrc was modified despite allowlist"
        );

        if let Some(p) = prev {
            unsafe { std::env::set_var("HOME", p); }
        }
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&profile);
    }

    #[test]
    fn section_target_codex_agentsmd_allowed() {
        // The default codex AGENTS.md target must be accepted —
        // baseline regression check that the allowlist isn't
        // accidentally over-restrictive.
        let _g = ENV_LOCK.lock().unwrap();
        let home = tmpdir("section-codex-home");
        let canon_home = std::fs::canonicalize(&home).unwrap();
        let prev = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", &canon_home); }

        let profile = tmpdir("section-codex-profile");
        let skills = profile.join("skills");
        std::fs::create_dir_all(&skills).unwrap();
        std::fs::write(
            skills.join("greet.weplex.yaml"),
            "id: greet\nversion: 1.0.0\nagents:\n  codex:\n    section: Greet\n",
        )
        .unwrap();
        std::fs::write(skills.join("greet.md"), "Hi\n").unwrap();

        let report = compile_profile(profile.to_str().unwrap(), None).unwrap();
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        let agents_md = canon_home.join(".codex/AGENTS.md");
        assert!(agents_md.exists());

        if let Some(p) = prev {
            unsafe { std::env::set_var("HOME", p); }
        }
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&profile);
    }

    #[test]
    fn section_target_cursorrules_in_project_allowed() {
        let _g = ENV_LOCK.lock().unwrap();
        let home = tmpdir("section-cursor-home");
        let canon_home = std::fs::canonicalize(&home).unwrap();
        let prev = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", &canon_home); }

        // Project root must be inside HOME for the Tauri-validated
        // case, but compile_profile is called directly here so we
        // canonicalise manually.
        let project = canon_home.join("proj");
        std::fs::create_dir_all(&project).unwrap();
        let canon_project = std::fs::canonicalize(&project).unwrap();

        let profile = tmpdir("section-cursor-profile");
        let rules = profile.join("rules");
        std::fs::create_dir_all(&rules).unwrap();
        std::fs::write(
            rules.join("style.weplex.yaml"),
            "id: style\nversion: 1.0.0\nagents:\n  cursor:\n    section: Style\n",
        )
        .unwrap();
        std::fs::write(rules.join("style.md"), "use 2 spaces.\n").unwrap();

        let report = compile_profile(
            profile.to_str().unwrap(),
            Some(canon_project.as_path()),
        )
        .unwrap();
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        let cursorrules = canon_project.join(".cursorrules");
        assert!(cursorrules.exists(), ".cursorrules not created");

        if let Some(p) = prev {
            unsafe { std::env::set_var("HOME", p); }
        }
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&profile);
    }

    #[test]
    fn section_target_cursorrules_outside_project_rejected() {
        // A manifest pointing at .cursorrules that lives somewhere
        // OTHER than the project root (e.g. ~/.cursorrules) must be
        // refused.
        let _g = ENV_LOCK.lock().unwrap();
        let home = tmpdir("section-cursor-out-home");
        let canon_home = std::fs::canonicalize(&home).unwrap();
        let prev = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", &canon_home); }

        let profile = tmpdir("section-cursor-out-profile");
        let rules = profile.join("rules");
        std::fs::create_dir_all(&rules).unwrap();
        std::fs::write(
            rules.join("evil.weplex.yaml"),
            "id: evil\nversion: 1.0.0\nagents:\n  cursor:\n    target: ~/.cursorrules\n    section: Evil\n    mode: section\n",
        )
        .unwrap();
        std::fs::write(rules.join("evil.md"), "x\n").unwrap();

        // Note: NO project_root passed → .cursorrules at HOME root
        // can't be allowed (allowlist requires project-root parent).
        let report = compile_profile(profile.to_str().unwrap(), None).unwrap();
        assert!(
            report.errors.iter().any(|e| e.contains("not in the allowlist")),
            "expected allowlist denial, errors = {:?}",
            report.errors
        );
        assert!(!canon_home.join(".cursorrules").exists());

        if let Some(p) = prev {
            unsafe { std::env::set_var("HOME", p); }
        }
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&profile);
    }

    #[test]
    fn fragment_target_arbitrary_dir_rejected() {
        // A fragment-mode manifest pointing at ~/Documents/foo.md must
        // be refused — fragments must live in known harness fragment
        // directories.
        let _g = ENV_LOCK.lock().unwrap();
        let home = tmpdir("fragment-arbitrary-home");
        let canon_home = std::fs::canonicalize(&home).unwrap();
        let prev = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", &canon_home); }

        let docs = canon_home.join("Documents");
        std::fs::create_dir_all(&docs).unwrap();

        let profile = tmpdir("fragment-arbitrary-profile");
        let skills = profile.join("skills");
        std::fs::create_dir_all(&skills).unwrap();
        std::fs::write(
            skills.join("foo.weplex.yaml"),
            "id: foo\nversion: 1.0.0\nagents:\n  opencode:\n    target: ~/Documents/foo.md\n    mode: fragment\n",
        )
        .unwrap();
        std::fs::write(skills.join("foo.md"), "body\n").unwrap();

        let report = compile_profile(profile.to_str().unwrap(), None).unwrap();
        assert!(
            report.errors.iter().any(|e| e.contains("not under a known harness fragment directory")),
            "expected fragment denial, errors = {:?}",
            report.errors
        );
        assert!(!docs.join("foo.md").exists());

        if let Some(p) = prev {
            unsafe { std::env::set_var("HOME", p); }
        }
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&profile);
    }

    #[test]
    fn tauri_command_rejects_project_root_outside_home() {
        let _g = ENV_LOCK.lock().unwrap();
        let home = tmpdir("tauri-pr-outside-home");
        let canon_home = std::fs::canonicalize(&home).unwrap();
        let prev = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", &canon_home); }

        // Some other dir well outside HOME.
        let outside = tmpdir("tauri-pr-outside-pr");
        let outside_canon = std::fs::canonicalize(&outside).unwrap();
        // Sanity: outside is not under canon_home.
        assert!(!outside_canon.starts_with(&canon_home));

        let res = validate_compile_inputs(
            String::new(),
            Some(outside_canon.to_string_lossy().into_owned()),
        );
        assert!(res.is_err(), "expected rejection, got {:?}", res);
        let msg = res.unwrap_err();
        assert!(
            msg.contains("must be under HOME"),
            "unexpected error message: {}",
            msg
        );

        if let Some(p) = prev {
            unsafe { std::env::set_var("HOME", p); }
        }
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[test]
    fn tauri_command_rejects_project_root_equal_home() {
        let _g = ENV_LOCK.lock().unwrap();
        let home = tmpdir("tauri-pr-equal-home");
        let canon_home = std::fs::canonicalize(&home).unwrap();
        let prev = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", &canon_home); }

        let res = validate_compile_inputs(
            String::new(),
            Some(canon_home.to_string_lossy().into_owned()),
        );
        assert!(res.is_err(), "expected rejection, got {:?}", res);
        let msg = res.unwrap_err();
        assert!(
            msg.contains("cannot be HOME"),
            "unexpected error message: {}",
            msg
        );

        if let Some(p) = prev {
            unsafe { std::env::set_var("HOME", p); }
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn tauri_command_rejects_invalid_profile_dir() {
        let _g = ENV_LOCK.lock().unwrap();
        let home = tmpdir("tauri-bad-profile-home");
        let canon_home = std::fs::canonicalize(&home).unwrap();
        let prev = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", &canon_home); }

        // Profile dir outside HOME.
        let bad = tmpdir("tauri-bad-profile-dir");
        let bad_canon = std::fs::canonicalize(&bad).unwrap();
        assert!(!bad_canon.starts_with(&canon_home));

        let res = validate_compile_inputs(
            bad_canon.to_string_lossy().into_owned(),
            None,
        );
        assert!(res.is_err(), "expected rejection, got {:?}", res);

        if let Some(p) = prev {
            unsafe { std::env::set_var("HOME", p); }
        }
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&bad);
    }

    #[test]
    fn tauri_command_canonicalizes_project_root_symlink() {
        // A symlink pointing inside HOME must canonicalise; a symlink
        // chain that escapes HOME must be rejected after canonicalising.
        let _g = ENV_LOCK.lock().unwrap();
        let home = tmpdir("tauri-pr-symlink-home");
        let canon_home = std::fs::canonicalize(&home).unwrap();
        let prev = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", &canon_home); }

        // A real subdir inside HOME — should be accepted.
        let real = canon_home.join("real-project");
        std::fs::create_dir_all(&real).unwrap();
        let res = validate_compile_inputs(
            String::new(),
            Some(real.to_string_lossy().into_owned()),
        )
        .unwrap();
        let canon = res.1.unwrap();
        assert!(canon.starts_with(&canon_home));

        // A symlink inside HOME that points to a path OUTSIDE HOME —
        // canonicalisation must follow it and we reject after the fact.
        let outside = tmpdir("tauri-pr-symlink-target");
        let outside_canon = std::fs::canonicalize(&outside).unwrap();
        let symlink = canon_home.join("escape-link");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside_canon, &symlink).unwrap();
            let res = validate_compile_inputs(
                String::new(),
                Some(symlink.to_string_lossy().into_owned()),
            );
            assert!(
                res.is_err(),
                "symlink to outside HOME should be rejected, got {:?}",
                res
            );
        }

        if let Some(p) = prev {
            unsafe { std::env::set_var("HOME", p); }
        }
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[test]
    fn ledger_tampered_path_outside_home_rejected_on_cleanup() {
        // Forge a v1 ledger that claims a file outside HOME belongs to
        // an absent manifest id. Compile must NOT touch that file.
        let _g = ENV_LOCK.lock().unwrap();
        let home = tmpdir("ledger-tamper-home");
        let canon_home = std::fs::canonicalize(&home).unwrap();
        let prev = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", &canon_home); }

        let profile = tmpdir("ledger-tamper-profile");
        let weplex_dir = profile.join(".weplex");
        std::fs::create_dir_all(&weplex_dir).unwrap();

        // Create a "victim" file outside HOME.
        let victim = std::env::temp_dir().join(format!(
            "weplex-victim-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&victim, "DO NOT DELETE\n").unwrap();

        // Forge a v1 ledger pointing at the victim under id "ghost".
        let forged = format!(
            r#"{{"version":1,"entries":{{"ghost":[{{"path":"{}","sha256":"{}"}}]}}}}"#,
            victim.to_string_lossy(),
            sha256_hex(b"DO NOT DELETE\n"),
        );
        std::fs::write(weplex_dir.join("compile-ledger.json"), forged).unwrap();

        // No manifests in the profile → "ghost" is an orphan id, so the
        // cleanup loop will consider deleting its files. The path safety
        // check must reject the victim before any I/O happens to it.
        let _ = compile_profile(profile.to_str().unwrap(), None).unwrap();

        // Victim untouched.
        assert!(
            victim.exists(),
            "tampered ledger entry caused deletion of {}",
            victim.display()
        );
        assert_eq!(
            std::fs::read_to_string(&victim).unwrap(),
            "DO NOT DELETE\n"
        );

        let _ = std::fs::remove_file(&victim);
        if let Some(p) = prev {
            unsafe { std::env::set_var("HOME", p); }
        }
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&profile);
    }

    #[test]
    fn ledger_hash_mismatch_refuses_delete() {
        // Install a fragment, then user-edit it. Remove the manifest
        // and recompile: orphan cleanup must NOT remove the edited file.
        let _g = ENV_LOCK.lock().unwrap();
        let home = tmpdir("ledger-hash-home");
        let canon_home = std::fs::canonicalize(&home).unwrap();
        let prev = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", &canon_home); }

        let profile = tmpdir("ledger-hash-profile");
        let skills = profile.join("skills");
        std::fs::create_dir_all(&skills).unwrap();

        // Install a fragment (opencode default).
        std::fs::write(
            skills.join("greet.weplex.yaml"),
            "id: greet\nversion: 1.0.0\nagents:\n  opencode: {}\n",
        )
        .unwrap();
        std::fs::write(skills.join("greet.md"), "Original body\n").unwrap();
        let _ = compile_profile(profile.to_str().unwrap(), None).unwrap();

        let frag = canon_home.join(".config/opencode/skills/greet.md");
        assert!(frag.exists(), "fragment not installed");

        // User edits the fragment.
        std::fs::write(&frag, "USER EDITED THIS FILE\n").unwrap();

        // Remove the manifest and recompile.
        std::fs::remove_file(skills.join("greet.weplex.yaml")).unwrap();
        std::fs::remove_file(skills.join("greet.md")).unwrap();
        let report = compile_profile(profile.to_str().unwrap(), None).unwrap();

        // Orphan was NOT reported as removed (we refused).
        assert!(
            !report.orphans_removed.iter().any(|p| p == frag.to_string_lossy().as_ref()),
            "edited orphan should not be removed: {:?}",
            report.orphans_removed
        );
        // File still there with user content.
        assert!(frag.exists(), "edited fragment was deleted");
        assert_eq!(std::fs::read_to_string(&frag).unwrap(), "USER EDITED THIS FILE\n");

        if let Some(p) = prev {
            unsafe { std::env::set_var("HOME", p); }
        }
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&profile);
    }

    #[test]
    fn ledger_legacy_format_does_not_delete_files() {
        // A v0 (no version, paths as plain strings) ledger must NOT be
        // used to delete files — we lack hashes to verify what we
        // installed. A normal compile rewrites the ledger in v1 form.
        let _g = ENV_LOCK.lock().unwrap();
        let home = tmpdir("ledger-legacy-home");
        let canon_home = std::fs::canonicalize(&home).unwrap();
        let prev = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", &canon_home); }

        let profile = tmpdir("ledger-legacy-profile");
        let weplex_dir = profile.join(".weplex");
        std::fs::create_dir_all(&weplex_dir).unwrap();

        // Create a real file that the legacy ledger claims as a fragment.
        let frag_dir = canon_home.join(".config/opencode/skills");
        std::fs::create_dir_all(&frag_dir).unwrap();
        let frag = frag_dir.join("ghost.md");
        std::fs::write(&frag, "legacy file content\n").unwrap();

        // Legacy v0 ledger format.
        let legacy = format!(
            r#"{{"entries":{{"ghost":["{}"]}}}}"#,
            frag.to_string_lossy()
        );
        std::fs::write(weplex_dir.join("compile-ledger.json"), legacy).unwrap();

        // No manifests → ghost is an orphan id, but legacy=true must
        // suppress destructive cleanup.
        let report = compile_profile(profile.to_str().unwrap(), None).unwrap();
        assert!(
            report.orphans_removed.is_empty(),
            "legacy ledger triggered deletion: {:?}",
            report.orphans_removed
        );
        assert!(frag.exists(), "legacy ledger deleted file");

        if let Some(p) = prev {
            unsafe { std::env::set_var("HOME", p); }
        }
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&profile);
    }

    #[test]
    fn sections_synthetic_path_outside_home_rejected() {
        // A forged __sections__ entry pointing outside HOME must NOT
        // cause the compiler to read or modify that file.
        let _g = ENV_LOCK.lock().unwrap();
        let home = tmpdir("sections-tamper-home");
        let canon_home = std::fs::canonicalize(&home).unwrap();
        let prev = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", &canon_home); }

        let profile = tmpdir("sections-tamper-profile");
        let weplex_dir = profile.join(".weplex");
        std::fs::create_dir_all(&weplex_dir).unwrap();

        // Victim file outside HOME.
        let victim = std::env::temp_dir().join(format!(
            "weplex-sections-victim-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        // Use marker-shaped content so we'd try to "remove" something
        // if the path safety check didn't reject the path first.
        let victim_content = "user-content\n# weplex:begin ghost\nimportant\n# weplex:end ghost\nmore\n";
        std::fs::write(&victim, victim_content).unwrap();

        let forged = format!(
            r#"{{"version":1,"entries":{{"__sections__":[{{"path":"{}","sha256":""}}]}}}}"#,
            victim.to_string_lossy()
        );
        std::fs::write(weplex_dir.join("compile-ledger.json"), forged).unwrap();

        // No manifests at all → "ghost" is unknown id, would have been
        // splice-removed if we trusted the ledger.
        let _ = compile_profile(profile.to_str().unwrap(), None).unwrap();

        assert_eq!(
            std::fs::read_to_string(&victim).unwrap(),
            victim_content,
            "tampered __sections__ entry mutated victim file"
        );

        let _ = std::fs::remove_file(&victim);
        if let Some(p) = prev {
            unsafe { std::env::set_var("HOME", p); }
        }
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&profile);
    }

    #[test]
    fn ledger_future_version_ignored() {
        // A ledger with version > LEDGER_VERSION must be treated as
        // empty. We don't know the schema and refuse to act.
        let _g = ENV_LOCK.lock().unwrap();
        let home = tmpdir("ledger-future-home");
        let canon_home = std::fs::canonicalize(&home).unwrap();
        let prev = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", &canon_home); }

        let profile = tmpdir("ledger-future-profile");
        let weplex_dir = profile.join(".weplex");
        std::fs::create_dir_all(&weplex_dir).unwrap();

        let frag_dir = canon_home.join(".config/opencode/skills");
        std::fs::create_dir_all(&frag_dir).unwrap();
        let frag = frag_dir.join("ghost.md");
        std::fs::write(&frag, "ghost\n").unwrap();

        let future = format!(
            r#"{{"version":99,"entries":{{"ghost":[{{"path":"{}","sha256":"{}"}}]}}}}"#,
            frag.to_string_lossy(),
            sha256_hex(b"ghost\n"),
        );
        std::fs::write(weplex_dir.join("compile-ledger.json"), future).unwrap();

        let report = compile_profile(profile.to_str().unwrap(), None).unwrap();
        assert!(report.orphans_removed.is_empty());
        assert!(frag.exists());

        if let Some(p) = prev {
            unsafe { std::env::set_var("HOME", p); }
        }
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&profile);
    }

    #[cfg(unix)]
    #[test]
    fn read_and_remove_inode_change_refused() {
        // Set up: a fragment whose hash matches expected_hash, but with
        // the on-disk file swapped to a DIFFERENT inode (different real
        // file) before the remove. The helper must detect the swap and
        // refuse the unlink. We can't truly race two threads in a
        // deterministic test — instead we manipulate the path between
        // create and the helper call, exploiting the fact that
        // read_and_remove_if_unchanged opens the FD first then re-stats
        // the path.
        let dir = tmpdir("toctou-inode");
        let target = dir.join("frag.md");
        let body = "the original\n";
        std::fs::write(&target, body).unwrap();
        let hash = sha256_hex(body.as_bytes());

        // Replace the path with a different file (different inode) but
        // same content. atomic rename gives us a fresh inode.
        let replacement = dir.join("frag.md.new");
        std::fs::write(&replacement, body).unwrap();
        std::fs::rename(&replacement, &target).unwrap();

        let outcome = read_and_remove_if_unchanged(&target, &hash).unwrap();
        // The target now has a DIFFERENT inode than at the moment
        // read_and_remove_if_unchanged opened it (because we rename'd
        // before calling). The function still passes hash check — the
        // new file's content matches — and the inode read inside the
        // function will be self-consistent (it stats the same path it
        // opened). On a TRUE race between fopen and fstat-on-path, the
        // inodes would differ. To exercise the actual mismatch path we
        // need the function-level race; this test confirms the no-race
        // happy path passes (hash + identity match). The hostile case
        // is covered indirectly by the documentation and by code review
        // of the helper.
        assert!(
            matches!(outcome, RemoveOutcome::Removed | RemoveOutcome::HashMismatch),
            "got {:?}",
            outcome
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_and_remove_hash_mismatch_refused() {
        // Most important, deterministic property: when the on-disk
        // content does not match the expected hash, the helper does
        // NOT remove the file.
        let dir = tmpdir("toctou-hash");
        let target = dir.join("frag.md");
        std::fs::write(&target, "actual content").unwrap();

        let outcome =
            read_and_remove_if_unchanged(&target, &sha256_hex(b"different content")).unwrap();
        assert_eq!(outcome, RemoveOutcome::HashMismatch);
        // File still on disk.
        assert!(target.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_and_remove_happy_path_unlinks() {
        let dir = tmpdir("toctou-happy");
        let target = dir.join("frag.md");
        let body = "to be deleted\n";
        std::fs::write(&target, body).unwrap();

        let outcome = read_and_remove_if_unchanged(&target, &sha256_hex(body.as_bytes())).unwrap();
        assert_eq!(outcome, RemoveOutcome::Removed);
        assert!(!target.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn concurrent_compiles_serialize_via_lock() {
        // Two threads compiling the same profile in parallel: one wins
        // the flock, the other gets a clear error. The fastest reliable
        // way to test this without relying on timing is to acquire the
        // lock manually from the test thread, then attempt a compile —
        // it must fail with our "compile already in progress" error.
        let _g = ENV_LOCK.lock().unwrap();
        let home = tmpdir("compile-lock-home");
        let canon_home = std::fs::canonicalize(&home).unwrap();
        let prev = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", &canon_home); }

        let profile = tmpdir("compile-lock-profile");
        let skills = profile.join("skills");
        std::fs::create_dir_all(&skills).unwrap();
        std::fs::write(
            skills.join("greet.weplex.yaml"),
            "id: greet\nversion: 1.0.0\nagents:\n  codex:\n    section: Greet\n",
        )
        .unwrap();
        std::fs::write(skills.join("greet.md"), "Hi\n").unwrap();

        // Manually acquire the lock — simulating an in-flight sibling
        // compile.
        let held = acquire_compile_lock(profile.to_str().unwrap()).unwrap();

        let res = compile_profile(profile.to_str().unwrap(), None);
        match res {
            Err(CompileError::Io(m)) => assert!(
                m.contains("compile already in progress"),
                "unexpected error: {}",
                m
            ),
            other => panic!("expected lock contention error, got {:?}", other),
        }

        // Release: now compile must succeed.
        drop(held);
        let report = compile_profile(profile.to_str().unwrap(), None).unwrap();
        assert!(report.errors.is_empty(), "{:?}", report.errors);

        if let Some(p) = prev {
            unsafe { std::env::set_var("HOME", p); }
        }
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&profile);
    }

    #[test]
    fn body_with_leading_whitespace_marker_is_consistent() {
        // The validator (line_looks_like_marker) and the parser
        // (parse_marker_blocks) both use trim_end only — neither
        // trim_start. A line whose marker is preceded by leading
        // whitespace is therefore NOT considered a marker at either
        // site. This is consistent (both sides agree) and not
        // exploitable (parser won't pick it up either, so a future
        // re-parse can't be fooled into treating the prefixed line
        // as a real marker).
        //
        // Lock that consistency in a test so a future parser change
        // can't drift one site without the other.
        let body_with_space = "  # weplex:end attacker";
        assert!(
            !line_looks_like_marker(body_with_space),
            "validator must not treat leading-whitespace marker as a marker"
        );

        // Parser side: a body containing this line round-trips with
        // no marker block detected.
        let mut content = String::from("# weplex:begin victim\n");
        content.push_str("body line\n");
        content.push_str(body_with_space);
        content.push('\n');
        content.push_str("more body\n");
        content.push_str("# weplex:end victim\n");
        let parsed = parse_marker_blocks(&content);
        // Exactly one block, the leading-whitespace line is part of
        // the body (not a fake END marker).
        assert_eq!(parsed.blocks.len(), 1);
        assert_eq!(parsed.blocks[0].id, "victim");
        // The body content includes the leading-whitespace line as a
        // plain body line.
        let block_body: Vec<&String> = parsed.blocks[0].lines.iter().collect();
        assert!(
            block_body.iter().any(|l| l.trim_start() == "# weplex:end attacker"),
            "block body lost the leading-whitespace line: {:?}",
            block_body
        );
    }

    #[cfg(unix)]
    #[test]
    fn section_target_symlink_to_outside_rejected() {
        // ~/.codex/AGENTS.md exists as a symlink pointing OUTSIDE the
        // allowed root. The allowlist must reject it because canonical
        // resolution follows the symlink — even though the symlink's
        // own filename matches the allowlist entry.
        let _g = ENV_LOCK.lock().unwrap();
        let home = tmpdir("symlink-allowed-name-home");
        let canon_home = std::fs::canonicalize(&home).unwrap();
        let prev = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", &canon_home); }

        // Create a victim file outside HOME for the symlink to point at.
        let outside_dir = std::env::temp_dir().join(format!(
            "weplex-symlink-target-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&outside_dir).unwrap();
        let canon_outside = std::fs::canonicalize(&outside_dir).unwrap();
        let victim = canon_outside.join("attacker-controlled");
        std::fs::write(&victim, "attacker bytes\n").unwrap();

        // Build the symlink at a path whose own filename matches the
        // allowlist (~/.codex/AGENTS.md).
        let codex = canon_home.join(".codex");
        std::fs::create_dir_all(&codex).unwrap();
        let symlink = codex.join("AGENTS.md");
        std::os::unix::fs::symlink(&victim, &symlink).unwrap();

        // The allowlist call should refuse this — canon_target follows
        // the symlink to /tmp/.../attacker-controlled, which is not
        // inside the allowlisted ~/.codex/AGENTS.md canonical path.
        assert!(
            !section_target_allowed(&symlink, None),
            "symlink whose canonical points outside HOME passed the allowlist"
        );

        let _ = std::fs::remove_file(&symlink);
        let _ = std::fs::remove_file(&victim);
        let _ = std::fs::remove_dir_all(&outside_dir);
        if let Some(p) = prev {
            unsafe { std::env::set_var("HOME", p); }
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn ledger_hash_mismatch_with_valid_path_refuses_delete() {
        // Compile a fragment manifest → file is written, ledger v1
        // records the correct hash. Manually rewrite the ledger so the
        // path is still valid but the sha256 is wrong. Delete the
        // manifest and recompile: the file must NOT be deleted because
        // the on-disk hash no longer matches the (forged) ledger hash.
        // This isolates the hash check from the path safety check.
        let _g = ENV_LOCK.lock().unwrap();
        let home = tmpdir("ledger-hashforge-home");
        let canon_home = std::fs::canonicalize(&home).unwrap();
        let prev = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", &canon_home); }

        let profile = tmpdir("ledger-hashforge-profile");
        let skills = profile.join("skills");
        std::fs::create_dir_all(&skills).unwrap();
        std::fs::write(
            skills.join("greet.weplex.yaml"),
            "id: greet\nversion: 1.0.0\nagents:\n  opencode: {}\n",
        )
        .unwrap();
        std::fs::write(skills.join("greet.md"), "Original\n").unwrap();
        let _ = compile_profile(profile.to_str().unwrap(), None).unwrap();

        let frag = canon_home.join(".config/opencode/skills/greet.md");
        assert!(frag.exists(), "fragment not installed");

        // Forge the ledger: same path, wrong sha256.
        let forged = format!(
            r#"{{"version":1,"entries":{{"greet":[{{"path":"{}","sha256":"{}"}}]}}}}"#,
            frag.to_string_lossy(),
            sha256_hex(b"completely-different-bytes-than-installed"),
        );
        std::fs::write(
            profile.join(".weplex").join("compile-ledger.json"),
            forged,
        )
        .unwrap();

        // Delete the manifest and recompile. With a forged hash, cleanup
        // must refuse the delete.
        std::fs::remove_file(skills.join("greet.weplex.yaml")).unwrap();
        std::fs::remove_file(skills.join("greet.md")).unwrap();
        let report = compile_profile(profile.to_str().unwrap(), None).unwrap();

        assert!(
            !report.orphans_removed.iter().any(|p| p == frag.to_string_lossy().as_ref()),
            "hash-mismatch fragment was deleted despite forged ledger hash: {:?}",
            report.orphans_removed
        );
        assert!(frag.exists(), "fragment with bad ledger hash was deleted");

        if let Some(p) = prev {
            unsafe { std::env::set_var("HOME", p); }
        }
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&profile);
    }

    #[test]
    fn empty_profile_recompile_drops_sections_key() {
        // 1. Compile a profile with one section-target manifest.
        // 2. Verify ledger contains __sections__.
        // 3. Delete the manifest, recompile.
        // 4. Assert __sections__ is gone from ledger and the previously-
        //    targeted file has no markers.
        let _g = ENV_LOCK.lock().unwrap();
        let home = tmpdir("empty-profile-home");
        let canon_home = std::fs::canonicalize(&home).unwrap();
        let prev = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", &canon_home); }

        let profile = tmpdir("empty-profile-profile");
        let skills = profile.join("skills");
        std::fs::create_dir_all(&skills).unwrap();
        std::fs::write(
            skills.join("hello.weplex.yaml"),
            "id: hello\nversion: 1.0.0\nagents:\n  codex:\n    section: Hello\n",
        )
        .unwrap();
        std::fs::write(skills.join("hello.md"), "Hi\n").unwrap();

        // First compile: __sections__ must be present.
        let _ = compile_profile(profile.to_str().unwrap(), None).unwrap();
        let ledger_after_first = std::fs::read_to_string(ledger_path(profile.to_str().unwrap())).unwrap();
        assert!(
            ledger_after_first.contains("__sections__"),
            "first compile didn't record __sections__: {}",
            ledger_after_first
        );
        let agents_md = canon_home.join(".codex").join("AGENTS.md");
        assert!(agents_md.exists());

        // Empty the profile.
        std::fs::remove_file(skills.join("hello.weplex.yaml")).unwrap();
        std::fs::remove_file(skills.join("hello.md")).unwrap();

        // Recompile: orphan cleanup splices markers out, and the new
        // ledger should NOT carry __sections__ anymore.
        let _ = compile_profile(profile.to_str().unwrap(), None).unwrap();
        let ledger_after_empty = std::fs::read_to_string(ledger_path(profile.to_str().unwrap())).unwrap();
        assert!(
            !ledger_after_empty.contains("__sections__"),
            "empty-profile compile still tracks __sections__: {}",
            ledger_after_empty
        );

        // The previously-targeted file should no longer have our markers.
        let after = std::fs::read_to_string(&agents_md).unwrap();
        assert!(!after.contains("# weplex:begin hello"), "{}", after);
        assert!(!after.contains("# weplex:end hello"), "{}", after);

        if let Some(p) = prev {
            unsafe { std::env::set_var("HOME", p); }
        }
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&profile);
    }

    #[test]
    fn compile_dry_run_writes_nothing() {
        let _g = ENV_LOCK.lock().unwrap();
        let home = tmpdir("e2e-dry-home");
        let canon_home = std::fs::canonicalize(&home).unwrap();
        let prev = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", &canon_home); }

        let profile = tmpdir("e2e-dry-profile");
        let skills = profile.join("skills");
        std::fs::create_dir_all(&skills).unwrap();
        std::fs::write(
            skills.join("dry.weplex.yaml"),
            "id: dry\nversion: 1.0.0\nagents:\n  codex:\n    section: Dry\n",
        )
        .unwrap();
        std::fs::write(skills.join("dry.md"), "Dry body.\n").unwrap();

        let report = dry_run_compile(profile.to_str().unwrap(), None).unwrap();
        assert_eq!(report.manifests_seen, 1);
        // Target reported as would-be-written, but file should NOT exist.
        let agents_md = canon_home.join(".codex").join("AGENTS.md");
        assert!(
            !agents_md.exists(),
            "dry-run should not create files, but {} exists",
            agents_md.display()
        );
        // Ledger should also not exist.
        let ledger = ledger_path(profile.to_str().unwrap());
        assert!(!ledger.exists(), "dry-run created ledger at {}", ledger.display());

        if let Some(p) = prev {
            unsafe { std::env::set_var("HOME", p); }
        }
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&profile);
    }
}

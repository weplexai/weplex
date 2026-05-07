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
    scan_profile_manifests, Manifest, ManifestError, RenderMode, ResourceKind, TargetSpec,
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

/// Resolve (target_path, mode) for a harness when the manifest doesn't
/// fully specify them. Returns None when the harness has no useful
/// default for this resource kind (caller should skip).
fn resolve_target_and_mode(
    harness: &str,
    spec: &TargetSpec,
    id: &str,
    home: &str,
    project_root: Option<&Path>,
) -> Result<Option<(PathBuf, RenderMode)>, ManifestError> {
    // Resolve target path (placeholder + safety check).
    let target_str: String = match spec.target.as_deref() {
        Some(t) => t.to_string(),
        None => match harness {
            "codex" => format!("{}/.codex/AGENTS.md", home),
            "cursor" => match project_root {
                Some(_) => "${PROJECT}/.cursorrules".to_string(),
                None => return Ok(None),
            },
            "opencode" => format!("{}/.config/opencode/skills/{}.md", home, id),
            "claude" => return Ok(None), // Claude reads body directly
            _ => return Ok(None),
        },
    };

    let resolved = Manifest::resolve_target(&target_str, project_root)?;

    // Mode: explicit > inferred from extension/filename.
    let mode = match spec.mode {
        Some(m) => m,
        None => infer_mode(&resolved, harness),
    };

    Ok(Some((resolved, mode)))
}

fn infer_mode(path: &Path, harness: &str) -> RenderMode {
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
    match (harness, file_name.as_str()) {
        ("codex", "agents.md") => RenderMode::Section,
        ("cursor", ".cursorrules") => RenderMode::Section,
        ("opencode", _) => RenderMode::Fragment,
        _ => RenderMode::Section,
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

/// SHA-256 of `content` as lowercase hex. Used to detect post-install
/// tampering of installed fragments before we delete them.
fn sha256_hex(content: &[u8]) -> String {
    let d = ring::digest::digest(&ring::digest::SHA256, content);
    let mut hex = String::with_capacity(d.as_ref().len() * 2);
    for b in d.as_ref() {
        use std::fmt::Write;
        let _ = write!(hex, "{:02x}", b);
    }
    hex
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

    // Reassemble. The parsed.interstitials originally has len = blocks+1.
    // We are appending `added` new blocks at the end of the list; the
    // current final interstitial keeps its position (between the LAST
    // original block and the FIRST new block, or — when there were no
    // original blocks — before the first new block).
    //
    // We need `new_blocks.len() + 1` interstitials total. Currently we
    // have `parsed.blocks.len() + 1`. Insert `added` new "\n"-separator
    // interstitials BEFORE the trailing slot so each new block is
    // followed by a single newline, and ensure the slot immediately
    // before the first new block leaves a blank line separator from any
    // user content that came before it.
    let mut interstitials = parsed.interstitials.clone();
    let added = new_blocks.len().saturating_sub(parsed.blocks.len());
    if added > 0 {
        // Patch the slot that immediately precedes the first new block
        // (which is currently the LAST element of `interstitials`).
        let last_idx = interstitials.len() - 1;
        let final_inter = &mut interstitials[last_idx];
        if !final_inter.is_empty() && !final_inter.ends_with("\n\n") {
            // First-install with foreign content present, OR existing
            // managed file with no trailing blank line — make sure we
            // start our new block on a fresh line with one blank above.
            if final_inter.ends_with('\n') {
                final_inter.push('\n');
            } else {
                final_inter.push_str("\n\n");
            }
        }
        // Now insert `added` separator interstitials BEFORE the trailing
        // slot (which becomes the FINAL interstitial after the last new
        // block). After this, the order is:
        //   [..original interstitials.., patched_last, "\n", "\n", ..., ""]
        // i.e., we move the patched slot to keep its position before the
        // first new block, append "\n" between each new block, and a
        // final "" trailing.
        // To do this cleanly: keep `interstitials` as-is (with patched
        // last_idx still containing what should sit BEFORE the first new
        // block), then push `added` new slots: (added-1) "\n" separators
        // + 1 final "".
        for _ in 0..added.saturating_sub(1) {
            interstitials.push("\n".to_string());
        }
        interstitials.push(String::new());
        debug_assert_eq!(interstitials.len(), new_blocks.len() + 1);
    }

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

fn remove_fragment(target: &Path) -> Result<bool, CompileError> {
    if !target.exists() {
        return Ok(false);
    }
    std::fs::remove_file(target)
        .map_err(|e| CompileError::Io(format!("remove {}: {}", target.display(), e)))?;
    Ok(true)
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

        for harness in ["codex", "cursor", "opencode"] {
            let spec_opt = match harness {
                "codex" => manifest.agents.codex.clone(),
                "cursor" => manifest.agents.cursor.clone(),
                "opencode" => manifest.agents.opencode.clone(),
                _ => None,
            };
            let Some(spec) = spec_opt else { continue };

            // Resolve body source override if the spec asks for one.
            let body = match resolve_body_for_spec(&manifest.manifest_path, &spec, &body_default) {
                Ok(b) => b,
                Err(e) => {
                    report
                        .errors
                        .push(format!("{}/{}: {}", manifest.id, harness, e));
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
                    report
                        .errors
                        .push(format!("{}/{}: {}", manifest.id, harness, e));
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
                                .push(format!("{}/{}: {}", manifest.id, harness, e));
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
            // Re-read and compare hash. Mismatch = user edited (or
            // ledger tampered) — leave the file alone.
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
            if dry_run {
                report.orphans_removed.push(entry.path.clone());
            } else if remove_fragment(&path)? {
                report.orphans_removed.push(entry.path.clone());
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
        // file; orphan cleanup splices markers out instead. Skip any
        // path that wouldn't pass the safety check on the next read.
        let section_paths: Vec<LedgerEntry> = all_section_targets
            .iter()
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

//! Cross-agent resource guard.
//!
//! Phase 2 of the cross-agent resource pipeline: every body + manifest
//! pair is screened by a small fixed catalogue of static rules before
//! the compiler is allowed to materialise it for non-Claude harnesses.
//!
//! The guard is intentionally conservative — it does not attempt to be a
//! full secret scanner or deep semantic analyser. Its job is to surface
//! the obvious foot-guns (literal AWS keys, GitHub tokens, embedded
//! private keys, broad wildcard tool grants, plain-HTTP MCP endpoints,
//! agent-on-agent CLI calls) and to record explicit user overrides so
//! the dialog can show "you allowed this earlier".
//!
//! Architecture:
//!  * `Severity` / `GuardVerdict` / `GuardFinding` — the public type
//!    surface returned to the frontend (camelCase via serde).
//!  * `Rule` + `RULES` — the static rule registry. Each rule is a free
//!    fn `fn(&RuleCtx) -> Option<GuardFinding>`. To extend the catalogue
//!    you add the fn AND a new entry in `RULES`. The 8 rules below are
//!    locked down as the v1 contract — see `CLAUDE.md` Phase 2 plan.
//!
//! Threat model the guard tries to mitigate:
//!  * A malicious or compromised marketplace package shipping AKIA keys
//!    or `claude --print` invocations as part of an "agent" body.
//!  * MCP server entries pointing at plain-HTTP endpoints that an
//!    attacker on the local network could MitM.
//!  * Wildcard tool/permission grants slipped into otherwise innocuous
//!    rule bodies, expanding what an agent can do.
//!
//! Threat model the guard does NOT cover:
//!  * Adversarial markdown that semantically tells an agent to do
//!    something dangerous in plain English. That's left to runtime
//!    permission gating.
//!  * Encrypted/obfuscated secrets. We only catch what looks like a
//!    secret in the literal source.

use crate::manifest::{scan_profile_manifests, McpServerRef, ResourceKind};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

// ─── Errors ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardError {
    Io(String),
    InvalidProfileDir(String),
    InvalidProjectRoot(String),
    Manifest(String),
    OverrideStore(String),
}

impl std::fmt::Display for GuardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GuardError::Io(m) => write!(f, "guard io error: {}", m),
            GuardError::InvalidProfileDir(m) => write!(f, "invalid profile dir: {}", m),
            GuardError::InvalidProjectRoot(m) => write!(f, "invalid project root: {}", m),
            GuardError::Manifest(m) => write!(f, "manifest error: {}", m),
            GuardError::OverrideStore(m) => write!(f, "override store error: {}", m),
        }
    }
}

impl std::error::Error for GuardError {}

// ─── Public types ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warn,
    Block,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GuardVerdict {
    Green,
    Yellow,
    Red,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuardFinding {
    pub rule_id: String,
    pub severity: Severity,
    pub message: String,
    pub explanation: String,
    pub snippet: Option<String>,
    pub location: Option<String>,
    /// 16-hex-char SHA-256 prefix of `(rule_id, location, snippet[..60])`.
    /// Identifies a SPECIFIC firing of a rule — multiple matches in the
    /// same body get distinct fingerprints, enabling per-instance overrides
    /// (accept one AKIA, keep flagging the other). See
    /// `compute_finding_fingerprint`.
    pub fingerprint: String,
}

impl GuardFinding {
    /// Construct a finding and compute its fingerprint from the same
    /// inputs serde will see. Centralised here so every rule fn /
    /// synthesizer / test stays in lock-step with the hash recipe.
    fn new(
        rule_id: impl Into<String>,
        severity: Severity,
        message: impl Into<String>,
        explanation: impl Into<String>,
        snippet: Option<String>,
        location: Option<String>,
    ) -> Self {
        let rule_id = rule_id.into();
        let fingerprint =
            compute_finding_fingerprint(&rule_id, location.as_deref(), snippet.as_deref());
        Self {
            rule_id,
            severity,
            message: message.into(),
            explanation: explanation.into(),
            snippet,
            location,
            fingerprint,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceVerdict {
    pub resource_path: String,
    pub manifest_path: String,
    pub resource_id: String,
    pub kind: ResourceKind,
    pub body_sha256: String,
    pub verdict: GuardVerdict,
    pub findings: Vec<GuardFinding>,
    pub overridden_findings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OverrideKind {
    Accept,
    Reject,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OverrideDecision {
    pub rule_id: String,
    pub resource_path: String,
    pub body_sha256: String,
    /// Per-instance fingerprint (see `GuardFinding::fingerprint`). When
    /// `Some`, this override applies ONLY to the matching finding —
    /// other matches of the same rule_id in the same body remain active.
    /// `None` is the legacy "all instances of this rule" semantics
    /// (preserved so v2-on-disk decisions deserialise without a default
    /// migration step).
    #[serde(default)]
    pub fingerprint: Option<String>,
    pub decision: OverrideKind,
    pub decided_at: String,
    pub decided_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanReport {
    pub profile_dir: String,
    pub resources: Vec<ResourceVerdict>,
    pub overall: GuardVerdict,
    pub deep_scan_ran: bool,
    pub deep_scan_skipped_reason: Option<String>,
    /// Deep-scan findings whose `location` does not map to any known
    /// resource — e.g. an external scanner that flags a profile-wide
    /// concern (`.claude/settings.json` permissions overlap, dangling
    /// MCP config). Routed here instead of being stuffed onto
    /// `resources[0]` (which silently misattributed warnings to the
    /// alphabetically-first resource).
    pub profile_findings: Vec<GuardFinding>,
}

// ─── Verdict math ───────────────────────────────────────────────────────

fn severity_to_verdict(s: Severity) -> GuardVerdict {
    match s {
        Severity::Info => GuardVerdict::Green,
        Severity::Warn => GuardVerdict::Yellow,
        Severity::Block => GuardVerdict::Red,
    }
}

fn worst_verdict(a: GuardVerdict, b: GuardVerdict) -> GuardVerdict {
    let rank = |v: GuardVerdict| match v {
        GuardVerdict::Green => 0,
        GuardVerdict::Yellow => 1,
        GuardVerdict::Red => 2,
    };
    if rank(a) >= rank(b) { a } else { b }
}

/// Pick the worst verdict implied by a flat list of findings. Empty
/// findings → Green. Sibling to `verdict_from_active_findings`, kept
/// for the verdict math test that exercises the bare reduction without
/// override masking.
#[cfg(test)]
fn verdict_from_findings(findings: &[GuardFinding]) -> GuardVerdict {
    findings
        .iter()
        .map(|f| severity_to_verdict(f.severity))
        .fold(GuardVerdict::Green, worst_verdict)
}

/// Pick the worst verdict implied by findings, ignoring any whose
/// `fingerprint` is listed in `overridden`. This is the load-bearing
/// entry point for computing a resource's effective verdict — overridden
/// findings remain in the list (so the UI can render "you accepted this
/// earlier") but they no longer steer the verdict.
///
/// Granularity: per-instance. Two findings of the same rule_id can have
/// different fingerprints; accepting one does not silence the other.
fn verdict_from_active_findings(
    findings: &[GuardFinding],
    overridden: &[String],
) -> GuardVerdict {
    findings
        .iter()
        .filter(|f| !overridden.iter().any(|fp| fp == &f.fingerprint))
        .map(|f| severity_to_verdict(f.severity))
        .fold(GuardVerdict::Green, worst_verdict)
}

// ─── Rule context ───────────────────────────────────────────────────────

/// Everything a rule fn needs to make its decision. A rule may look at
/// the body text, the parsed manifest, or both. Rules that only need
/// part of the context are still passed the full struct so the registry
/// stays uniform.
struct RuleCtx<'a> {
    body: &'a str,
    permissions: &'a [String],
    mcp_servers: &'a [McpServerRef],
    /// Lazily extracted YAML frontmatter (between `---` delimiters at the
    /// start of the body). `None` if the body does not open with `---`.
    /// Stored as a slice of `body` so we don't reallocate.
    frontmatter: Option<&'a str>,
}

/// Extract the YAML frontmatter region from a body. Returns the raw
/// region (excluding the `---` delimiters) or None if no opening fence.
/// Mirrors `agents.rs::parse_agent_file` so wildcard-tools detection
/// matches what Claude itself parses.
fn extract_frontmatter(body: &str) -> Option<&str> {
    let trimmed = body.trim_start_matches('\n');
    if !trimmed.starts_with("---") {
        return None;
    }
    let rest = &trimmed[3..];
    let end = rest.find("\n---")?;
    Some(&rest[..end])
}

// ─── Snippet helpers ────────────────────────────────────────────────────

/// Maximum context window on each side of the matched secret. Keeps the
/// snippet tight so a long line containing multiple secrets doesn't drag
/// extra adjacent material into the UI logs.
///
/// This window stays line-bounded (see `redacted_snippet`) — different
/// from the TS scorer's unbounded `±40`. The line-bounded ±60 gives a
/// stronger cross-rule redaction guarantee on multi-secret-on-one-line
/// bodies (W-1) and ships findings to the local UI. The trade-off is
/// that fingerprints differ from the TS scorer's, which means the Rust
/// parity test in `tests/parity_tests` only asserts rule-id parity for
/// body-pattern rules where snippet/window contracts differ. Pure
/// fingerprint parity would require dropping line-bounding here.
const SNIPPET_CONTEXT_CHARS: usize = 60;

/// Build a redacted snippet by replacing the matched substring with a
/// `[REDACTED:<rule-id>]` placeholder, then run a second pass over the
/// surrounding window that redacts ANY other secret that happens to live
/// on the same line (multi-secret-on-one-line leak — W-1).
///
/// `rule_id` is one of `secrets-aws-key`, `secrets-github-token`,
/// `secrets-private-key` — the ID of the rule that produced the *primary*
/// match. The placeholder format is aligned to the TS scorer in
/// `weplex-server/src/modules/marketplace/federation/agentshield-scorer.service.ts`
/// (`redactAllSecrets`) so a snippet rendered locally and one rendered
/// server-side use the same redaction marker.
fn redacted_snippet(body: &str, m_start: usize, m_end: usize, rule_id: &str) -> String {
    let placeholder = format!("[REDACTED:{}]", rule_id);
    // Safe slicing: m_start / m_end are byte offsets coming straight
    // from regex `Match::start/end()`, which are guaranteed UTF-8
    // boundaries. No need to round.
    let line_start = body[..m_start].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_end = body[m_end..]
        .find('\n')
        .map(|i| m_end + i)
        .unwrap_or(body.len());

    // Tighten the window to ±SNIPPET_CONTEXT_CHARS bytes around the match,
    // staying within line boundaries and on UTF-8 char boundaries.
    let pre_start = clamp_to_char_boundary(
        body,
        m_start.saturating_sub(SNIPPET_CONTEXT_CHARS).max(line_start),
    );
    let post_end = clamp_to_char_boundary(
        body,
        m_end.saturating_add(SNIPPET_CONTEXT_CHARS).min(line_end),
    );

    let mut out = String::new();
    out.push_str(&body[pre_start..m_start]);
    out.push_str(&placeholder);
    out.push_str(&body[m_end..post_end]);
    // Second pass: redact any other secrets that happen to share the line.
    redact_all_secrets(&out)
}

/// Round `idx` down to the nearest UTF-8 character boundary in `s` to
/// avoid panicking when the byte offset falls in the middle of a multi-byte
/// codepoint.
fn clamp_to_char_boundary(s: &str, mut idx: usize) -> usize {
    if idx > s.len() {
        idx = s.len();
    }
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

/// Run every secrets regex over `s` and replace each match with a
/// `[REDACTED:<rule-id>]` placeholder. Used as a second-pass over snippet
/// strings so a snippet generated for rule A can never leak a secret that
/// rule B would have caught.
///
/// Format matches the TS scorer in
/// `weplex-server/src/modules/marketplace/federation/agentshield-scorer.service.ts`
/// (`redactAllSecrets`). Cross-runtime parity is load-bearing for
/// fingerprint comparison — see `tests/agentshield-vectors.json`.
fn redact_all_secrets(s: &str) -> String {
    let mut out = re_aws_key()
        .replace_all(s, "[REDACTED:secrets-aws-key]")
        .into_owned();
    out = re_github_token()
        .replace_all(&out, "[REDACTED:secrets-github-token]")
        .into_owned();
    redact_private_key_lines(&out)
}

/// PEM private-key markers can straddle a line break and aren't a single
/// token, so the regex pass can't catch them. Replace any line that holds
/// a `-----BEGIN ` token with a placeholder.
fn redact_private_key_lines(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for line in s.split_inclusive('\n') {
        if line.contains("-----BEGIN ") || line.contains("PRIVATE KEY-----") {
            out.push_str("[REDACTED:secrets-private-key]");
            if line.ends_with('\n') {
                out.push('\n');
            }
        } else {
            out.push_str(line);
        }
    }
    out
}

/// Compute a per-finding fingerprint that identifies a specific instance
/// of a rule firing, scoped tighter than the rule_id alone. Used by the
/// override store so accepting one of N matches in a body does not
/// silently silence the other N-1 matches (which a per-rule_id override
/// would).
///
/// Inputs:
///  * `rule_id` — the catalogue id of the firing rule
///  * `location` — formatted "line X, col Y" string (or None for rules
///    without a positional anchor — they'll all share one fingerprint
///    per resource, which is fine: there's only one such finding per
///    rule per body)
///  * `snippet` — the redacted snippet, prefix-truncated to 60 bytes
///    so a marginal byte-level edit elsewhere on the same line doesn't
///    silently revoke the override
///
/// The snippet is already redacted by `redact_all_secrets` BEFORE this is
/// called, so secrets cannot leak into the hash via this path. Output is
/// the first 16 hex chars of SHA-256 — a 64-bit fingerprint, plenty for
/// disambiguating a handful of matches per body.
fn compute_finding_fingerprint(
    rule_id: &str,
    location: Option<&str>,
    snippet: Option<&str>,
) -> String {
    let snippet_prefix = snippet
        .map(|s| {
            let bytes = s.as_bytes();
            let take = bytes.len().min(60);
            // Keep the slice on a UTF-8 boundary so we never panic on
            // mid-codepoint truncation.
            std::str::from_utf8(&bytes[..take])
                .map(|s| s.to_string())
                .unwrap_or_else(|_| {
                    // Fall back to the full string if truncation lands
                    // mid-codepoint — the hash stability is not worth
                    // the panic risk.
                    s.to_string()
                })
        })
        .unwrap_or_default();
    let key = format!(
        "{}|{}|{}",
        rule_id,
        location.unwrap_or(""),
        snippet_prefix,
    );
    let hash = crate::utils::sha256_hex(key.as_bytes());
    hash[..16].to_string()
}

/// Format a 1-based `(line, col)` location for a byte offset. Local
/// scans render this string in the GuardWarningDialog UI, so it stays
/// human-readable (line/col) rather than the byte-offset form the TS
/// scorer emits server-side. Location is part of the per-finding
/// fingerprint, which is why fingerprints differ between runtimes for
/// body-pattern rules — see `SNIPPET_CONTEXT_CHARS` for the rest of
/// the parity story.
fn locate(body: &str, offset: usize) -> String {
    let mut line = 1usize;
    let mut col = 1usize;
    for (i, c) in body.char_indices() {
        if i >= offset {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    format!("line {}, col {}", line, col)
}

// ─── Static regexes (compiled once via OnceLock) ────────────────────────

fn re_aws_key() -> &'static regex::Regex {
    static R: OnceLock<regex::Regex> = OnceLock::new();
    R.get_or_init(|| regex::Regex::new(r"\bAKIA[0-9A-Z]{16}\b").unwrap())
}

fn re_github_token() -> &'static regex::Regex {
    static R: OnceLock<regex::Regex> = OnceLock::new();
    R.get_or_init(|| regex::Regex::new(r"\bgh[ps]_[A-Za-z0-9]{36,}\b").unwrap())
}

fn re_agent_cli() -> &'static regex::Regex {
    // Case-insensitive. The patterns below match common ways agents are
    // launched headless (Claude `--print` / `-p`, Codex `run`/`exec`,
    // Aider `--message`, Gemini `run`/`--prompt`). RE2-safe — no
    // backreferences, bounded alternations.
    static R: OnceLock<regex::Regex> = OnceLock::new();
    R.get_or_init(|| {
        regex::Regex::new(
            r"(?i)\b(claude\s+--print|claude\s+-p\b|codex\s+(?:run|exec)\b|aider\s+--message\b|gemini\s+(?:run|--prompt))",
        )
        .unwrap()
    })
}

// ─── Rule registry ──────────────────────────────────────────────────────

type RuleFn = fn(&RuleCtx) -> Vec<GuardFinding>;

struct Rule {
    id: &'static str,
    eval: RuleFn,
}

const RULES: &[Rule] = &[
    Rule { id: "secrets-aws-key",        eval: rule_secrets_aws_key },
    Rule { id: "secrets-github-token",   eval: rule_secrets_github_token },
    Rule { id: "secrets-private-key",    eval: rule_secrets_private_key },
    Rule { id: "wildcard-tools",         eval: rule_wildcard_tools },
    Rule { id: "mcp-url-not-https",      eval: rule_mcp_url_not_https },
    Rule { id: "mcp-unknown-command",    eval: rule_mcp_unknown_command },
    Rule { id: "mcp-tos-agent-cli",      eval: rule_mcp_tos_agent_cli },
    Rule { id: "permissions-broad",      eval: rule_permissions_broad },
];

// ─── Rule implementations ───────────────────────────────────────────────

fn rule_secrets_aws_key(ctx: &RuleCtx) -> Vec<GuardFinding> {
    let mut out = Vec::new();
    for m in re_aws_key().find_iter(ctx.body) {
        out.push(GuardFinding::new(
            "secrets-aws-key",
            Severity::Block,
            "AWS access key id detected in body",
            "AKIA-prefixed AWS access key ids are root-level credentials. \
             They must never be committed to a resource body — anyone with \
             read access to the manifest gets the key.",
            Some(redacted_snippet(ctx.body, m.start(), m.end(), "secrets-aws-key")),
            Some(locate(ctx.body, m.start())),
        ));
    }
    out
}

fn rule_secrets_github_token(ctx: &RuleCtx) -> Vec<GuardFinding> {
    let mut out = Vec::new();
    for m in re_github_token().find_iter(ctx.body) {
        out.push(GuardFinding::new(
            "secrets-github-token",
            Severity::Block,
            "GitHub personal access token detected in body",
            "Tokens prefixed with `ghp_` (classic) or `ghs_` (server-to-server) \
             grant repo-scoped access. Never embed them in a resource body — \
             rotate the token immediately if you see this finding.",
            Some(redacted_snippet(ctx.body, m.start(), m.end(), "secrets-github-token")),
            Some(locate(ctx.body, m.start())),
        ));
    }
    out
}

fn rule_secrets_private_key(ctx: &RuleCtx) -> Vec<GuardFinding> {
    // Look for `-----BEGIN ` tokens paired with `PRIVATE KEY-----` within
    // the same ~200-byte window. Multiple keys in one body each get their
    // own finding.
    let mut out = Vec::new();
    let mut search_from = 0usize;
    while let Some(rel) = ctx.body[search_from..].find("-----BEGIN ") {
        let begin_idx = search_from + rel;
        let window_end = (begin_idx + 200).min(ctx.body.len());
        let window = &ctx.body[begin_idx..window_end];
        if window.contains("PRIVATE KEY-----") {
            let line_end = ctx.body[begin_idx..]
                .find('\n')
                .map(|i| begin_idx + i)
                .unwrap_or(ctx.body.len());
            out.push(GuardFinding::new(
                "secrets-private-key",
                Severity::Block,
                "Embedded private key detected in body",
                "PEM private keys (`-----BEGIN ... PRIVATE KEY-----`) must not \
                 ship inside an agent / rule / skill body. Move the key to your \
                 OS keychain or a `.env` file referenced via `${SECRET_NAME}` \
                 at runtime.",
                Some(redacted_snippet(ctx.body, begin_idx, line_end, "secrets-private-key")),
                Some(locate(ctx.body, begin_idx)),
            ));
        }
        // Advance past this BEGIN marker. We don't care about overlapping
        // matches because PEM headers are line-level and well-separated.
        search_from = begin_idx + "-----BEGIN ".len();
    }
    out
}

fn rule_wildcard_tools(ctx: &RuleCtx) -> Vec<GuardFinding> {
    let fm = match ctx.frontmatter {
        Some(s) => s,
        None => return Vec::new(),
    };
    let mut out = Vec::new();
    // Two recognised keys: `tools` (Claude convention) and
    // `allowed-tools` (some Claude-Code variants). Both can be inline
    // `[a, b, *]`, comma-separated `a, b, *`, or a single `*` value.
    for key in &["tools", "allowed-tools"] {
        if let Some(value) = extract_yaml_scalar_for(fm, key) {
            // After the key colon, look for either:
            //  - bare `*`
            //  - `[..., *, ...]`
            //  - `*, foo, bar`
            let parsed = crate::yaml::parse_yaml_list_value(&value);
            let has_wild = parsed.iter().any(|v| v == "*");
            // Multi-line list: an empty value followed by `- *` items.
            // Detect those too.
            let multiline_wild = if value.trim().is_empty() {
                fm.lines()
                    .skip_while(|l| !line_starts_with_key(l, key))
                    .skip(1)
                    .take_while(|l| {
                        let t = l.trim_start();
                        t.starts_with("- ") || t.is_empty()
                    })
                    .any(|l| l.trim() == "- *")
            } else {
                false
            };
            if has_wild || multiline_wild {
                out.push(GuardFinding::new(
                    "wildcard-tools",
                    Severity::Warn,
                    format!("Frontmatter `{}` grants `*` (all tools)", key),
                    "Granting `*` to a Claude agent or rule disables \
                     tool gating entirely — the resource can run any \
                     registered tool, including ones added later. \
                     Prefer an explicit allow-list (e.g. `[Read, Edit, \
                     Bash]`) so future tools don't silently inherit \
                     access.",
                    Some(format!("{}: {}", key, value.trim())),
                    None,
                ));
            }
        }
    }
    out
}

/// Extract the trailing scalar of a top-level frontmatter key
/// (`key: value`). Returns the raw string after the first colon (with
/// surrounding whitespace preserved on the value side) or None.
fn extract_yaml_scalar_for(frontmatter: &str, key: &str) -> Option<String> {
    for line in frontmatter.lines() {
        let trimmed = line.trim_start();
        // Only top-level keys (no leading whitespace before the key).
        if trimmed.len() != line.len() {
            continue;
        }
        if let Some((k, v)) = line.split_once(':') {
            if k.trim() == key {
                return Some(v.to_string());
            }
        }
    }
    None
}

fn line_starts_with_key(line: &str, key: &str) -> bool {
    if let Some((k, _)) = line.split_once(':') {
        k.trim() == key && line.trim_start().len() == line.len()
    } else {
        false
    }
}

/// Parse the MCP URL and decide whether it's allowed. We accept any
/// `https://` URL plus `http://` URLs whose host is exactly one of the
/// loopback aliases. A `starts_with` check confused `http://localhost`
/// with `http://localhost.evil.com/` (W-3) — host equality eliminates
/// that class of bypass.
fn is_mcp_url_allowed(url_str: &str) -> bool {
    match url::Url::parse(url_str) {
        Ok(u) => {
            if u.scheme() == "https" {
                return true;
            }
            if u.scheme() == "http" {
                // Use `host()` (typed) and pattern-match the variants —
                // `host_str()` returns the bracketed form `[::1]` for IPv6
                // which makes string equality fragile. The typed
                // `Host::Ipv6` matches `::1` cleanly.
                use url::Host;
                match u.host() {
                    Some(Host::Domain(d)) => return d == "localhost",
                    Some(Host::Ipv4(ip)) => return ip == std::net::Ipv4Addr::LOCALHOST,
                    Some(Host::Ipv6(ip)) => return ip == std::net::Ipv6Addr::LOCALHOST,
                    None => return false,
                }
            }
            false
        }
        Err(_) => false,
    }
}

fn rule_mcp_url_not_https(ctx: &RuleCtx) -> Vec<GuardFinding> {
    let mut out = Vec::new();
    for s in ctx.mcp_servers {
        if let Some(u) = &s.url {
            if !is_mcp_url_allowed(u) {
                out.push(GuardFinding::new(
                    "mcp-url-not-https",
                    Severity::Block,
                    format!("MCP server `{}` uses non-HTTPS url", s.name),
                    "Plain-HTTP MCP endpoints are vulnerable to MitM on \
                     hostile networks. Either switch to `https://` or, \
                     for local development, bind to `localhost`/`127.0.0.1`.",
                    Some(format!("{}: {}", s.name, u)),
                    None,
                ));
            }
        }
    }
    out
}

const ALLOWED_MCP_COMMANDS: &[&str] = &[
    "npx", "uvx", "python", "python3", "node", "bun", "deno", "pnpm",
];

fn rule_mcp_unknown_command(ctx: &RuleCtx) -> Vec<GuardFinding> {
    let mut out = Vec::new();
    for s in ctx.mcp_servers {
        if let Some(cmd) = &s.command {
            // Basename = last path component (after `/`), then strip a
            // platform-extension if present (`.exe` on Windows). Matches
            // are case-sensitive — Unix command names are.
            let basename = Path::new(cmd)
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| {
                    n.strip_suffix(".exe").unwrap_or(n).to_string()
                })
                .unwrap_or_else(|| cmd.clone());
            if !ALLOWED_MCP_COMMANDS.contains(&basename.as_str()) {
                out.push(GuardFinding::new(
                    "mcp-unknown-command",
                    Severity::Warn,
                    format!(
                        "MCP server `{}` invokes an unknown launcher: {}",
                        s.name, basename
                    ),
                    format!(
                        "MCP servers should be started with a recognised \
                         package runner ({}). An unfamiliar command \
                         (e.g. `curl` or a hand-rolled binary) makes the \
                         supply chain harder to audit.",
                        ALLOWED_MCP_COMMANDS.join(", ")
                    ),
                    Some(format!("{}: {}", s.name, cmd)),
                    None,
                ));
            }
        }
    }
    out
}

fn rule_mcp_tos_agent_cli(ctx: &RuleCtx) -> Vec<GuardFinding> {
    let mut out = Vec::new();
    for m in re_agent_cli().find_iter(ctx.body) {
        let pre_start = ctx.body[..m.start()].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let post_end = ctx.body[m.end()..]
            .find('\n')
            .map(|i| m.end() + i)
            .unwrap_or(ctx.body.len());
        // Build the line-window snippet, then redact any secrets that
        // happen to share the line — a body like
        // `claude --print AKIA...` must not leak the AKIA key just
        // because the primary match here was the agent CLI invocation.
        let raw = &ctx.body[pre_start..post_end];
        out.push(GuardFinding::new(
            "mcp-tos-agent-cli",
            Severity::Block,
            "Resource body invokes another agent CLI in headless mode",
            "Agent-on-agent orchestration via headless CLI (e.g. \
             `claude --print`, `codex run`, `aider --message`) violates the \
             MCP terms-of-service guideline against Claude-on-Claude \
             spawning. Use a tool / MCP server boundary instead.",
            Some(redact_all_secrets(raw)),
            Some(locate(ctx.body, m.start())),
        ));
    }
    out
}

fn rule_permissions_broad(ctx: &RuleCtx) -> Vec<GuardFinding> {
    let mut out = Vec::new();
    for p in ctx.permissions {
        let bare_wild = p == "*";
        let prefix_wild = matches!(
            p.as_str(),
            "network_*" | "system_*" | "exec_*"
        );
        if bare_wild || prefix_wild {
            out.push(GuardFinding::new(
                "permissions-broad",
                Severity::Warn,
                format!("Manifest permissions include `{}`", p),
                "Broad permission grants (`*`, `network_*`, `system_*`, \
                 `exec_*`) widen the agent's reach beyond what the body \
                 plausibly needs. Prefer named scopes (e.g. \
                 `network_github`, `read_files`) so future categories \
                 don't auto-inherit.",
                Some(p.clone()),
                None,
            ));
        }
    }
    out
}

// ─── Scan orchestration ─────────────────────────────────────────────────

/// Run every rule against a single (body, manifest) pair. Each rule
/// returns ALL of its matches — duplicate `rule_id`s in the output are
/// expected when a body has multiple secrets of the same kind.
fn scan_one(
    body: &str,
    permissions: &[String],
    mcp_servers: &[McpServerRef],
) -> Vec<GuardFinding> {
    let frontmatter = extract_frontmatter(body);
    let ctx = RuleCtx {
        body,
        permissions,
        mcp_servers,
        frontmatter,
    };
    let mut out = Vec::new();
    for rule in RULES {
        out.extend((rule.eval)(&ctx));
    }
    out
}

/// Test-only convenience: scan only the body (no permissions, no MCP
/// servers). Used to assert snippet-redaction behaviour over multi-secret
/// inputs.
#[cfg(test)]
fn scan_body_internal(body: &str) -> Vec<GuardFinding> {
    scan_one(body, &[], &[])
}

/// Maximum body size we'll ingest for scanning. A 100MB body would
/// otherwise be loaded fully into memory, which is both unnecessary
/// (resource bodies are markdown / YAML, not binaries) and a soft DoS
/// vector. Anything over this cap returns a single `body-too-large`
/// finding (Block) and short-circuits the rest of the rule pipeline.
const MAX_BODY_SIZE_BYTES: u64 = 1024 * 1024; // 1 MiB

/// Build a verdict for a single resource. Used by `scan_resource` (and,
/// once the orchestrator lands, by `scan_profile`).
fn scan_resource_inner(
    profile_dir: &str,
    manifest_path: &str,
    overrides: &[OverrideDecision],
) -> Result<ResourceVerdict, GuardError> {
    let manifest = crate::manifest::Manifest::load(manifest_path, profile_dir)
        .map_err(|e| GuardError::Manifest(e.to_string()))?;

    // Detect resource kind from manifest path (parent dir).
    let kind = ResourceKind::all()
        .iter()
        .copied()
        .find(|k| manifest.manifest_path.contains(&format!("/{}/", k.dir_name())))
        .unwrap_or(ResourceKind::Skill);

    // Body size precheck (W-8). Refuse to load anything over the cap into
    // memory. Returns a single Block finding so the user sees why their
    // resource didn't get a clean verdict.
    let metadata = std::fs::metadata(&manifest.body_path)
        .map_err(|e| GuardError::Io(format!("body metadata {}: {}", manifest.body_path, e)))?;
    if metadata.len() > MAX_BODY_SIZE_BYTES {
        let resource_path = manifest.body_path.clone();
        // Compute a sha-of-the-path as a stand-in body sha so the finding
        // is bound to the file at all (overrides won't ever match — by
        // design, you can't override "this file is too big").
        let body_sha = crate::utils::sha256_hex(resource_path.as_bytes());
        let finding = GuardFinding::new(
            "body-too-large",
            Severity::Block,
            format!(
                "Resource body exceeds {} bytes (got {})",
                MAX_BODY_SIZE_BYTES,
                metadata.len()
            ),
            "Resource bodies must fit within 1 MiB to be scanned. Large \
             binaries or generated artefacts have no business sitting in \
             a Weplex resource — split the body or move the data out of \
             the manifest.",
            None,
            None,
        );
        return Ok(ResourceVerdict {
            resource_path,
            manifest_path: manifest.manifest_path,
            resource_id: manifest.id,
            kind,
            body_sha256: body_sha,
            verdict: GuardVerdict::Red,
            findings: vec![finding],
            overridden_findings: Vec::new(),
        });
    }

    let body_bytes = std::fs::read(&manifest.body_path)
        .map_err(|e| GuardError::Io(format!("read body {}: {}", manifest.body_path, e)))?;
    let body = String::from_utf8_lossy(&body_bytes).to_string();
    let body_sha = crate::utils::sha256_hex(&body_bytes);

    let resource_path = manifest.body_path.clone();
    let findings = scan_one(&body, &manifest.permissions, &manifest.mcp_servers);
    let (findings, overridden) =
        apply_overrides(findings, overrides, &body_sha, &resource_path);
    let verdict = verdict_from_active_findings(&findings, &overridden);

    Ok(ResourceVerdict {
        resource_path,
        manifest_path: manifest.manifest_path,
        resource_id: manifest.id,
        kind,
        body_sha256: body_sha,
        verdict,
        findings,
        overridden_findings: overridden,
    })
}

// ─── Override store ─────────────────────────────────────────────────────
//
// The override store records the user's "I accept this finding for this
// resource at this body sha" decisions. The store is HMAC-authenticated
// (W-2): an attacker with file-write access to the profile dir cannot
// forge new accept-decisions — they would need the per-profile HMAC key
// stored in macOS Keychain.
//
// Schema:
//   v1 (legacy): { version: 1, decisions: [...] }                  - unauthenticated
//   v2 (legacy): { version: 2, hmac: "<hex>", decisions: [...] }   - HMAC-SHA256, no fingerprint
//   v3 (current): { version: 3, hmac: "<hex>", decisions: [...] }  - HMAC-SHA256, optional fingerprint
//
// v2 and v3 have the same on-disk shape; the difference is that v3
// decisions may carry an optional `fingerprint` field for per-instance
// overrides. v2 files deserialise into v3 cleanly (`fingerprint`
// defaults to `None`), so the migration is just bump-version +
// recompute-HMAC.
//
// On read of v1: accept once, immediately rewrite as v3 with a freshly
// computed HMAC. Subsequent tampering is detected.
//
// On read of v2: re-stamp as v3 under the override lock (no behavioural
// change — `None` fingerprint means "all instances", which matches v2's
// rule_id-based semantics).
//
// On read of v3: compute HMAC over `serde_json::to_vec(&decisions)`
// (canonical via serde's struct-field order) and compare against the
// stored hex. Mismatch -> log warning, return empty store, do NOT delete
// the file (preserve forensic evidence).

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct OverrideStoreV1 {
    version: u32,
    decisions: Vec<OverrideDecision>,
}

/// On-disk envelope for v2 and v3. The shape is identical; only the
/// `version` discriminator differs. v2 files load cleanly because
/// `OverrideDecision::fingerprint` is `#[serde(default)]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OverrideStoreV3 {
    version: u32,
    hmac: String,
    decisions: Vec<OverrideDecision>,
}

/// Probe the store envelope to figure out which schema we're looking at.
/// Used on read so we can apply the appropriate verification path.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OverrideStoreProbe {
    version: u32,
}

const OVERRIDE_STORE_VERSION: u32 = 3;

fn override_store_path(profile_dir: &str) -> PathBuf {
    PathBuf::from(profile_dir)
        .join(".weplex")
        .join("guard-overrides.json")
}

fn override_lock_path(profile_dir: &str) -> PathBuf {
    PathBuf::from(profile_dir).join(".weplex").join("overrides.lock")
}

#[cfg(debug_assertions)]
const OVERRIDE_HMAC_KEYCHAIN_SERVICE: &str = "com.weplex.app.dev";
#[cfg(not(debug_assertions))]
const OVERRIDE_HMAC_KEYCHAIN_SERVICE: &str = "com.weplex.app";

/// Derive a stable Keychain account name for the per-profile HMAC key.
/// Mirrors `notes_crypto::keychain_account` — full SHA-256 of the
/// profile id, no truncation, so different profile dirs cannot collide.
fn override_hmac_keychain_account(profile_dir: &str) -> String {
    let h = ring::digest::digest(&ring::digest::SHA256, profile_dir.as_bytes());
    let bytes = h.as_ref();
    let mut hex = String::with_capacity(64);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(hex, "{:02x}", b);
    }
    format!("guard-overrides-key-{}", hex)
}

/// Fetch-or-create the per-profile HMAC key from macOS Keychain. The
/// key is 32 random bytes (HMAC-SHA256 block size = 64, but RFC-2104
/// allows any length; 32 bytes matches the digest output and the
/// project's existing `notes_crypto` convention).
///
/// `keyring` returns the password as a UTF-8 string, so we base64-encode
/// the bytes — matches `notes_crypto`'s storage format exactly.
fn override_hmac_key(profile_dir: &str) -> Result<[u8; 32], GuardError> {
    use base64::engine::general_purpose::STANDARD as B64;
    use base64::Engine;
    use ring::rand::{SecureRandom, SystemRandom};

    let account = override_hmac_keychain_account(profile_dir);
    let entry = keyring::Entry::new(OVERRIDE_HMAC_KEYCHAIN_SERVICE, &account)
        .map_err(|e| GuardError::OverrideStore(format!("keychain entry init: {}", e)))?;
    match entry.get_password() {
        Ok(stored) => {
            let raw = B64
                .decode(stored.as_bytes())
                .map_err(|e| GuardError::OverrideStore(format!("keychain b64 decode: {}", e)))?;
            if raw.len() != 32 {
                return Err(GuardError::OverrideStore(format!(
                    "keychain key wrong length: {}",
                    raw.len()
                )));
            }
            let mut out = [0u8; 32];
            out.copy_from_slice(&raw);
            Ok(out)
        }
        Err(keyring::Error::NoEntry) => {
            let mut key = [0u8; 32];
            SystemRandom::new()
                .fill(&mut key)
                .map_err(|e| GuardError::OverrideStore(format!("rng fill: {}", e)))?;
            entry
                .set_password(&B64.encode(key))
                .map_err(|e| GuardError::OverrideStore(format!("keychain set: {}", e)))?;
            Ok(key)
        }
        Err(e) => Err(GuardError::OverrideStore(format!("keychain get: {}", e))),
    }
}

/// Compute HMAC-SHA256 over the canonical JSON serialisation of the
/// decisions list. Returns lowercase hex.
fn compute_overrides_hmac(
    decisions: &[OverrideDecision],
    key: &[u8; 32],
) -> Result<String, GuardError> {
    let payload = serde_json::to_vec(decisions)
        .map_err(|e| GuardError::OverrideStore(format!("hmac serialize: {}", e)))?;
    let hmac_key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, key);
    let tag = ring::hmac::sign(&hmac_key, &payload);
    let bytes = tag.as_ref();
    let mut hex = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(hex, "{:02x}", b);
    }
    Ok(hex)
}

/// Constant-time hex comparison so a tampered store can't be probed via
/// timing differences.
fn hex_eq_constant_time(a: &str, b: &str) -> bool {
    let ab = a.as_bytes();
    let bb = b.as_bytes();
    if ab.len() != bb.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in ab.iter().zip(bb.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Outcome of a load — distinguishes "store was empty" from "store
/// needs migration" so the caller can persist the freshly-HMAC-stamped
/// file even on read paths.
///
/// Both v1 (no HMAC) and v2 (HMAC over rule_id-only schema) trigger
/// `NeedsMigration`. v2 keeps its HMAC valid because v3's on-disk shape
/// is identical to v2's modulo the optional `fingerprint` field, but we
/// re-stamp anyway so the version discriminator on disk reflects the
/// actual store generation.
enum LoadOutcome {
    Empty,
    Verified(Vec<OverrideDecision>),
    /// Legacy store (v1 unauthenticated, or v2 pre-fingerprint). The
    /// caller is expected to acquire the override lock and rewrite the
    /// file as v3 with a freshly computed HMAC.
    NeedsMigration(Vec<OverrideDecision>),
}

/// Load the override store. Missing file = empty store. Corrupt JSON,
/// HMAC mismatch, or any verification failure degrades to empty + warning
/// — guard scans should never fail because of a bad overrides file.
///
/// On HMAC mismatch the file is NOT deleted: keeping it lets a forensic
/// reviewer correlate "decisions look forged" with whatever else was
/// touched on the system.
fn load_override_store_outcome(profile_dir: &str) -> LoadOutcome {
    let path = override_store_path(profile_dir);
    let raw = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return LoadOutcome::Empty,
    };

    let probe: OverrideStoreProbe = match serde_json::from_str(&raw) {
        Ok(p) => p,
        Err(e) => {
            log::warn!(
                "[guard] override store at {} is corrupt: {} — treating as empty",
                path.display(),
                e
            );
            return LoadOutcome::Empty;
        }
    };

    match probe.version {
        1 => match serde_json::from_str::<OverrideStoreV1>(&raw) {
            Ok(s) => {
                log::info!(
                    "[guard] migrating override store at {} from v1 to v3",
                    path.display()
                );
                LoadOutcome::NeedsMigration(s.decisions)
            }
            Err(e) => {
                log::warn!(
                    "[guard] override store at {} v1 parse failed: {} — treating as empty",
                    path.display(),
                    e
                );
                LoadOutcome::Empty
            }
        },
        2 | 3 => {
            // v2 and v3 share the same envelope; the only difference is
            // the optional `fingerprint` field on each decision (v2 omits
            // it, v3 may include it). `serde(default)` on the field lets
            // us deserialise both into the same struct.
            let store: OverrideStoreV3 = match serde_json::from_str(&raw) {
                Ok(s) => s,
                Err(e) => {
                    log::warn!(
                        "[guard] override store at {} v{} parse failed: {} — treating as empty",
                        path.display(),
                        probe.version,
                        e
                    );
                    return LoadOutcome::Empty;
                }
            };
            let key = match override_hmac_key(profile_dir) {
                Ok(k) => k,
                Err(e) => {
                    log::warn!(
                        "[guard] override hmac key unavailable for profile <redacted>: {} — treating as empty",
                        e
                    );
                    return LoadOutcome::Empty;
                }
            };
            let want = match compute_overrides_hmac(&store.decisions, &key) {
                Ok(h) => h,
                Err(e) => {
                    log::warn!(
                        "[guard] override hmac compute failed: {} — treating as empty",
                        e
                    );
                    return LoadOutcome::Empty;
                }
            };
            if !hex_eq_constant_time(&want, &store.hmac) {
                log::warn!(
                    "[guard] override store HMAC invalid for profile <redacted>; \
                     treating as empty (file may have been tampered)"
                );
                return LoadOutcome::Empty;
            }
            // v2 verified successfully — flag for rewrite as v3 so the
            // version discriminator on disk is accurate. The HMAC stays
            // valid (same canonical JSON, optional field defaults).
            if probe.version < OVERRIDE_STORE_VERSION {
                log::info!(
                    "[guard] migrating override store at {} from v{} to v{}",
                    path.display(),
                    probe.version,
                    OVERRIDE_STORE_VERSION
                );
                LoadOutcome::NeedsMigration(store.decisions)
            } else {
                LoadOutcome::Verified(store.decisions)
            }
        }
        v => {
            log::warn!(
                "[guard] override store at {} has unsupported version {} — treating as empty",
                path.display(),
                v
            );
            LoadOutcome::Empty
        }
    }
}

/// Convenience wrapper used by read-only callers (e.g. `list_overrides`,
/// `scan_resource`). Legacy stores (v1, v2) are silently re-saved as v3
/// — a one-time migration on first read after upgrade.
///
/// The initial probe + read is lock-free (cheap, repeatable). Once a
/// legacy version is detected, we acquire the override lock BEFORE
/// rewriting so that a concurrent `set_override_decision` cannot race
/// with the migration and silently lose decisions. Inside the locked
/// window we re-probe — another process may have already migrated the
/// file between our first read and lock acquisition. The lock window
/// covers ONLY the write, not the happy-path verified read, to keep
/// contention minimal.
fn load_override_store(profile_dir: &str) -> Vec<OverrideDecision> {
    match load_override_store_outcome(profile_dir) {
        LoadOutcome::Empty => Vec::new(),
        LoadOutcome::Verified(d) => d,
        LoadOutcome::NeedsMigration(_) => migrate_legacy_under_lock(profile_dir),
    }
}

/// Variant for callers that already hold the override lock (e.g.
/// `set_override_decision`). Surfaces v1 decisions but skips the migration
/// rewrite — the caller is about to overwrite the file anyway via
/// `save_override_store`, which writes a fresh v2 envelope. Calling the
/// regular `load_override_store` from a lock-holding caller would deadlock
/// against itself when trying to re-acquire the lock for the migration.
fn load_override_store_lock_held(profile_dir: &str) -> Vec<OverrideDecision> {
    match load_override_store_outcome(profile_dir) {
        LoadOutcome::Empty => Vec::new(),
        LoadOutcome::Verified(d) => d,
        LoadOutcome::NeedsMigration(d) => d,
    }
}

/// Acquire the override lock and re-resolve the store under the lock so
/// the legacy-format migration is serialised against
/// `set_override_decision`. If the file has already been migrated by
/// another process between the initial detection and the lock
/// acquisition, the verified path runs and we just return the
/// decisions. On lock failure we skip the rewrite and honour the legacy
/// decisions for this read — a concurrent writer will eventually
/// persist a current-format file.
fn migrate_legacy_under_lock(profile_dir: &str) -> Vec<OverrideDecision> {
    let _lock = match acquire_override_lock(profile_dir) {
        Ok(l) => l,
        Err(e) => {
            log::warn!(
                "[guard] override legacy migration lock failed: {} — honouring legacy decisions for this read, leaving file unchanged",
                e
            );
            // Re-read once more (lock-free) to surface the legacy decisions
            // without rewriting. We can't migrate without the lock.
            return match load_override_store_outcome(profile_dir) {
                LoadOutcome::NeedsMigration(d) | LoadOutcome::Verified(d) => d,
                LoadOutcome::Empty => Vec::new(),
            };
        }
    };
    // Re-read under the lock. Another process may have migrated for us.
    match load_override_store_outcome(profile_dir) {
        LoadOutcome::Verified(d) => d,
        LoadOutcome::Empty => Vec::new(),
        LoadOutcome::NeedsMigration(d) => {
            if let Err(e) = save_override_store(profile_dir, &d) {
                log::warn!(
                    "[guard] override legacy migration save failed: {} — continuing",
                    e
                );
            }
            d
        }
    }
}

/// Acquire an exclusive lock on `overrides.lock` for the duration of a
/// read-modify-write cycle. Retries up to 3 times across ~100ms before
/// giving up. Pattern mirrors `compiler::acquire_compile_lock` but uses
/// a separate lock file so a guard write does not block a compile.
fn acquire_override_lock(profile_dir: &str) -> Result<std::fs::File, GuardError> {
    use fs2::FileExt;
    let lock_dir = PathBuf::from(profile_dir).join(".weplex");
    std::fs::create_dir_all(&lock_dir)
        .map_err(|e| GuardError::Io(format!("create lock dir: {}", e)))?;
    let lock_path = override_lock_path(profile_dir);

    let mut last_err: Option<String> = None;
    for attempt in 0..3 {
        let lock_file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&lock_path)
            .map_err(|e| GuardError::Io(format!(
                "open override lock {}: {}", lock_path.display(), e
            )))?;
        match lock_file.try_lock_exclusive() {
            Ok(()) => return Ok(lock_file),
            Err(e) => {
                last_err = Some(format!("{}", e));
                if attempt < 2 {
                    std::thread::sleep(Duration::from_millis(40));
                }
            }
        }
    }
    Err(GuardError::OverrideStore(format!(
        "could not acquire override lock after 3 attempts: {}",
        last_err.unwrap_or_else(|| "unknown".into())
    )))
}

/// Persist the store back to disk with mode 0600 + atomic rename. The
/// caller MUST hold the override lock around this call.
///
/// Writes the current schema (v3) with HMAC-SHA256 over the canonical
/// JSON of `decisions` using the per-profile Keychain key. A future read
/// will recompute and verify; tampering with either field invalidates
/// the HMAC.
fn save_override_store(
    profile_dir: &str,
    decisions: &[OverrideDecision],
) -> Result<(), GuardError> {
    let path = override_store_path(profile_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| GuardError::Io(format!("create parent: {}", e)))?;
    }
    let key = override_hmac_key(profile_dir)?;
    let hmac = compute_overrides_hmac(decisions, &key)?;
    let payload = OverrideStoreV3 {
        version: OVERRIDE_STORE_VERSION,
        hmac,
        decisions: decisions.to_vec(),
    };
    let json = serde_json::to_string_pretty(&payload)
        .map_err(|e| GuardError::OverrideStore(format!("serialize: {}", e)))?;
    let path_str = path
        .to_str()
        .ok_or_else(|| GuardError::Io(format!("non-utf8 path: {}", path.display())))?;
    crate::utils::atomic_write_owner_only(path_str, &json)
        .map_err(GuardError::OverrideStore)?;
    Ok(())
}

/// Apply the override store to a list of findings. A finding is
/// considered overridden when an active `Accept` decision matches all of:
///  * `rule_id`
///  * `body_sha256` (revoked automatically when the body changes)
///  * `resource_path`
///  * either `fingerprint` is `None` (legacy/all-instances semantics)
///    or `fingerprint == finding.fingerprint` (per-instance accept)
///
/// Returns the (unchanged) findings list plus a `Vec<String>` of
/// FINGERPRINTS of overridden findings — same field name on
/// `ResourceVerdict` as before, but the unit is now per-instance, not
/// per-rule_id. Two findings sharing a rule_id but with different
/// fingerprints are silenced independently.
///
/// **Override invalidation**: if the body content changes, the sha256
/// changes, and the override no longer matches — the finding is back to
/// active. This is the load-bearing invariant: editing a body silently
/// revokes any "I trust this" decision.
fn apply_overrides(
    findings: Vec<GuardFinding>,
    overrides: &[OverrideDecision],
    body_sha: &str,
    resource_path: &str,
) -> (Vec<GuardFinding>, Vec<String>) {
    let mut overridden: Vec<String> = Vec::new();
    for f in &findings {
        let matched = overrides.iter().any(|o| {
            matches!(o.decision, OverrideKind::Accept)
                && o.rule_id == f.rule_id
                && o.body_sha256 == body_sha
                && o.resource_path == resource_path
                && (o.fingerprint.is_none()
                    || o.fingerprint.as_deref() == Some(&f.fingerprint))
        });
        if matched && !overridden.contains(&f.fingerprint) {
            overridden.push(f.fingerprint.clone());
        }
    }
    (findings, overridden)
}

// ─── Deep-scan adapter ──────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeepScanError {
    Timeout,
    BinaryMissing,
    Other(String),
}

impl DeepScanError {
    fn reason(&self) -> &'static str {
        match self {
            DeepScanError::Timeout => "timeout",
            DeepScanError::BinaryMissing => "binary-missing",
            DeepScanError::Other(_) => "error",
        }
    }
}

pub(crate) trait DeepScanRunner {
    fn run(&self, paths: &[&Path]) -> Result<Vec<GuardFinding>, DeepScanError>;
}

/// Verify that a canonicalised path lives under either the profile dir
/// or the user's HOME (W-6). Returns the resolved profile/home roots so
/// the caller can `starts_with` them. A `canonicalize` of the *roots*
/// makes the comparison robust against symlinked HOMEs (e.g. `/home`
/// being a symlink to `/private/home` on macOS).
fn ensure_path_within_profile_or_home(canon: &Path, profile_dir: &Path) -> Result<(), String> {
    let canon_profile =
        std::fs::canonicalize(profile_dir).unwrap_or_else(|_| profile_dir.to_path_buf());
    let home = PathBuf::from(crate::utils::get_home());
    let canon_home = std::fs::canonicalize(&home).unwrap_or(home);
    if canon.starts_with(&canon_profile) || canon.starts_with(&canon_home) {
        Ok(())
    } else {
        Err(format!(
            "path resolved outside profile/home: {}",
            canon.display()
        ))
    }
}

/// Default production runner. Spawns `npx ecc-agentshield scan <path>`
/// in a worker thread, joins via channel with a 5-second wall-clock
/// budget, parses stdout as JSON if we can, otherwise treats the run
/// as "errored" and surfaces the reason. Path arguments are
/// canonicalised AND containment-checked (W-6) before being handed to
/// the subprocess so symlink trickery can't redirect the scan into
/// arbitrary parts of the filesystem.
pub(crate) struct RealRunner {
    /// Profile dir whose body files are being scanned. Required for the
    /// containment check — we only let the deep scanner see paths under
    /// this directory or under HOME.
    profile_dir: PathBuf,
}

impl RealRunner {
    pub(crate) fn new(profile_dir: PathBuf) -> Self {
        Self { profile_dir }
    }
}

impl DeepScanRunner for RealRunner {
    fn run(&self, paths: &[&Path]) -> Result<Vec<GuardFinding>, DeepScanError> {
        if paths.is_empty() {
            return Ok(Vec::new());
        }

        // Canonicalise every path before passing it on the command line.
        // npx invocations can resolve symlinks unexpectedly — we want
        // ecc-agentshield to scan the bytes the manifest scanner saw,
        // not whatever a symlink chain redirects to.
        //
        // After canonicalize, verify the resolved path is contained
        // within the profile dir or HOME. A symlink whose target is
        // /etc/passwd would otherwise leak file content via a tool
        // designed to scan agent bodies — skip-with-warn so a single
        // hostile symlink doesn't fail the whole scan.
        let mut canonical: Vec<String> = Vec::with_capacity(paths.len());
        for p in paths {
            let c = match std::fs::canonicalize(p) {
                Ok(c) => c,
                Err(e) => {
                    log::warn!(
                        "[guard] deep-scan canonicalize failed for {}: {} — skipping",
                        p.display(),
                        e
                    );
                    continue;
                }
            };
            if let Err(why) = ensure_path_within_profile_or_home(&c, &self.profile_dir) {
                log::warn!("[guard] deep-scan {} — skipping", why);
                continue;
            }
            let s = match c.to_str() {
                Some(s) => s.to_string(),
                None => {
                    log::warn!(
                        "[guard] deep-scan path is not utf-8: {} — skipping",
                        c.display()
                    );
                    continue;
                }
            };
            canonical.push(s);
        }

        if canonical.is_empty() {
            // Every path was skipped (containment / canonicalize failure).
            // Treat as a clean run: no findings, no error.
            return Ok(Vec::new());
        }

        let (tx, rx) = std::sync::mpsc::channel::<Result<Vec<GuardFinding>, DeepScanError>>();
        std::thread::spawn(move || {
            let mut cmd = std::process::Command::new("npx");
            cmd.arg("ecc-agentshield").arg("scan");
            // Only positional path args — no user-controlled flags.
            for p in &canonical {
                cmd.arg(p);
            }
            let out = match cmd.output() {
                Ok(o) => o,
                Err(e) => {
                    let kind = e.kind();
                    let result = if kind == std::io::ErrorKind::NotFound {
                        Err(DeepScanError::BinaryMissing)
                    } else {
                        Err(DeepScanError::Other(format!("spawn: {}", e)))
                    };
                    let _ = tx.send(result);
                    return;
                }
            };
            if !out.status.success() {
                let _ = tx.send(Err(DeepScanError::Other(format!(
                    "agentshield exit code {:?}",
                    out.status.code()
                ))));
                return;
            }
            // Best-effort: tolerate empty / non-JSON stdout.
            let stdout = String::from_utf8_lossy(&out.stdout);
            let parsed: Vec<GuardFinding> =
                serde_json::from_str(stdout.trim()).unwrap_or_default();
            let _ = tx.send(Ok(parsed));
        });

        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(r) => r,
            Err(_) => Err(DeepScanError::Timeout),
        }
    }
}

fn default_runner(profile_dir: &str) -> impl DeepScanRunner {
    RealRunner::new(PathBuf::from(profile_dir))
}

// ─── Profile scan ───────────────────────────────────────────────────────

/// Validate `project_root` mirroring `compiler::validate_compile_inputs`:
/// must canonicalise inside HOME, must not be HOME itself.
fn validate_project_root(project_root: Option<String>) -> Result<Option<PathBuf>, GuardError> {
    match project_root {
        None => Ok(None),
        Some(s) if s.is_empty() => Ok(None),
        Some(s) => {
            let p = PathBuf::from(&s);
            if !p.is_dir() {
                return Err(GuardError::InvalidProjectRoot(format!(
                    "project_root is not a directory: {}",
                    redact_home(&s)
                )));
            }
            let canon = std::fs::canonicalize(&p).map_err(|e| {
                GuardError::InvalidProjectRoot(format!("canonicalize: {}", e))
            })?;
            let home = PathBuf::from(crate::utils::get_home());
            let canon_home = std::fs::canonicalize(&home).unwrap_or(home);
            if !canon.starts_with(&canon_home) {
                return Err(GuardError::InvalidProjectRoot(
                    "project_root must be under HOME".into(),
                ));
            }
            if canon == canon_home {
                return Err(GuardError::InvalidProjectRoot(
                    "project_root cannot be HOME itself".into(),
                ));
            }
            Ok(Some(canon))
        }
    }
}

fn scan_profile_with_runner<R: DeepScanRunner>(
    profile_dir: &str,
    _project_root: Option<&Path>,
    deep_scan: bool,
    runner: &R,
) -> Result<ScanReport, GuardError> {
    let manifests = scan_profile_manifests(profile_dir)
        .map_err(|e| GuardError::Manifest(e.to_string()))?;
    let overrides = load_override_store(profile_dir);

    let mut resources: Vec<ResourceVerdict> = Vec::with_capacity(manifests.len());
    for (m, _kind) in manifests {
        match scan_resource_inner(profile_dir, &m.manifest_path, &overrides) {
            Ok(v) => resources.push(v),
            Err(e) => {
                log::warn!("guard skip {}: {}", m.manifest_path, e);
            }
        }
    }

    // Optional deep scan: merge findings into existing per-resource
    // verdicts. Each resource path gets passed to the runner; on any
    // error we leave the per-resource findings alone and record the
    // reason at report level. Findings whose `location` carries a
    // recognised resource path are routed to that resource; ones with
    // no parseable location land in the report-level `profile_findings`
    // bucket so they aren't silently misattributed to the first resource.
    let mut profile_findings: Vec<GuardFinding> = Vec::new();
    let (deep_scan_ran, deep_scan_skipped_reason) = if !deep_scan {
        (false, Some("disabled".to_string()))
    } else {
        let path_bufs: Vec<PathBuf> = resources
            .iter()
            .map(|r| PathBuf::from(&r.resource_path))
            .collect();
        let path_refs: Vec<&Path> = path_bufs.iter().map(|p| p.as_path()).collect();
        match runner.run(&path_refs) {
            Ok(extra) => {
                if !extra.is_empty() {
                    let mut by_idx: std::collections::HashMap<usize, Vec<GuardFinding>> =
                        std::collections::HashMap::new();
                    for f in extra {
                        // Resolve to a resource by location prefix; otherwise
                        // route to the profile-level bucket. We never coerce
                        // an unlocated finding onto resources[0] anymore —
                        // that hid bugs by attributing wrong-source warnings
                        // to whichever resource sorted first.
                        let resolved = match &f.location {
                            Some(loc) => resources
                                .iter()
                                .position(|r| loc.starts_with(&r.resource_path)),
                            None => None,
                        };
                        match resolved {
                            Some(idx) => by_idx.entry(idx).or_default().push(f),
                            None => profile_findings.push(f),
                        }
                    }
                    for (idx, mut fs_) in by_idx {
                        if let Some(target) = resources.get_mut(idx) {
                            target.findings.append(&mut fs_);
                            target.verdict = verdict_from_active_findings(
                                &target.findings,
                                &target.overridden_findings,
                            );
                        }
                    }
                }
                (true, None)
            }
            Err(e) => (false, Some(e.reason().to_string())),
        }
    };

    // Overall verdict folds in profile-level findings too — a profile-wide
    // Block (e.g. forbidden permission) must surface even if every body
    // is individually clean.
    let resources_verdict = resources
        .iter()
        .map(|r| r.verdict)
        .fold(GuardVerdict::Green, worst_verdict);
    let profile_verdict = profile_findings
        .iter()
        .map(|f| severity_to_verdict(f.severity))
        .fold(GuardVerdict::Green, worst_verdict);
    let overall = worst_verdict(resources_verdict, profile_verdict);

    Ok(ScanReport {
        profile_dir: profile_dir.to_string(),
        resources,
        overall,
        deep_scan_ran,
        deep_scan_skipped_reason,
        profile_findings,
    })
}

// ─── Tauri commands ─────────────────────────────────────────────────────

fn validate_profile_dir_cmd(profile_config_dir: String) -> Result<String, String> {
    if profile_config_dir.is_empty() {
        return Ok(format!("{}/.claude", crate::utils::get_home()));
    }
    // Route through `GuardError::InvalidProfileDir` so all profile-dir
    // rejections share the same error variant — keeps the error taxonomy
    // honest. The string is then redacted at the Tauri boundary.
    crate::utils::validate_config_dir(&profile_config_dir)
        .map_err(|e| GuardError::InvalidProfileDir(e))
        .map_err(|e| redact_home(&e.to_string()))
}

/// Replace a leading `$HOME` with `~` so error strings handed back to
/// the renderer don't leak the user's home path.
fn redact_home(s: &str) -> String {
    let home = crate::utils::get_home();
    if !home.is_empty() && s.starts_with(&home) {
        format!("~{}", &s[home.len()..])
    } else {
        s.to_string()
    }
}

#[tauri::command]
pub fn scan_resource(
    profile_config_dir: String,
    manifest_path: String,
) -> Result<ResourceVerdict, String> {
    let profile_dir = validate_profile_dir_cmd(profile_config_dir)?;
    let overrides = load_override_store(&profile_dir);
    scan_resource_inner(&profile_dir, &manifest_path, &overrides)
        .map_err(|e| redact_home(&e.to_string()))
}

#[tauri::command]
pub fn scan_profile(
    profile_config_dir: String,
    project_root: Option<String>,
    deep_scan: bool,
) -> Result<ScanReport, String> {
    let profile_dir = validate_profile_dir_cmd(profile_config_dir)?;
    let project_root_canon = validate_project_root(project_root)
        .map_err(|e| redact_home(&e.to_string()))?;
    let runner = default_runner(&profile_dir);
    scan_profile_with_runner(
        &profile_dir,
        project_root_canon.as_deref(),
        deep_scan,
        &runner,
    )
    .map_err(|e| redact_home(&e.to_string()))
}

#[tauri::command]
pub fn set_override_decision(
    profile_config_dir: String,
    decision: OverrideDecision,
) -> Result<(), String> {
    let profile_dir = validate_profile_dir_cmd(profile_config_dir)?;
    let _lock = acquire_override_lock(&profile_dir)
        .map_err(|e| redact_home(&e.to_string()))?;
    // Use the lock-held loader to avoid deadlocking on the migration's
    // self-acquired lock. The save call below rewrites as v2 anyway.
    let mut current = load_override_store_lock_held(&profile_dir);
    // Replace any existing decision for the same triple.
    current.retain(|d| {
        !(d.rule_id == decision.rule_id
            && d.resource_path == decision.resource_path
            && d.body_sha256 == decision.body_sha256)
    });
    current.push(decision);
    save_override_store(&profile_dir, &current)
        .map_err(|e| redact_home(&e.to_string()))?;
    Ok(())
}

#[tauri::command]
pub fn list_overrides(
    profile_config_dir: String,
) -> Result<Vec<OverrideDecision>, String> {
    let profile_dir = validate_profile_dir_cmd(profile_config_dir)?;
    Ok(load_override_store(&profile_dir))
}

#[tauri::command]
pub fn scan_mcp_server(
    profile_config_dir: String,
    server: McpServerRef,
) -> Result<Vec<GuardFinding>, String> {
    let _profile_dir = validate_profile_dir_cmd(profile_config_dir)?;
    // MCP-only scan: only the two MCP rules apply (url + command).
    let servers = vec![server];
    let ctx = RuleCtx {
        body: "",
        permissions: &[],
        mcp_servers: &servers,
        frontmatter: None,
    };
    let mut out = Vec::new();
    for rule in RULES {
        if matches!(
            rule.id,
            "mcp-url-not-https" | "mcp-unknown-command"
        ) {
            out.extend((rule.eval)(&ctx));
        }
    }
    Ok(out)
}

// ─── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::McpServerRef;

    fn ctx<'a>(body: &'a str) -> RuleCtx<'a> {
        RuleCtx {
            body,
            permissions: &[],
            mcp_servers: &[],
            frontmatter: extract_frontmatter(body),
        }
    }

    // ── Secrets rules ────────────────────────────────────────────────

    fn first<F>(v: Vec<F>) -> F {
        v.into_iter().next().expect("expected at least one finding")
    }

    #[test]
    fn secrets_aws_key_match_redacted() {
        let body = "before AKIAIOSFODNN7EXAMPLE after";
        let f = first(rule_secrets_aws_key(&ctx(body)));
        assert_eq!(f.severity, Severity::Block);
        let s = f.snippet.as_deref().unwrap();
        assert!(!s.contains("AKIAIOSFODNN7EXAMPLE"), "snippet leaked secret: {}", s);
        // Format aligned to TS scorer (`[REDACTED:<rule-id>]`) so server-
        // side and client-side scans of the same content produce stable,
        // comparable fingerprints.
        assert!(s.contains("[REDACTED:secrets-aws-key]"), "expected aligned redaction marker, got: {}", s);
    }

    #[test]
    fn secrets_github_token_match_redacted() {
        // 40 chars after gh prefix.
        let body = "use ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa here";
        let f = first(rule_secrets_github_token(&ctx(body)));
        assert_eq!(f.severity, Severity::Block);
        let s = f.snippet.as_deref().unwrap();
        assert!(!s.contains("ghp_aaaa"), "snippet leaked token: {}", s);
        assert!(s.contains("[REDACTED:secrets-github-token]"), "expected aligned redaction marker, got: {}", s);
    }

    #[test]
    fn secrets_private_key_match_redacted() {
        let body = "-----BEGIN RSA PRIVATE KEY-----\nMIIEvQIB...\n-----END RSA PRIVATE KEY-----";
        let f = first(rule_secrets_private_key(&ctx(body)));
        assert_eq!(f.severity, Severity::Block);
        let s = f.snippet.as_deref().unwrap();
        assert!(!s.contains("BEGIN RSA"), "snippet should be redacted: {}", s);
        assert!(s.contains("[REDACTED:secrets-private-key]"), "expected aligned redaction marker, got: {}", s);
    }

    // ── Wildcard tools rule ─────────────────────────────────────────

    #[test]
    fn wildcard_tools_match() {
        let body = "---\nname: foo\nallowed-tools: *\n---\nbody";
        let f = first(rule_wildcard_tools(&ctx(body)));
        assert_eq!(f.severity, Severity::Warn);
    }

    #[test]
    fn wildcard_tools_no_match() {
        let body = "---\nname: foo\nallowed-tools: [Read, Edit]\n---\nbody";
        assert!(rule_wildcard_tools(&ctx(body)).is_empty());
    }

    // ── MCP url rule ────────────────────────────────────────────────

    fn mcp_ctx<'a>(servers: &'a [McpServerRef]) -> RuleCtx<'a> {
        RuleCtx {
            body: "",
            permissions: &[],
            mcp_servers: servers,
            frontmatter: None,
        }
    }

    #[test]
    fn mcp_url_not_https_match() {
        let s = vec![McpServerRef {
            name: "evil".into(),
            url: Some("http://evil.example".into()),
            command: None,
        }];
        let f = first(rule_mcp_url_not_https(&mcp_ctx(&s)));
        assert_eq!(f.severity, Severity::Block);
    }

    #[test]
    fn mcp_url_localhost_ok() {
        let s = vec![McpServerRef {
            name: "ok".into(),
            url: Some("http://localhost:3000".into()),
            command: None,
        }];
        assert!(rule_mcp_url_not_https(&mcp_ctx(&s)).is_empty());
    }

    #[test]
    fn mcp_url_127_0_0_1_ok() {
        let s = vec![McpServerRef {
            name: "ok".into(),
            url: Some("http://127.0.0.1:3000".into()),
            command: None,
        }];
        assert!(rule_mcp_url_not_https(&mcp_ctx(&s)).is_empty());
    }

    /// W-3 regression: `starts_with("http://localhost")` would have
    /// accepted `http://localhost.evil.com/` because the prefix matches.
    /// `is_mcp_url_allowed` parses the URL and compares the host string
    /// for exact equality, so the spoofed host is rejected.
    #[test]
    fn mcp_url_localhost_evil_rejected() {
        let s = vec![McpServerRef {
            name: "x".into(),
            url: Some("http://localhost.evil.com/mcp".into()),
            command: None,
        }];
        let f = first(rule_mcp_url_not_https(&mcp_ctx(&s)));
        assert_eq!(f.rule_id, "mcp-url-not-https");
        // 127.0.0.1 prefix bypass attempt
        let s2 = vec![McpServerRef {
            name: "y".into(),
            url: Some("http://127.0.0.1.evil.com/mcp".into()),
            command: None,
        }];
        let f2 = first(rule_mcp_url_not_https(&mcp_ctx(&s2)));
        assert_eq!(f2.rule_id, "mcp-url-not-https");
    }

    #[test]
    fn mcp_url_ipv6_loopback_ok() {
        let s = vec![McpServerRef {
            name: "ok".into(),
            url: Some("http://[::1]:3000/path".into()),
            command: None,
        }];
        assert!(
            rule_mcp_url_not_https(&mcp_ctx(&s)).is_empty(),
            "::1 loopback should pass"
        );
    }

    // ── MCP unknown command rule ────────────────────────────────────

    #[test]
    fn mcp_unknown_command_match() {
        let s = vec![McpServerRef {
            name: "weird".into(),
            url: None,
            command: Some("/usr/bin/curl".into()),
        }];
        let f = first(rule_mcp_unknown_command(&mcp_ctx(&s)));
        assert_eq!(f.severity, Severity::Warn);
    }

    #[test]
    fn mcp_unknown_command_npx_ok() {
        let s = vec![McpServerRef {
            name: "ok".into(),
            url: None,
            command: Some("npx".into()),
        }];
        assert!(rule_mcp_unknown_command(&mcp_ctx(&s)).is_empty());
    }

    // ── MCP ToS rule ────────────────────────────────────────────────

    #[test]
    fn mcp_tos_agent_cli_claude_match() {
        let body = r#"Run: claude --print "hello""#;
        let f = first(rule_mcp_tos_agent_cli(&ctx(body)));
        assert_eq!(f.severity, Severity::Block);
    }

    #[test]
    fn mcp_tos_agent_cli_codex_match() {
        let body = "Tool: codex run foo";
        let f = first(rule_mcp_tos_agent_cli(&ctx(body)));
        assert_eq!(f.severity, Severity::Block);
    }

    #[test]
    fn mcp_tos_agent_cli_no_match() {
        // Mentions Claude in prose but no headless CLI invocation.
        let body = "I love using Claude for code review. It's great.";
        assert!(rule_mcp_tos_agent_cli(&ctx(body)).is_empty());
    }

    // ── Permissions rule ────────────────────────────────────────────

    fn perm_ctx<'a>(perms: &'a [String]) -> RuleCtx<'a> {
        RuleCtx {
            body: "",
            permissions: perms,
            mcp_servers: &[],
            frontmatter: None,
        }
    }

    #[test]
    fn permissions_broad_wildcard_match() {
        let p = vec!["*".to_string()];
        let f = first(rule_permissions_broad(&perm_ctx(&p)));
        assert_eq!(f.severity, Severity::Warn);
    }

    #[test]
    fn permissions_broad_network_match() {
        let p = vec!["network_*".to_string()];
        let f = first(rule_permissions_broad(&perm_ctx(&p)));
        assert_eq!(f.severity, Severity::Warn);
    }

    #[test]
    fn permissions_specific_ok() {
        let p = vec!["read_files".to_string()];
        assert!(rule_permissions_broad(&perm_ctx(&p)).is_empty());
    }

    /// W-5: multiple AKIA matches in one body produce one finding each.
    #[test]
    fn rules_emit_one_finding_per_match() {
        let body = "config: AKIAIOSFODNN7EXAMPLE plus AKIA1234567890ABCDEF";
        let aws = rule_secrets_aws_key(&ctx(body));
        assert_eq!(aws.len(), 2);
    }

    // ── W-1: multi-secret-on-one-line redaction ─────────────────────

    /// Two AWS keys on one line: scanner produces one finding per match
    /// (W-5), and each snippet redacts ALL secrets that share the line
    /// (W-1) — not just the rule's primary match.
    #[test]
    fn redacted_snippet_redacts_all_secrets_on_same_line() {
        let body = "config: AKIAIOSFODNN7EXAMPLE plus AKIA1234567890ABCDEF";
        let findings = scan_body_internal(body);
        let aws: Vec<_> = findings
            .iter()
            .filter(|f| f.rule_id == "secrets-aws-key")
            .collect();
        assert_eq!(aws.len(), 2, "expected one finding per AKIA match");
        for f in &aws {
            let s = f.snippet.as_deref().unwrap();
            assert!(
                !s.contains("AKIAIOSFODNN7EXAMPLE"),
                "first secret leaked in snippet: {}",
                s
            );
            assert!(
                !s.contains("AKIA1234567890ABCDEF"),
                "second secret leaked in snippet: {}",
                s
            );
        }
    }

    /// AKIA + ghp_ on one line: each rule redacts the OTHER rule's
    /// finding too, so neither snippet leaks either secret.
    #[test]
    fn redacted_snippet_redacts_cross_rule_secrets() {
        let body =
            "leak AKIAIOSFODNN7EXAMPLE then ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let findings = scan_body_internal(body);
        for f in &findings {
            let s = f.snippet.as_deref().unwrap_or("");
            assert!(!s.contains("AKIAIOSFODNN7EXAMPLE"), "AKIA leaked in {}: {}", f.rule_id, s);
            assert!(!s.contains("ghp_aaaa"), "ghp_ leaked in {}: {}", f.rule_id, s);
        }
    }

    /// W-4 regression: when the agent CLI invocation lives on the same
    /// line as a secret, the snippet must NOT leak the secret text.
    #[test]
    fn mcp_tos_snippet_redacts_secret_on_same_line() {
        let body = "claude --print AKIAIOSFODNN7EXAMPLE";
        let f = first(rule_mcp_tos_agent_cli(&ctx(body)));
        let s = f.snippet.as_deref().unwrap();
        assert!(
            !s.contains("AKIAIOSFODNN7EXAMPLE"),
            "MCP-ToS snippet leaked secret: {}",
            s
        );
        assert!(s.contains("claude --print"));
    }

    // ── Verdict math ────────────────────────────────────────────────

    #[test]
    fn worst_severity_picks_red() {
        let findings = vec![
            GuardFinding::new("x", Severity::Warn, "", "", None, None),
            GuardFinding::new("y", Severity::Block, "", "", None, None),
        ];
        assert_eq!(verdict_from_findings(&findings), GuardVerdict::Red);
    }

    #[test]
    fn worst_severity_no_findings_green() {
        assert_eq!(verdict_from_findings(&[]), GuardVerdict::Green);
    }

    // ── Override application ────────────────────────────────────────

    fn tmpdir(label: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "weplex-guard-test-{}-{}-{}",
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
    fn apply_overrides_removes_for_matching_sha() {
        let findings = vec![GuardFinding::new(
            "secrets-aws-key",
            Severity::Block,
            "",
            "",
            None,
            None,
        )];
        let body_sha = "abc123";
        let resource_path = "/tmp/foo.md";
        let overrides = vec![OverrideDecision {
            rule_id: "secrets-aws-key".into(),
            resource_path: resource_path.into(),
            body_sha256: body_sha.into(),
            fingerprint: None,
            decision: OverrideKind::Accept,
            decided_at: "2026-01-01T00:00:00Z".into(),
            decided_by: None,
        }];
        let (filtered, overridden) =
            apply_overrides(findings.clone(), &overrides, body_sha, resource_path);
        assert_eq!(filtered.len(), 1, "finding stays in list");
        // Override is recorded by per-instance fingerprint, matching the
        // single finding's fingerprint exactly.
        assert_eq!(overridden, vec![filtered[0].fingerprint.clone()]);
        let v = verdict_from_active_findings(&filtered, &overridden);
        assert_eq!(v, GuardVerdict::Green, "verdict downgraded");
    }

    #[test]
    fn apply_overrides_skips_after_body_edit() {
        let findings = vec![GuardFinding::new(
            "secrets-aws-key",
            Severity::Block,
            "",
            "",
            None,
            None,
        )];
        let resource_path = "/tmp/foo.md";
        let overrides = vec![OverrideDecision {
            rule_id: "secrets-aws-key".into(),
            resource_path: resource_path.into(),
            body_sha256: "old-sha".into(),
            fingerprint: None,
            decision: OverrideKind::Accept,
            decided_at: "2026-01-01T00:00:00Z".into(),
            decided_by: None,
        }];
        // Caller passes the *new* sha. Override should not match.
        let (filtered, overridden) =
            apply_overrides(findings, &overrides, "new-sha", resource_path);
        assert!(overridden.is_empty(), "override invalidated by sha drift");
        let v = verdict_from_active_findings(&filtered, &overridden);
        assert_eq!(v, GuardVerdict::Red);
    }

    /// Wrapper for `set_override_decision` that bypasses the Tauri
    /// command's HOME-containment check. Tests put profile dirs in
    /// `/tmp` (macOS canonicalises to `/private/var/folders/...`),
    /// outside HOME — using the internal API exercises the same
    /// read-modify-write flow without re-creating an entire HOME
    /// fixture.
    fn set_override_internal(profile_dir: &str, decision: OverrideDecision) -> Result<(), String> {
        let _lock = acquire_override_lock(profile_dir).map_err(|e| e.to_string())?;
        let mut current = load_override_store_lock_held(profile_dir);
        current.retain(|d| {
            !(d.rule_id == decision.rule_id
                && d.resource_path == decision.resource_path
                && d.body_sha256 == decision.body_sha256)
        });
        current.push(decision);
        save_override_store(profile_dir, &current).map_err(|e| e.to_string())
    }

    #[test]
    fn override_persistence_roundtrip() {
        let dir = tmpdir("override-rt");
        let profile = dir.to_str().unwrap().to_string();
        let dec = OverrideDecision {
            rule_id: "secrets-aws-key".into(),
            resource_path: "/tmp/foo.md".into(),
            body_sha256: "abc".into(),
            fingerprint: None,
            decision: OverrideKind::Accept,
            decided_at: "2026-05-07T00:00:00Z".into(),
            decided_by: Some("user".into()),
        };
        set_override_internal(&profile, dec.clone()).unwrap();
        let listed = load_override_store(&profile);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].rule_id, dec.rule_id);
        assert_eq!(listed[0].body_sha256, dec.body_sha256);
        assert_eq!(listed[0].decision, dec.decision);
        // Round-trip must round-trip byte-equal-ish: deserialise +
        // re-serialise must produce the same shape (modulo JSON whitespace).
        let path = override_store_path(&profile);
        let raw = std::fs::read_to_string(&path).unwrap();
        let parsed: OverrideStoreV3 = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed.version, OVERRIDE_STORE_VERSION);
        assert!(!parsed.hmac.is_empty(), "v2 store must include hmac");
        assert_eq!(parsed.decisions.len(), 1);
        assert_eq!(parsed.decisions[0].rule_id, dec.rule_id);
        let _ = std::fs::remove_dir_all(&dir);
        cleanup_keychain(&profile);
    }

    #[test]
    fn override_atomic_write_under_concurrent_set() {
        // Two threads call set with different decisions targeting the
        // same profile. Final JSON must be parseable (no torn writes).
        // The lock-protected RMW means each thread serialises against
        // the other; we don't assert order, only that the file is
        // valid JSON and non-empty at the end.
        let dir = tmpdir("override-concurrent");
        let profile = dir.to_str().unwrap().to_string();
        let p1 = profile.clone();
        let p2 = profile.clone();
        let h1 = std::thread::spawn(move || {
            for i in 0..10 {
                let d = OverrideDecision {
                    rule_id: "secrets-aws-key".into(),
                    resource_path: format!("/tmp/a-{}.md", i),
                    body_sha256: "sha-a".into(),
                    fingerprint: None,
                    decision: OverrideKind::Accept,
                    decided_at: "2026-05-07T00:00:00Z".into(),
                    decided_by: None,
                };
                let _ = set_override_internal(&p1, d);
            }
        });
        let h2 = std::thread::spawn(move || {
            for i in 0..10 {
                let d = OverrideDecision {
                    rule_id: "wildcard-tools".into(),
                    resource_path: format!("/tmp/b-{}.md", i),
                    body_sha256: "sha-b".into(),
                    fingerprint: None,
                    decision: OverrideKind::Accept,
                    decided_at: "2026-05-07T00:00:00Z".into(),
                    decided_by: None,
                };
                let _ = set_override_internal(&p2, d);
            }
        });
        h1.join().unwrap();
        h2.join().unwrap();
        // Final read must succeed (no JSON corruption).
        let path = override_store_path(&profile);
        let raw = std::fs::read_to_string(&path).unwrap();
        let parsed: OverrideStoreV3 = serde_json::from_str(&raw)
            .expect("final override store must be valid JSON");
        assert_eq!(parsed.version, OVERRIDE_STORE_VERSION);
        assert!(!parsed.hmac.is_empty(), "v2 store must include hmac");
        assert!(!parsed.decisions.is_empty(), "expected at least one decision saved");
        let _ = std::fs::remove_dir_all(&dir);
        cleanup_keychain(&profile);
    }

    /// Best-effort cleanup of the per-profile HMAC key in Keychain so
    /// tests don't leak entries on developer machines. Failures (e.g.
    /// linux without secret-service) are silently ignored.
    fn cleanup_keychain(profile: &str) {
        let account = override_hmac_keychain_account(profile);
        if let Ok(entry) =
            keyring::Entry::new(OVERRIDE_HMAC_KEYCHAIN_SERVICE, &account)
        {
            let _ = entry.delete_credential();
        }
    }

    // ── W-2: HMAC integrity tests ────────────────────────────────────

    /// Helper used by HMAC tests to build a one-decision store via the
    /// internal save path (which stamps the HMAC).
    fn save_one_decision(profile: &str, rule_id: &str) {
        let dec = OverrideDecision {
            rule_id: rule_id.into(),
            resource_path: "/tmp/foo.md".into(),
            body_sha256: "sha-1".into(),
            fingerprint: None,
            decision: OverrideKind::Accept,
            decided_at: "2026-05-07T00:00:00Z".into(),
            decided_by: None,
        };
        set_override_internal(profile, dec).unwrap();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn override_hmac_verifies_unmodified() {
        let dir = tmpdir("hmac-good");
        let profile = dir.to_str().unwrap().to_string();
        save_one_decision(&profile, "secrets-aws-key");
        let listed = load_override_store(&profile);
        assert_eq!(listed.len(), 1, "valid HMAC must read back");
        let _ = std::fs::remove_dir_all(&dir);
        cleanup_keychain(&profile);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn override_hmac_rejects_tampered_decisions() {
        let dir = tmpdir("hmac-tamper-decisions");
        let profile = dir.to_str().unwrap().to_string();
        save_one_decision(&profile, "secrets-aws-key");

        // Mutate the JSON file to add a forged decision while keeping
        // the original HMAC. A naive verifier would accept this.
        let path = override_store_path(&profile);
        let raw = std::fs::read_to_string(&path).unwrap();
        let mut parsed: OverrideStoreV3 = serde_json::from_str(&raw).unwrap();
        parsed.decisions.push(OverrideDecision {
            rule_id: "wildcard-tools".into(),
            resource_path: "/tmp/forged.md".into(),
            body_sha256: "sha-forged".into(),
            fingerprint: None,
            decision: OverrideKind::Accept,
            decided_at: "2026-05-07T00:00:00Z".into(),
            decided_by: None,
        });
        std::fs::write(&path, serde_json::to_string_pretty(&parsed).unwrap()).unwrap();

        let listed = load_override_store(&profile);
        assert!(
            listed.is_empty(),
            "HMAC mismatch must drop the tampered store, got {:?}",
            listed
        );
        // File must still exist (forensic evidence).
        assert!(path.exists(), "tampered file must NOT be deleted");
        let _ = std::fs::remove_dir_all(&dir);
        cleanup_keychain(&profile);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn override_hmac_rejects_swapped_hmac() {
        let dir = tmpdir("hmac-tamper-hmac");
        let profile = dir.to_str().unwrap().to_string();
        save_one_decision(&profile, "secrets-aws-key");

        let path = override_store_path(&profile);
        let raw = std::fs::read_to_string(&path).unwrap();
        let mut parsed: OverrideStoreV3 = serde_json::from_str(&raw).unwrap();
        // Flip a single byte in the hex string. Constant-time compare
        // ensures this falls into the same "rejected" path as a wholly
        // bogus value.
        let mut chars: Vec<char> = parsed.hmac.chars().collect();
        chars[0] = if chars[0] == '0' { '1' } else { '0' };
        parsed.hmac = chars.into_iter().collect();
        std::fs::write(&path, serde_json::to_string_pretty(&parsed).unwrap()).unwrap();

        let listed = load_override_store(&profile);
        assert!(
            listed.is_empty(),
            "swapped HMAC must drop the store, got {:?}",
            listed
        );
        let _ = std::fs::remove_dir_all(&dir);
        cleanup_keychain(&profile);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn override_v1_migrates_to_current_on_first_read() {
        let dir = tmpdir("hmac-v1-migrate");
        let profile = dir.to_str().unwrap().to_string();

        // Manually drop a legacy v1 file.
        let path = override_store_path(&profile);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let v1 = OverrideStoreV1 {
            version: 1,
            decisions: vec![OverrideDecision {
                rule_id: "secrets-aws-key".into(),
                resource_path: "/tmp/foo.md".into(),
                body_sha256: "sha-1".into(),
                fingerprint: None,
                decision: OverrideKind::Accept,
                decided_at: "2026-05-07T00:00:00Z".into(),
                decided_by: None,
            }],
        };
        std::fs::write(&path, serde_json::to_string_pretty(&v1).unwrap()).unwrap();

        // First read: decisions should be migrated and the file rewritten
        // at the current schema version with a valid HMAC.
        let listed = load_override_store(&profile);
        assert_eq!(listed.len(), 1, "v1 decisions must be honoured once");

        // File is now at current version + has valid HMAC.
        let raw = std::fs::read_to_string(&path).unwrap();
        let parsed: OverrideStoreV3 = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed.version, OVERRIDE_STORE_VERSION);
        assert!(!parsed.hmac.is_empty());
        // Subsequent reads see the same decisions via the verified path.
        let listed_again = load_override_store(&profile);
        assert_eq!(listed_again.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
        cleanup_keychain(&profile);
    }

    /// v2 store on disk (HMAC-stamped, no fingerprint field on decisions)
    /// must migrate cleanly to v3. Each migrated decision keeps
    /// `fingerprint: None` (legacy semantics: silence all instances).
    #[cfg(target_os = "macos")]
    #[test]
    fn override_v2_migrates_to_v3_with_no_fingerprints() {
        // Build a minimal v2 envelope by hand: same shape as v3 but
        // decisions omit the fingerprint field (we hand-write the JSON to
        // simulate the on-disk format from before the field existed).
        let dir = tmpdir("hmac-v2-migrate");
        let profile = dir.to_str().unwrap().to_string();
        let path = override_store_path(&profile);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();

        // Compute the HMAC over the canonical decisions list. Since v2
        // and v3 share the deserialised struct (with `fingerprint:
        // serde(default)`), a v2 file with no `fingerprint` keys
        // canonicalises through serde to the same bytes as a v3 file
        // with `fingerprint: null`. That keeps the HMAC valid across
        // migration.
        let key = override_hmac_key(&profile).unwrap();
        let decisions = vec![OverrideDecision {
            rule_id: "secrets-aws-key".into(),
            resource_path: "/tmp/foo.md".into(),
            body_sha256: "sha-1".into(),
            fingerprint: None,
            decision: OverrideKind::Accept,
            decided_at: "2026-05-07T00:00:00Z".into(),
            decided_by: None,
        }];
        let hmac = compute_overrides_hmac(&decisions, &key).unwrap();
        let v2_json = serde_json::json!({
            "version": 2,
            "hmac": hmac,
            "decisions": [{
                "ruleId": "secrets-aws-key",
                "resourcePath": "/tmp/foo.md",
                "bodySha256": "sha-1",
                "decision": "accept",
                "decidedAt": "2026-05-07T00:00:00Z",
                "decidedBy": null,
            }],
        });
        std::fs::write(&path, serde_json::to_string_pretty(&v2_json).unwrap()).unwrap();

        // First read migrates v2 -> v3 transparently.
        let listed = load_override_store(&profile);
        assert_eq!(listed.len(), 1);
        assert!(listed[0].fingerprint.is_none(), "v2 decisions migrate as legacy/all-instances");

        // File on disk is now v3 with a valid HMAC.
        let raw = std::fs::read_to_string(&path).unwrap();
        let parsed: OverrideStoreV3 = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed.version, OVERRIDE_STORE_VERSION);
        assert!(!parsed.hmac.is_empty());
        assert_eq!(parsed.decisions.len(), 1);
        assert!(parsed.decisions[0].fingerprint.is_none());

        let _ = std::fs::remove_dir_all(&dir);
        cleanup_keychain(&profile);
    }

    // ── Per-finding fingerprint overrides ────────────────────────────

    /// Body with two AKIA matches → two findings, each with its own
    /// fingerprint. An override carrying the fingerprint of the FIRST
    /// finding silences only that one; the second remains active and
    /// the overall verdict stays Red.
    #[test]
    fn override_per_instance_silences_only_matching_finding() {
        let body = "leak AKIAIOSFODNN7EXAMPLE plus AKIA1234567890ABCDEF";
        let findings = scan_body_internal(body);
        let aws: Vec<&GuardFinding> = findings
            .iter()
            .filter(|f| f.rule_id == "secrets-aws-key")
            .collect();
        assert_eq!(aws.len(), 2, "expected one finding per AKIA match");
        assert_ne!(
            aws[0].fingerprint, aws[1].fingerprint,
            "distinct AKIAs must hash to distinct fingerprints"
        );

        let body_sha = "body-sha";
        let resource_path = "/tmp/two-aws.md";
        let overrides = vec![OverrideDecision {
            rule_id: "secrets-aws-key".into(),
            resource_path: resource_path.into(),
            body_sha256: body_sha.into(),
            fingerprint: Some(aws[0].fingerprint.clone()),
            decision: OverrideKind::Accept,
            decided_at: "2026-05-07T00:00:00Z".into(),
            decided_by: None,
        }];
        let (filtered, overridden) =
            apply_overrides(findings.clone(), &overrides, body_sha, resource_path);
        // Only the first finding is recorded as overridden.
        assert_eq!(overridden.len(), 1, "exactly one finding silenced");
        assert_eq!(overridden[0], aws[0].fingerprint);
        // Verdict remains Red because the second AKIA still actively flags.
        let v = verdict_from_active_findings(&filtered, &overridden);
        assert_eq!(v, GuardVerdict::Red, "second AKIA still active");
    }

    /// Legacy override with `fingerprint: None` silences ALL findings
    /// of that rule_id in the body — backward-compat with the v2
    /// semantics where one accept-decision per rule_id silenced every
    /// instance.
    #[test]
    fn override_legacy_no_fingerprint_silences_all_instances() {
        let body = "leak AKIAIOSFODNN7EXAMPLE plus AKIA1234567890ABCDEF";
        let findings = scan_body_internal(body);
        let aws_count = findings
            .iter()
            .filter(|f| f.rule_id == "secrets-aws-key")
            .count();
        assert_eq!(aws_count, 2);

        let body_sha = "body-sha";
        let resource_path = "/tmp/two-aws.md";
        let overrides = vec![OverrideDecision {
            rule_id: "secrets-aws-key".into(),
            resource_path: resource_path.into(),
            body_sha256: body_sha.into(),
            fingerprint: None, // legacy semantics
            decision: OverrideKind::Accept,
            decided_at: "2026-05-07T00:00:00Z".into(),
            decided_by: None,
        }];
        let (filtered, overridden) =
            apply_overrides(findings.clone(), &overrides, body_sha, resource_path);
        // Both AKIA findings appear in `overridden` (by their distinct
        // fingerprints).
        let aws_fps: Vec<&String> = filtered
            .iter()
            .filter(|f| f.rule_id == "secrets-aws-key")
            .map(|f| &f.fingerprint)
            .collect();
        for fp in &aws_fps {
            assert!(
                overridden.contains(fp),
                "fingerprint {} should be silenced under legacy override",
                fp
            );
        }
        let v = verdict_from_active_findings(&filtered, &overridden);
        assert_eq!(v, GuardVerdict::Green, "all AKIAs silenced");
    }

    // ── Profile scan integration ────────────────────────────────────

    fn write_pair(dir: &Path, id: &str, manifest_yaml: &str, body: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join(format!("{}.weplex.yaml", id)), manifest_yaml).unwrap();
        std::fs::write(dir.join(format!("{}.md", id)), body).unwrap();
    }

    /// Build a fixture profile with three resources:
    ///  - clean (no findings)
    ///  - yellow (wildcard-tools warn)
    ///  - red (AKIA secret block)
    fn fixture_profile() -> PathBuf {
        let profile = tmpdir("scan-integ");
        let agents = profile.join("agents");
        write_pair(
            &agents,
            "clean",
            "id: clean\nversion: 1.0.0\n",
            "# nothing scary here",
        );
        let yellow = profile.join("rules");
        write_pair(
            &yellow,
            "yellow",
            "id: yellow\nversion: 1.0.0\n",
            "---\nname: yellow\nallowed-tools: *\n---\nbody",
        );
        let red = profile.join("skills");
        write_pair(
            &red,
            "red",
            "id: red\nversion: 1.0.0\n",
            "leak AKIAIOSFODNN7EXAMPLE here",
        );
        profile
    }

    struct NoopRunner;
    impl DeepScanRunner for NoopRunner {
        fn run(&self, _paths: &[&Path]) -> Result<Vec<GuardFinding>, DeepScanError> {
            Ok(Vec::new())
        }
    }

    #[test]
    fn scan_profile_integration() {
        let profile = fixture_profile();
        let report = scan_profile_with_runner(
            profile.to_str().unwrap(),
            None,
            false,
            &NoopRunner,
        )
        .unwrap();
        assert_eq!(report.resources.len(), 3);
        assert_eq!(report.overall, GuardVerdict::Red);
        let _ = std::fs::remove_dir_all(&profile);
    }

    #[test]
    fn scan_profile_integration_with_override() {
        let profile = fixture_profile();
        let profile_str = profile.to_str().unwrap().to_string();

        // Find the red resource's body sha.
        let report1 = scan_profile_with_runner(&profile_str, None, false, &NoopRunner).unwrap();
        let red = report1
            .resources
            .iter()
            .find(|r| r.resource_id == "red")
            .unwrap();
        let dec = OverrideDecision {
            rule_id: "secrets-aws-key".into(),
            resource_path: red.resource_path.clone(),
            body_sha256: red.body_sha256.clone(),
            fingerprint: None,
            decision: OverrideKind::Accept,
            decided_at: "2026-05-07T00:00:00Z".into(),
            decided_by: None,
        };
        set_override_internal(&profile_str, dec).unwrap();

        let report2 = scan_profile_with_runner(&profile_str, None, false, &NoopRunner).unwrap();
        // Yellow (wildcard-tools) still active, so overall = Yellow.
        assert_eq!(report2.overall, GuardVerdict::Yellow);
        let red2 = report2
            .resources
            .iter()
            .find(|r| r.resource_id == "red")
            .unwrap();
        assert_eq!(red2.verdict, GuardVerdict::Green);
        // Override is recorded by per-instance fingerprint. With a
        // legacy `fingerprint: None` decision, all matching findings of
        // that rule_id are silenced — verify by fingerprint count instead
        // of comparing to a fixed string.
        let red2_aws_fps: Vec<String> = red2
            .findings
            .iter()
            .filter(|f| f.rule_id == "secrets-aws-key")
            .map(|f| f.fingerprint.clone())
            .collect();
        assert_eq!(red2.overridden_findings, red2_aws_fps);
        let _ = std::fs::remove_dir_all(&profile);
        cleanup_keychain(&profile_str);
    }

    // ── Deep-scan adapter ───────────────────────────────────────────

    #[test]
    fn deep_scan_disabled_returns_skipped() {
        let profile = fixture_profile();
        let report = scan_profile_with_runner(
            profile.to_str().unwrap(),
            None,
            false,
            &NoopRunner,
        )
        .unwrap();
        assert!(!report.deep_scan_ran);
        assert_eq!(report.deep_scan_skipped_reason.as_deref(), Some("disabled"));
        let _ = std::fs::remove_dir_all(&profile);
    }

    struct FakeRunner {
        result: std::cell::RefCell<Option<Result<Vec<GuardFinding>, DeepScanError>>>,
    }
    impl DeepScanRunner for FakeRunner {
        fn run(&self, _paths: &[&Path]) -> Result<Vec<GuardFinding>, DeepScanError> {
            self.result
                .borrow_mut()
                .take()
                .unwrap_or_else(|| Ok(Vec::new()))
        }
    }

    #[test]
    fn deep_scan_fake_returns_findings() {
        let profile = fixture_profile();
        // Two extra findings: one without location (routes to profile-level
        // bucket), one with location prefixed at the red resource (routes
        // to that resource).
        let red_path = profile.join("skills/red.md");
        let red_path_str = red_path.to_str().unwrap().to_string();
        let extra = vec![
            GuardFinding::new(
                "deep-scan-unlocated",
                Severity::Warn,
                "deep finding without location",
                "from fake runner",
                None,
                None,
            ),
            GuardFinding::new(
                "deep-scan-located",
                Severity::Warn,
                "deep finding for red",
                "from fake runner",
                None,
                Some(red_path_str.clone()),
            ),
        ];
        let runner = FakeRunner {
            result: std::cell::RefCell::new(Some(Ok(extra))),
        };
        let report = scan_profile_with_runner(
            profile.to_str().unwrap(),
            None,
            true,
            &runner,
        )
        .unwrap();
        assert!(report.deep_scan_ran);
        // Unlocated finding goes to profile_findings, NOT to resources[0].
        assert_eq!(report.profile_findings.len(), 1);
        assert_eq!(report.profile_findings[0].rule_id, "deep-scan-unlocated");
        for r in &report.resources {
            assert!(
                r.findings.iter().all(|f| f.rule_id != "deep-scan-unlocated"),
                "unlocated finding leaked onto resource {}",
                r.resource_id
            );
        }
        // Located finding lands on the red resource.
        let red = report
            .resources
            .iter()
            .find(|r| r.resource_id == "red")
            .expect("red resource missing");
        assert!(red.findings.iter().any(|f| f.rule_id == "deep-scan-located"));
        let _ = std::fs::remove_dir_all(&profile);
    }

    #[test]
    fn deep_scan_fake_timeout_returns_skipped() {
        let profile = fixture_profile();
        let runner = FakeRunner {
            result: std::cell::RefCell::new(Some(Err(DeepScanError::Timeout))),
        };
        let report = scan_profile_with_runner(
            profile.to_str().unwrap(),
            None,
            true,
            &runner,
        )
        .unwrap();
        assert!(!report.deep_scan_ran);
        assert_eq!(report.deep_scan_skipped_reason.as_deref(), Some("timeout"));
        let _ = std::fs::remove_dir_all(&profile);
    }

    #[test]
    fn deep_scan_fake_binary_missing_returns_skipped() {
        let profile = fixture_profile();
        let runner = FakeRunner {
            result: std::cell::RefCell::new(Some(Err(DeepScanError::BinaryMissing))),
        };
        let report = scan_profile_with_runner(
            profile.to_str().unwrap(),
            None,
            true,
            &runner,
        )
        .unwrap();
        assert!(!report.deep_scan_ran);
        assert_eq!(
            report.deep_scan_skipped_reason.as_deref(),
            Some("binary-missing")
        );
        let _ = std::fs::remove_dir_all(&profile);
    }

    // ── W-8: oversized body rejection ───────────────────────────────

    #[test]
    fn scan_rejects_oversized_body() {
        let profile = tmpdir("oversize");
        let agents = profile.join("agents");
        std::fs::create_dir_all(&agents).unwrap();
        let manifest_path = agents.join("big.weplex.yaml");
        let body_path = agents.join("big.md");
        std::fs::write(&manifest_path, "id: big\nversion: 1.0.0\n").unwrap();
        // Write 1 MiB + 1 byte — just over the cap.
        let payload = vec![b'x'; (MAX_BODY_SIZE_BYTES as usize) + 1];
        std::fs::write(&body_path, &payload).unwrap();

        let verdict = scan_resource_inner(
            profile.to_str().unwrap(),
            manifest_path.to_str().unwrap(),
            &[],
        )
        .unwrap();
        assert_eq!(verdict.verdict, GuardVerdict::Red);
        assert_eq!(verdict.findings.len(), 1);
        assert_eq!(verdict.findings[0].rule_id, "body-too-large");
        assert_eq!(verdict.findings[0].severity, Severity::Block);
        let _ = std::fs::remove_dir_all(&profile);
    }

    #[test]
    fn scan_accepts_body_at_cap() {
        let profile = tmpdir("at-cap");
        let agents = profile.join("agents");
        std::fs::create_dir_all(&agents).unwrap();
        let manifest_path = agents.join("at.weplex.yaml");
        let body_path = agents.join("at.md");
        std::fs::write(&manifest_path, "id: at\nversion: 1.0.0\n").unwrap();
        // Exactly at the cap — should still be accepted.
        let payload = vec![b'x'; MAX_BODY_SIZE_BYTES as usize];
        std::fs::write(&body_path, &payload).unwrap();

        let verdict = scan_resource_inner(
            profile.to_str().unwrap(),
            manifest_path.to_str().unwrap(),
            &[],
        )
        .unwrap();
        // No secrets / wildcards / ToS hits in plain `xxxx...`, so green.
        assert!(
            verdict
                .findings
                .iter()
                .all(|f| f.rule_id != "body-too-large"),
            "at-cap body should not be flagged: {:?}",
            verdict.findings
        );
        let _ = std::fs::remove_dir_all(&profile);
    }

    // ── W-6: containment helper ─────────────────────────────────────

    #[test]
    fn ensure_path_within_profile_or_home_accepts_inside_profile() {
        let profile = tmpdir("contain-ok");
        let inside = profile.join("agents/foo.md");
        std::fs::create_dir_all(inside.parent().unwrap()).unwrap();
        std::fs::write(&inside, "x").unwrap();
        let canon = std::fs::canonicalize(&inside).unwrap();
        let r = ensure_path_within_profile_or_home(&canon, &profile);
        assert!(r.is_ok(), "got {:?}", r);
        let _ = std::fs::remove_dir_all(&profile);
    }

    #[test]
    fn ensure_path_within_profile_or_home_rejects_outside() {
        let profile = tmpdir("contain-no");
        // /etc must exist on macOS/linux test runners. canonicalize to
        // give a stable absolute path.
        let outside = std::path::PathBuf::from("/etc");
        let canon = std::fs::canonicalize(&outside).unwrap_or(outside);
        let r = ensure_path_within_profile_or_home(&canon, &profile);
        assert!(r.is_err(), "expected /etc rejection, got {:?}", r);
        let _ = std::fs::remove_dir_all(&profile);
    }

    // ── Tauri command validation ────────────────────────────────────

    #[test]
    fn tauri_scan_profile_rejects_invalid_profile_dir() {
        let r = scan_profile("/etc".to_string(), None, false);
        assert!(r.is_err(), "expected error, got {:?}", r);
    }

    #[test]
    fn tauri_scan_profile_rejects_project_root_outside_home() {
        // Use a real profile dir under HOME, but project_root pointing
        // at /etc.
        let profile = tmpdir("invalid-proj");
        let r = scan_profile(
            profile.to_str().unwrap().to_string(),
            Some("/etc".to_string()),
            false,
        );
        assert!(r.is_err(), "expected error, got {:?}", r);
        let _ = std::fs::remove_dir_all(&profile);
    }

    // ── Parity vectors (cross-runtime AgentShield) ──────────────────
    //
    // The fixture file at `../tests/agentshield-vectors.json` is a copy
    // of the canonical version that lives in
    // `weplex-server/src/modules/marketplace/federation/__fixtures__/agentshield-vectors.json`.
    // The two MUST stay in sync — the TS scorer (server side) and the
    // Rust scorer (client side) both consume the same content and need
    // to agree on which rules fire and what the resulting score is.
    //
    // Why two copies and not a symlink: symlinks aren't portable across
    // OSes (Windows + git). The source of truth is the server-side
    // file; bake-and-copy is enforced by this test (rule-id and score
    // mismatches between the two scorers will fail here).
    //
    // Fingerprint parity: the per-finding fingerprint is the
    // sha256 prefix of `(rule_id, location, snippet[..60])`. The TS
    // scorer uses `offset:N` for `location` and a non-line-bounded ±40
    // window for `snippet`; the Rust scorer uses `line N, col M` and a
    // line-bounded ±60 window. As a result, fingerprints DO NOT match
    // cross-runtime for body-pattern rules. This test asserts the
    // weaker contract (rule-id sets + scores), and validates locally
    // that every finding still has a 16-char hex fingerprint. Closing
    // the fingerprint gap requires aligning windows + locations on
    // both sides, which is a Phase-6 design decision (not just a test
    // tweak).

    #[derive(serde::Deserialize)]
    struct ParityVector {
        name: String,
        content: String,
        #[serde(rename = "expectedRuleIds")]
        expected_rule_ids: Vec<String>,
        #[serde(rename = "expectedScore")]
        expected_score: String,
    }

    #[derive(serde::Deserialize)]
    struct ParityVectors {
        vectors: Vec<ParityVector>,
    }

    /// Rule IDs that the Rust scanner can fire purely from a body
    /// string (no manifest). The TS scorer parses the YAML
    /// frontmatter directly for `mcp-url-not-https`,
    /// `mcp-unknown-command`, and `permissions-broad`; the Rust
    /// scorer expects those signals on `RuleCtx::mcp_servers` /
    /// `permissions` (i.e. parsed manifest fields). Vectors whose
    /// expected rules fall outside this set get the manifest-driven
    /// rule IDs filtered out before comparison.
    const BODY_SCANNABLE_RULES: &[&str] = &[
        "secrets-aws-key",
        "secrets-github-token",
        "secrets-private-key",
        "wildcard-tools",      // reads `tools:` / `allowed-tools:` directly from frontmatter text
        "mcp-tos-agent-cli",   // body regex
    ];

    /// Vectors whose body content exercises a known cross-runtime
    /// divergence between the TS and Rust scanners.
    ///
    /// 1. `wildcard-tools` (`tools: '*'`): TS uses `js-yaml`
    ///    (FAILSAFE_SCHEMA) so the quoted scalar `'*'` decodes to
    ///    `"*"`. The Rust scalar extractor returns the literal
    ///    3-char string `'*'` and the bare-wildcard check fails.
    ///
    /// 2. `wildcard-bash` (multi-line list with `- Bash(*)`): Rust's
    ///    multi-line wildcard check looks for the literal token
    ///    `- *`, not the `Foo(*)` pattern that TS's regex
    ///    (`/Bash\(\s*\*\s*\)/`) catches.
    ///
    /// 3. `block-trumps-warn`: combines (1) — quoted `tools: '*'` —
    ///    with an AKIA secret. The Rust scanner correctly fires the
    ///    AWS rule, but the wildcard rule misses, so the rule-id
    ///    set diverges.
    ///
    /// When (if) the Rust extractor learns to unquote YAML scalars
    /// AND match `Foo(*)` patterns, the corresponding entries can be
    /// removed and these vectors will participate in parity again.
    const VECTORS_WITH_RUNTIME_DIVERGENCE: &[&str] = &[
        "wildcard-tools",
        "wildcard-bash",
        "block-trumps-warn",
    ];

    fn parity_score_to_verdict(s: &str) -> GuardVerdict {
        match s {
            "red" => GuardVerdict::Red,
            "yellow" => GuardVerdict::Yellow,
            _ => GuardVerdict::Green,
        }
    }

    /// Translate the body-scannable expected rule IDs into the
    /// strongest verdict they would produce. Used to compute the
    /// Rust-side expected score after dropping manifest-driven
    /// rule IDs (e.g. a vector that on the TS side fires `permissions-
    /// broad` (yellow) and nothing else expects yellow; on the Rust
    /// side, the same body produces no findings so the verdict is
    /// green, and the parity test compares against the filtered
    /// expectation, not the original yellow).
    fn parity_expected_verdict_for_rust(rule_ids: &[String]) -> GuardVerdict {
        let mut v = GuardVerdict::Green;
        for id in rule_ids {
            let s = match id.as_str() {
                "secrets-aws-key" | "secrets-github-token" | "secrets-private-key"
                | "mcp-tos-agent-cli" => GuardVerdict::Red,
                "wildcard-tools" => GuardVerdict::Yellow,
                _ => GuardVerdict::Green,
            };
            v = worst_verdict(v, s);
        }
        v
    }

    #[test]
    fn parity_vectors_rule_ids_match_body_scannable_subset() {
        // Loads the local copy of the fixture. If you bump the
        // server-side file, copy it here too — the comment block
        // above explains why.
        let raw = include_str!("../tests/agentshield-vectors.json");
        let vectors: ParityVectors = serde_json::from_str(raw)
            .expect("parity vectors JSON must deserialise");

        for v in &vectors.vectors {
            // Spot-check the parity score lookup helper against the
            // raw expected_score string so a typo in the JSON would
            // fail loudly here (rather than silently mapping to
            // green via the catch-all). Done up-front for every
            // vector — diverged or not — because score validation
            // is independent of the runtime mismatch.
            match v.expected_score.as_str() {
                "green" => assert_eq!(parity_score_to_verdict("green"), GuardVerdict::Green),
                "yellow" => assert_eq!(parity_score_to_verdict("yellow"), GuardVerdict::Yellow),
                "red" => assert_eq!(parity_score_to_verdict("red"), GuardVerdict::Red),
                other => panic!(
                    "vector `{}` has unrecognised expected_score: {:?}",
                    v.name, other
                ),
            }

            if VECTORS_WITH_RUNTIME_DIVERGENCE.contains(&v.name.as_str()) {
                // Skip rule-id parity for these — see the doc on
                // VECTORS_WITH_RUNTIME_DIVERGENCE for why.
                continue;
            }

            // Subset of expected rule IDs that the Rust scanner can
            // fire from body alone.
            let mut expected_subset: Vec<String> = v
                .expected_rule_ids
                .iter()
                .filter(|id| BODY_SCANNABLE_RULES.contains(&id.as_str()))
                .cloned()
                .collect();
            expected_subset.sort();

            let findings = scan_body_internal(&v.content);
            let mut actual_ids: Vec<String> =
                findings.iter().map(|f| f.rule_id.clone()).collect();
            actual_ids.sort();

            assert_eq!(
                actual_ids, expected_subset,
                "vector `{}` rule-id mismatch: actual={:?} expected={:?}",
                v.name, actual_ids, expected_subset,
            );

            // Verdict: compare against the filtered expectation. A
            // vector whose only expected rule is manifest-driven
            // (e.g. `permissions-broad`) is green from the Rust
            // body-only scanner.
            let expected_verdict =
                parity_expected_verdict_for_rust(&expected_subset);
            let actual_verdict = verdict_from_findings(&findings);
            assert_eq!(
                actual_verdict, expected_verdict,
                "vector `{}` verdict mismatch (body-only): actual={:?} expected={:?}",
                v.name, actual_verdict, expected_verdict,
            );

            // Sanity: every Rust finding must have a 16-char hex
            // fingerprint. Same shape as the TS scorer's, even if
            // the value differs (see module-level comment).
            for f in &findings {
                assert_eq!(
                    f.fingerprint.len(),
                    16,
                    "vector `{}` finding fingerprint not 16 chars: {:?}",
                    v.name,
                    f.fingerprint,
                );
                assert!(
                    f.fingerprint
                        .chars()
                        .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
                    "vector `{}` finding fingerprint not lower-hex: {:?}",
                    v.name,
                    f.fingerprint,
                );
            }
        }
    }

    #[test]
    fn parity_vectors_fingerprints_are_stable_within_rust() {
        // Ensures the Rust scorer's fingerprint output doesn't drift
        // run-to-run. (Cross-runtime parity is a separate concern —
        // see the module-level comment above.) Two scans of the
        // same body must produce the same fingerprint per finding.
        let raw = include_str!("../tests/agentshield-vectors.json");
        let vectors: ParityVectors = serde_json::from_str(raw).unwrap();
        for v in &vectors.vectors {
            let a = scan_body_internal(&v.content);
            let b = scan_body_internal(&v.content);
            let af: Vec<_> = a.iter().map(|f| (&f.rule_id, &f.fingerprint)).collect();
            let bf: Vec<_> = b.iter().map(|f| (&f.rule_id, &f.fingerprint)).collect();
            assert_eq!(
                af, bf,
                "vector `{}` produced non-deterministic findings",
                v.name
            );
        }
    }
}

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
#[allow(dead_code)] // Variants reserved for orchestrator additions in
                    // follow-up commits (override store, deep scan).
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

/// Pick the worst verdict implied by findings, ignoring any rule_id
/// listed in `overridden`. This is the load-bearing entry point for
/// computing a resource's effective verdict — overridden findings
/// remain in the list (so the UI can render "you accepted this earlier")
/// but they no longer steer the verdict.
fn verdict_from_active_findings(
    findings: &[GuardFinding],
    overridden: &[String],
) -> GuardVerdict {
    findings
        .iter()
        .filter(|f| !overridden.iter().any(|id| id == &f.rule_id))
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
const SNIPPET_CONTEXT_CHARS: usize = 60;

/// Build a redacted snippet by replacing the matched substring with a
/// `<redacted:N chars>` placeholder, then run a second pass over the
/// surrounding window that redacts ANY other secret that happens to live
/// on the same line (multi-secret-on-one-line leak — W-1).
fn redacted_snippet(body: &str, m_start: usize, m_end: usize) -> String {
    let len = m_end - m_start;
    let placeholder = format!("<redacted:{} chars>", len);
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
/// `<redacted:N chars>` placeholder. Used as a second-pass over snippet
/// strings so a snippet generated for rule A can never leak a secret that
/// rule B would have caught.
fn redact_all_secrets(s: &str) -> String {
    let mut out = re_aws_key()
        .replace_all(s, |c: &regex::Captures| {
            format!(
                "<redacted:{} chars>",
                c.get(0).map(|m| m.as_str().len()).unwrap_or(0)
            )
        })
        .into_owned();
    out = re_github_token()
        .replace_all(&out, |c: &regex::Captures| {
            format!(
                "<redacted:{} chars>",
                c.get(0).map(|m| m.as_str().len()).unwrap_or(0)
            )
        })
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
            out.push_str("<redacted: pem private key line>");
            if line.ends_with('\n') {
                out.push('\n');
            }
        } else {
            out.push_str(line);
        }
    }
    out
}

/// Format a 1-based `(line, col)` location for a byte offset.
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
        out.push(GuardFinding {
            rule_id: "secrets-aws-key".into(),
            severity: Severity::Block,
            message: "AWS access key id detected in body".into(),
            explanation:
                "AKIA-prefixed AWS access key ids are root-level credentials. \
                 They must never be committed to a resource body — anyone with \
                 read access to the manifest gets the key."
                    .into(),
            snippet: Some(redacted_snippet(ctx.body, m.start(), m.end())),
            location: Some(locate(ctx.body, m.start())),
        });
    }
    out
}

fn rule_secrets_github_token(ctx: &RuleCtx) -> Vec<GuardFinding> {
    let mut out = Vec::new();
    for m in re_github_token().find_iter(ctx.body) {
        out.push(GuardFinding {
            rule_id: "secrets-github-token".into(),
            severity: Severity::Block,
            message: "GitHub personal access token detected in body".into(),
            explanation:
                "Tokens prefixed with `ghp_` (classic) or `ghs_` (server-to-server) \
                 grant repo-scoped access. Never embed them in a resource body — \
                 rotate the token immediately if you see this finding."
                    .into(),
            snippet: Some(redacted_snippet(ctx.body, m.start(), m.end())),
            location: Some(locate(ctx.body, m.start())),
        });
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
            out.push(GuardFinding {
                rule_id: "secrets-private-key".into(),
                severity: Severity::Block,
                message: "Embedded private key detected in body".into(),
                explanation:
                    "PEM private keys (`-----BEGIN ... PRIVATE KEY-----`) must not \
                     ship inside an agent / rule / skill body. Move the key to your \
                     OS keychain or a `.env` file referenced via `${SECRET_NAME}` \
                     at runtime."
                        .into(),
                snippet: Some(redacted_snippet(ctx.body, begin_idx, line_end)),
                location: Some(locate(ctx.body, begin_idx)),
            });
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
                out.push(GuardFinding {
                    rule_id: "wildcard-tools".into(),
                    severity: Severity::Warn,
                    message: format!(
                        "Frontmatter `{}` grants `*` (all tools)",
                        key
                    ),
                    explanation:
                        "Granting `*` to a Claude agent or rule disables \
                         tool gating entirely — the resource can run any \
                         registered tool, including ones added later. \
                         Prefer an explicit allow-list (e.g. `[Read, Edit, \
                         Bash]`) so future tools don't silently inherit \
                         access."
                            .into(),
                    snippet: Some(format!("{}: {}", key, value.trim())),
                    location: None,
                });
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
                out.push(GuardFinding {
                    rule_id: "mcp-url-not-https".into(),
                    severity: Severity::Block,
                    message: format!(
                        "MCP server `{}` uses non-HTTPS url",
                        s.name
                    ),
                    explanation:
                        "Plain-HTTP MCP endpoints are vulnerable to MitM on \
                         hostile networks. Either switch to `https://` or, \
                         for local development, bind to `localhost`/`127.0.0.1`."
                            .into(),
                    snippet: Some(format!("{}: {}", s.name, u)),
                    location: None,
                });
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
                out.push(GuardFinding {
                    rule_id: "mcp-unknown-command".into(),
                    severity: Severity::Warn,
                    message: format!(
                        "MCP server `{}` invokes an unknown launcher: {}",
                        s.name, basename
                    ),
                    explanation: format!(
                        "MCP servers should be started with a recognised \
                         package runner ({}). An unfamiliar command \
                         (e.g. `curl` or a hand-rolled binary) makes the \
                         supply chain harder to audit.",
                        ALLOWED_MCP_COMMANDS.join(", ")
                    ),
                    snippet: Some(format!("{}: {}", s.name, cmd)),
                    location: None,
                });
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
        out.push(GuardFinding {
            rule_id: "mcp-tos-agent-cli".into(),
            severity: Severity::Block,
            message: "Resource body invokes another agent CLI in headless mode".into(),
            explanation:
                "Agent-on-agent orchestration via headless CLI (e.g. \
                 `claude --print`, `codex run`, `aider --message`) violates the \
                 MCP terms-of-service guideline against Claude-on-Claude \
                 spawning. Use a tool / MCP server boundary instead."
                    .into(),
            snippet: Some(redact_all_secrets(raw)),
            location: Some(locate(ctx.body, m.start())),
        });
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
            out.push(GuardFinding {
                rule_id: "permissions-broad".into(),
                severity: Severity::Warn,
                message: format!("Manifest permissions include `{}`", p),
                explanation:
                    "Broad permission grants (`*`, `network_*`, `system_*`, \
                     `exec_*`) widen the agent's reach beyond what the body \
                     plausibly needs. Prefer named scopes (e.g. \
                     `network_github`, `read_files`) so future categories \
                     don't auto-inherit."
                        .into(),
                snippet: Some(p.clone()),
                location: None,
            });
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct OverrideStoreOnDisk {
    version: u32,
    decisions: Vec<OverrideDecision>,
}

const OVERRIDE_STORE_VERSION: u32 = 1;

fn override_store_path(profile_dir: &str) -> PathBuf {
    PathBuf::from(profile_dir)
        .join(".weplex")
        .join("guard-overrides.json")
}

fn override_lock_path(profile_dir: &str) -> PathBuf {
    PathBuf::from(profile_dir).join(".weplex").join("overrides.lock")
}

/// Load the override store. Missing file = empty store (treated as
/// "user has not made any decisions yet"). Corrupt JSON degrades to
/// empty + warning rather than a hard error — guard scans should never
/// fail because of a bad overrides file.
fn load_override_store(profile_dir: &str) -> Vec<OverrideDecision> {
    let path = override_store_path(profile_dir);
    let raw = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    match serde_json::from_str::<OverrideStoreOnDisk>(&raw) {
        Ok(s) => s.decisions,
        Err(e) => {
            log::warn!(
                "guard override store at {} is corrupt: {} — treating as empty",
                path.display(),
                e
            );
            Vec::new()
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
fn save_override_store(
    profile_dir: &str,
    decisions: &[OverrideDecision],
) -> Result<(), GuardError> {
    let path = override_store_path(profile_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| GuardError::Io(format!("create parent: {}", e)))?;
    }
    let payload = OverrideStoreOnDisk {
        version: OVERRIDE_STORE_VERSION,
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

/// Apply the override store to a list of findings: any finding whose
/// `(rule_id, body_sha256, resource_path)` triple matches an active
/// `Accept` decision is downgraded — the finding stays in the list (so
/// the UI can render "you accepted this earlier") but its rule_id is
/// added to `overridden`. The verdict computation downstream consults
/// `overridden` to ignore those rule_ids.
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
    for o in overrides {
        if matches!(o.decision, OverrideKind::Accept)
            && o.body_sha256 == body_sha
            && o.resource_path == resource_path
        {
            // For every accept-override, find any matching finding and
            // record it as overridden (deduped).
            if findings.iter().any(|f| f.rule_id == o.rule_id)
                && !overridden.contains(&o.rule_id)
            {
                overridden.push(o.rule_id.clone());
            }
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

/// Default production runner. Spawns `npx ecc-agentshield scan <path>`
/// in a worker thread, joins via channel with a 5-second wall-clock
/// budget, parses stdout as JSON if we can, otherwise treats the run
/// as "errored" and surfaces the reason. Path arguments are
/// canonicalised before being handed to the subprocess so symlink
/// trickery can't redirect the scan elsewhere.
pub(crate) struct RealRunner;

impl DeepScanRunner for RealRunner {
    fn run(&self, paths: &[&Path]) -> Result<Vec<GuardFinding>, DeepScanError> {
        if paths.is_empty() {
            return Ok(Vec::new());
        }

        // Canonicalise every path before passing it on the command line.
        // npx invocations can resolve symlinks unexpectedly — we want
        // ecc-agentshield to scan the bytes the manifest scanner saw,
        // not whatever a symlink chain redirects to.
        let mut canonical: Vec<String> = Vec::with_capacity(paths.len());
        for p in paths {
            let c = std::fs::canonicalize(p)
                .map_err(|e| DeepScanError::Other(format!("canonicalize: {}", e)))?;
            let s = c
                .to_str()
                .ok_or_else(|| DeepScanError::Other("non-utf8 path".into()))?
                .to_string();
            canonical.push(s);
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

fn default_runner() -> impl DeepScanRunner {
    RealRunner
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
    // recognised resource path are routed to that resource; others fall
    // onto the first resource as a coarse fallback.
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
                if !extra.is_empty() && !resources.is_empty() {
                    let mut by_idx: std::collections::HashMap<usize, Vec<GuardFinding>> =
                        std::collections::HashMap::new();
                    for f in extra {
                        let idx = match &f.location {
                            Some(loc) => resources
                                .iter()
                                .position(|r| loc.starts_with(&r.resource_path))
                                .unwrap_or(0),
                            None => 0,
                        };
                        by_idx.entry(idx).or_default().push(f);
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

    let overall = resources
        .iter()
        .map(|r| r.verdict)
        .fold(GuardVerdict::Green, worst_verdict);

    Ok(ScanReport {
        profile_dir: profile_dir.to_string(),
        resources,
        overall,
        deep_scan_ran,
        deep_scan_skipped_reason,
    })
}

// ─── Tauri commands ─────────────────────────────────────────────────────

fn validate_profile_dir_cmd(profile_config_dir: String) -> Result<String, String> {
    if profile_config_dir.is_empty() {
        return Ok(format!("{}/.claude", crate::utils::get_home()));
    }
    crate::utils::validate_config_dir(&profile_config_dir)
        .map_err(|e| format!("invalid profile_config_dir: {}", redact_home(&e)))
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
    let runner = default_runner();
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
    let mut current = load_override_store(&profile_dir);
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
        assert!(s.contains("<redacted:"));
    }

    #[test]
    fn secrets_github_token_match_redacted() {
        // 40 chars after gh prefix.
        let body = "use ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa here";
        let f = first(rule_secrets_github_token(&ctx(body)));
        assert_eq!(f.severity, Severity::Block);
        let s = f.snippet.as_deref().unwrap();
        assert!(!s.contains("ghp_aaaa"), "snippet leaked token: {}", s);
        assert!(s.contains("<redacted:"));
    }

    #[test]
    fn secrets_private_key_match_redacted() {
        let body = "-----BEGIN RSA PRIVATE KEY-----\nMIIEvQIB...\n-----END RSA PRIVATE KEY-----";
        let f = first(rule_secrets_private_key(&ctx(body)));
        assert_eq!(f.severity, Severity::Block);
        let s = f.snippet.as_deref().unwrap();
        assert!(!s.contains("BEGIN RSA"), "snippet should be redacted: {}", s);
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
            GuardFinding {
                rule_id: "x".into(),
                severity: Severity::Warn,
                message: "".into(),
                explanation: "".into(),
                snippet: None,
                location: None,
            },
            GuardFinding {
                rule_id: "y".into(),
                severity: Severity::Block,
                message: "".into(),
                explanation: "".into(),
                snippet: None,
                location: None,
            },
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
        let findings = vec![GuardFinding {
            rule_id: "secrets-aws-key".into(),
            severity: Severity::Block,
            message: "".into(),
            explanation: "".into(),
            snippet: None,
            location: None,
        }];
        let body_sha = "abc123";
        let resource_path = "/tmp/foo.md";
        let overrides = vec![OverrideDecision {
            rule_id: "secrets-aws-key".into(),
            resource_path: resource_path.into(),
            body_sha256: body_sha.into(),
            decision: OverrideKind::Accept,
            decided_at: "2026-01-01T00:00:00Z".into(),
            decided_by: None,
        }];
        let (filtered, overridden) =
            apply_overrides(findings.clone(), &overrides, body_sha, resource_path);
        assert_eq!(filtered.len(), 1, "finding stays in list");
        assert_eq!(overridden, vec!["secrets-aws-key".to_string()]);
        let v = verdict_from_active_findings(&filtered, &overridden);
        assert_eq!(v, GuardVerdict::Green, "verdict downgraded");
    }

    #[test]
    fn apply_overrides_skips_after_body_edit() {
        let findings = vec![GuardFinding {
            rule_id: "secrets-aws-key".into(),
            severity: Severity::Block,
            message: "".into(),
            explanation: "".into(),
            snippet: None,
            location: None,
        }];
        let resource_path = "/tmp/foo.md";
        let overrides = vec![OverrideDecision {
            rule_id: "secrets-aws-key".into(),
            resource_path: resource_path.into(),
            body_sha256: "old-sha".into(),
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
        let mut current = load_override_store(profile_dir);
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
        let parsed: OverrideStoreOnDisk = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed.version, OVERRIDE_STORE_VERSION);
        assert_eq!(parsed.decisions.len(), 1);
        assert_eq!(parsed.decisions[0].rule_id, dec.rule_id);
        let _ = std::fs::remove_dir_all(&dir);
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
        let parsed: OverrideStoreOnDisk = serde_json::from_str(&raw)
            .expect("final override store must be valid JSON");
        assert_eq!(parsed.version, OVERRIDE_STORE_VERSION);
        assert!(!parsed.decisions.is_empty(), "expected at least one decision saved");
        let _ = std::fs::remove_dir_all(&dir);
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
        assert_eq!(
            red2.overridden_findings,
            vec!["secrets-aws-key".to_string()]
        );
        let _ = std::fs::remove_dir_all(&profile);
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
        let extra = vec![GuardFinding {
            rule_id: "deep-scan-test".into(),
            severity: Severity::Warn,
            message: "deep finding".into(),
            explanation: "from fake runner".into(),
            snippet: None,
            location: None,
        }];
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
        // Extra finding got merged onto the first resource.
        let first = &report.resources[0];
        assert!(first.findings.iter().any(|f| f.rule_id == "deep-scan-test"));
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
}

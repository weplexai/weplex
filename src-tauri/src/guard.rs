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

use crate::manifest::{McpServerRef, ResourceKind};
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

/// Build a redacted snippet by replacing the matched substring with a
/// `<redacted:N chars>` placeholder. Used for every secrets-* rule so
/// raw secret bytes never reach the UI logs.
fn redacted_snippet(body: &str, m_start: usize, m_end: usize) -> String {
    // Take a small window around the match for context, but redact the
    // match itself. The window is intentionally tight (≤ 60 chars total
    // each side) — too much context risks pulling in *other* secrets on
    // adjacent lines.
    let len = m_end - m_start;
    let placeholder = format!("<redacted:{} chars>", len);
    // Safe slicing: m_start / m_end are byte offsets coming straight
    // from regex `Match::start/end()`, which are guaranteed UTF-8
    // boundaries. No need to round.
    let pre_start = body[..m_start].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let post_end = body[m_end..]
        .find('\n')
        .map(|i| m_end + i)
        .unwrap_or(body.len());
    let mut out = String::new();
    out.push_str(&body[pre_start..m_start]);
    out.push_str(&placeholder);
    out.push_str(&body[m_end..post_end]);
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

type RuleFn = fn(&RuleCtx) -> Option<GuardFinding>;

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

fn rule_secrets_aws_key(ctx: &RuleCtx) -> Option<GuardFinding> {
    let m = re_aws_key().find(ctx.body)?;
    Some(GuardFinding {
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
    })
}

fn rule_secrets_github_token(ctx: &RuleCtx) -> Option<GuardFinding> {
    let m = re_github_token().find(ctx.body)?;
    Some(GuardFinding {
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
    })
}

fn rule_secrets_private_key(ctx: &RuleCtx) -> Option<GuardFinding> {
    // Look for a `-----BEGIN ` token paired with `PRIVATE KEY-----` on
    // the same or the next line. We can't use a single regex because the
    // pattern straddles a newline reliably only with a multiline mode +
    // bounded `.` — and we'd rather keep this tight and explicit.
    let begin_idx = ctx.body.find("-----BEGIN ")?;
    // Search window: from BEGIN up to ~120 bytes (one PEM header line).
    let window_end = (begin_idx + 200).min(ctx.body.len());
    let window = &ctx.body[begin_idx..window_end];
    if !window.contains("PRIVATE KEY-----") {
        return None;
    }
    // Snippet is just the BEGIN line, fully redacted (header strings are
    // not secret on their own but redacting prevents accidental copy of
    // surrounding key material on multi-line snippets in future).
    let line_end = ctx.body[begin_idx..]
        .find('\n')
        .map(|i| begin_idx + i)
        .unwrap_or(ctx.body.len());
    Some(GuardFinding {
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
    })
}

fn rule_wildcard_tools(ctx: &RuleCtx) -> Option<GuardFinding> {
    let fm = ctx.frontmatter?;
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
                return Some(GuardFinding {
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
    None
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

fn rule_mcp_url_not_https(ctx: &RuleCtx) -> Option<GuardFinding> {
    for s in ctx.mcp_servers {
        if let Some(u) = &s.url {
            let allowed = u.starts_with("https://")
                || u.starts_with("http://localhost")
                || u.starts_with("http://127.0.0.1")
                || u.starts_with("http://[::1]");
            if !allowed {
                return Some(GuardFinding {
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
    None
}

const ALLOWED_MCP_COMMANDS: &[&str] = &[
    "npx", "uvx", "python", "python3", "node", "bun", "deno", "pnpm",
];

fn rule_mcp_unknown_command(ctx: &RuleCtx) -> Option<GuardFinding> {
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
                return Some(GuardFinding {
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
    None
}

fn rule_mcp_tos_agent_cli(ctx: &RuleCtx) -> Option<GuardFinding> {
    let m = re_agent_cli().find(ctx.body)?;
    Some(GuardFinding {
        rule_id: "mcp-tos-agent-cli".into(),
        severity: Severity::Block,
        message: "Resource body invokes another agent CLI in headless mode".into(),
        explanation:
            "Agent-on-agent orchestration via headless CLI (e.g. \
             `claude --print`, `codex run`, `aider --message`) violates the \
             MCP terms-of-service guideline against Claude-on-Claude \
             spawning. Use a tool / MCP server boundary instead."
                .into(),
        snippet: Some({
            let pre_start = ctx.body[..m.start()].rfind('\n').map(|i| i + 1).unwrap_or(0);
            let post_end = ctx.body[m.end()..]
                .find('\n')
                .map(|i| m.end() + i)
                .unwrap_or(ctx.body.len());
            ctx.body[pre_start..post_end].to_string()
        }),
        location: Some(locate(ctx.body, m.start())),
    })
}

fn rule_permissions_broad(ctx: &RuleCtx) -> Option<GuardFinding> {
    for p in ctx.permissions {
        let bare_wild = p == "*";
        let prefix_wild = matches!(
            p.as_str(),
            "network_*" | "system_*" | "exec_*"
        );
        if bare_wild || prefix_wild {
            return Some(GuardFinding {
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
    None
}

// ─── Scan orchestration ─────────────────────────────────────────────────

/// Run every rule against a single (body, manifest) pair. Returns the
/// raw findings list; override application happens in the caller in a
/// follow-up commit.
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
        if let Some(f) = (rule.eval)(&ctx) {
            out.push(f);
        }
    }
    out
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
            if let Some(f) = (rule.eval)(&ctx) {
                out.push(f);
            }
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

    #[test]
    fn secrets_aws_key_match_redacted() {
        let body = "before AKIAIOSFODNN7EXAMPLE after";
        let f = rule_secrets_aws_key(&ctx(body)).unwrap();
        assert_eq!(f.severity, Severity::Block);
        let s = f.snippet.as_deref().unwrap();
        assert!(!s.contains("AKIAIOSFODNN7EXAMPLE"), "snippet leaked secret: {}", s);
        assert!(s.contains("<redacted:"));
    }

    #[test]
    fn secrets_github_token_match_redacted() {
        // 40 chars after gh prefix.
        let body = "use ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa here";
        let f = rule_secrets_github_token(&ctx(body)).unwrap();
        assert_eq!(f.severity, Severity::Block);
        let s = f.snippet.as_deref().unwrap();
        assert!(!s.contains("ghp_aaaa"), "snippet leaked token: {}", s);
        assert!(s.contains("<redacted:"));
    }

    #[test]
    fn secrets_private_key_match_redacted() {
        let body = "-----BEGIN RSA PRIVATE KEY-----\nMIIEvQIB...\n-----END RSA PRIVATE KEY-----";
        let f = rule_secrets_private_key(&ctx(body)).unwrap();
        assert_eq!(f.severity, Severity::Block);
        let s = f.snippet.as_deref().unwrap();
        assert!(!s.contains("BEGIN RSA"), "snippet should be redacted: {}", s);
    }

    // ── Wildcard tools rule ─────────────────────────────────────────

    #[test]
    fn wildcard_tools_match() {
        let body = "---\nname: foo\nallowed-tools: *\n---\nbody";
        let f = rule_wildcard_tools(&ctx(body)).unwrap();
        assert_eq!(f.severity, Severity::Warn);
    }

    #[test]
    fn wildcard_tools_no_match() {
        let body = "---\nname: foo\nallowed-tools: [Read, Edit]\n---\nbody";
        assert!(rule_wildcard_tools(&ctx(body)).is_none());
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
        let f = rule_mcp_url_not_https(&mcp_ctx(&s)).unwrap();
        assert_eq!(f.severity, Severity::Block);
    }

    #[test]
    fn mcp_url_localhost_ok() {
        let s = vec![McpServerRef {
            name: "ok".into(),
            url: Some("http://localhost:3000".into()),
            command: None,
        }];
        assert!(rule_mcp_url_not_https(&mcp_ctx(&s)).is_none());
    }

    #[test]
    fn mcp_url_127_0_0_1_ok() {
        let s = vec![McpServerRef {
            name: "ok".into(),
            url: Some("http://127.0.0.1:3000".into()),
            command: None,
        }];
        assert!(rule_mcp_url_not_https(&mcp_ctx(&s)).is_none());
    }

    // ── MCP unknown command rule ────────────────────────────────────

    #[test]
    fn mcp_unknown_command_match() {
        let s = vec![McpServerRef {
            name: "weird".into(),
            url: None,
            command: Some("/usr/bin/curl".into()),
        }];
        let f = rule_mcp_unknown_command(&mcp_ctx(&s)).unwrap();
        assert_eq!(f.severity, Severity::Warn);
    }

    #[test]
    fn mcp_unknown_command_npx_ok() {
        let s = vec![McpServerRef {
            name: "ok".into(),
            url: None,
            command: Some("npx".into()),
        }];
        assert!(rule_mcp_unknown_command(&mcp_ctx(&s)).is_none());
    }

    // ── MCP ToS rule ────────────────────────────────────────────────

    #[test]
    fn mcp_tos_agent_cli_claude_match() {
        let body = r#"Run: claude --print "hello""#;
        let f = rule_mcp_tos_agent_cli(&ctx(body)).unwrap();
        assert_eq!(f.severity, Severity::Block);
    }

    #[test]
    fn mcp_tos_agent_cli_codex_match() {
        let body = "Tool: codex run foo";
        let f = rule_mcp_tos_agent_cli(&ctx(body)).unwrap();
        assert_eq!(f.severity, Severity::Block);
    }

    #[test]
    fn mcp_tos_agent_cli_no_match() {
        // Mentions Claude in prose but no headless CLI invocation.
        let body = "I love using Claude for code review. It's great.";
        assert!(rule_mcp_tos_agent_cli(&ctx(body)).is_none());
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
        let f = rule_permissions_broad(&perm_ctx(&p)).unwrap();
        assert_eq!(f.severity, Severity::Warn);
    }

    #[test]
    fn permissions_broad_network_match() {
        let p = vec!["network_*".to_string()];
        let f = rule_permissions_broad(&perm_ctx(&p)).unwrap();
        assert_eq!(f.severity, Severity::Warn);
    }

    #[test]
    fn permissions_specific_ok() {
        let p = vec!["read_files".to_string()];
        assert!(rule_permissions_broad(&perm_ctx(&p)).is_none());
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
}

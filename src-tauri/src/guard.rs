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
use std::path::Path;
use std::sync::OnceLock;

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
/// findings → Green. Used to compute a per-resource verdict when no
/// override mask is in play (override-aware variant lands in the
/// follow-up commit that wires the override store into the scanner).
fn verdict_from_findings(findings: &[GuardFinding]) -> GuardVerdict {
    findings
        .iter()
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
    let verdict = verdict_from_findings(&findings);

    Ok(ResourceVerdict {
        resource_path,
        manifest_path: manifest.manifest_path,
        resource_id: manifest.id,
        kind,
        body_sha256: body_sha,
        verdict,
        findings,
        overridden_findings: Vec::new(),
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
    scan_resource_inner(&profile_dir, &manifest_path)
        .map_err(|e| redact_home(&e.to_string()))
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
}

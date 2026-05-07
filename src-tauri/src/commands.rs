/// Claude commands (.claude/commands/*.md) parsing and management.

#[derive(serde::Serialize, serde::Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CommandFile {
    pub name: String,
    pub file_path: String,
    pub scope: String, // "user" or "project"
    pub description: String,
    pub argument_hint: String,
    pub allowed_tools: Vec<String>,
    pub model: String,
    pub body: String,
    /// Discriminator: "command" (default) or "pipeline".
    /// `#[serde(default)]` makes existing serialized data continue to deserialize.
    #[serde(default = "default_command_type")]
    pub command_type: String,
}

fn default_command_type() -> String {
    "command".to_string()
}

/// Parse a Claude command .md file (YAML frontmatter + body).
fn parse_command_file(content: &str, file_path: &str, scope: &str) -> CommandFile {
    let name = std::path::Path::new(file_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();

    let content = content.trim_start_matches('\n');
    let mut description = String::new();
    let mut argument_hint = String::new();
    let mut allowed_tools: Vec<String> = Vec::new();
    let mut model = String::new();
    let mut command_type = "command".to_string();
    let mut body = content.to_string();

    if content.starts_with("---") {
        let rest = &content[3..];
        if let Some(end) = rest.find("\n---") {
            let frontmatter = &rest[..end];
            body = rest[end + 4..].trim_start_matches('\n').to_string();

            for line in frontmatter.lines() {
                // Split on first ": " (colon-space) to handle values with colons (URLs etc.)
                let parts = if let Some(pos) = line.find(": ") {
                    Some((&line[..pos], &line[pos + 2..]))
                } else if let Some((k, v)) = line.split_once(':') {
                    // Fallback for "key:" with no value
                    Some((k, v))
                } else {
                    None
                };
                if let Some((key, value)) = parts {
                    let key = key.trim();
                    let value = value.trim().to_string();
                    let value = if (value.starts_with('"') && value.ends_with('"'))
                        || (value.starts_with('\'') && value.ends_with('\''))
                    {
                        value[1..value.len() - 1].to_string()
                    } else {
                        value
                    };

                    match key {
                        "description" => description = value,
                        "argument-hint" => argument_hint = value,
                        "model" => model = value,
                        "allowed-tools" => {
                            allowed_tools = value
                                .split(',')
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .collect();
                        }
                        "type" => {
                            // Whitelist: only accept "pipeline" — everything else
                            // (including unknown values) falls back to the default.
                            command_type = if value == "pipeline" {
                                "pipeline".to_string()
                            } else {
                                "command".to_string()
                            };
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    CommandFile {
        name,
        file_path: file_path.to_string(),
        scope: scope.to_string(),
        description,
        argument_hint,
        allowed_tools,
        model,
        body,
        command_type,
    }
}

/// Read all .md command files from a directory.
fn read_commands_from_dir(dir_path: &str, scope: &str) -> Vec<CommandFile> {
    let dir = match std::fs::read_dir(dir_path) {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };

    let mut commands = Vec::new();
    for entry in dir.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let file_path = path.to_string_lossy().to_string();
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        commands.push(parse_command_file(&content, &file_path, scope));
    }
    commands
}

/// Structured result of `ensure_default_commands`. The frontend currently
/// ignores the value, but operators reading logs and tests asserting on
/// sidecar self-heal need to see how many `.md` files we created, how
/// many sidecars we wrote, and any non-fatal sidecar warnings — without
/// having to grep `log::warn!` output.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnsureDefaultCommandsResult {
    pub md_created: u32,
    pub sidecars_created: u32,
    pub sidecar_warnings: Vec<String>,
}

/// Create default command files in the profile's commands/ directory if
/// they don't exist.
///
/// `profile_config_dir = None` defaults to `~/.claude` (default profile).
///
/// Each default ships with a sibling `<name>.weplex.yaml` cross-agent
/// manifest. Both files route through the lockfile (`source: builtin`)
/// so installs are tracked and any prior version lands in history.
/// Existing files are not overwritten — the lockfile's same-sha no-op
/// short-circuits.
#[tauri::command]
pub fn ensure_default_commands(
    profile_config_dir: Option<String>,
) -> Result<EnsureDefaultCommandsResult, String> {
    let profile_dir = resolve_profile_config_dir(profile_config_dir)?;
    let dir = format!("{}/commands", profile_dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let defaults: Vec<(&str, &str)> = vec![
        ("review", r#"---
description: Code review for architecture, security, tests, and requirements
allowed-tools: Read, Grep, Glob, Bash, Agent
---

Review the current changes for:
- Architecture: patterns, structure, code quality, maintainability
- Security: vulnerabilities, input validation, secrets exposure, access control
- Testing: coverage of critical paths, edge cases, regression risks
- Requirements: scope alignment, acceptance criteria, completeness

If issues are found — fix them and review again.
Iterate until all areas pass.
"#),
        ("review-iterate", r#"---
description: Re-run review after applying fixes
allowed-tools: Read, Grep, Glob, Bash, Agent
---

Re-review all areas (architecture, security, testing, requirements).
If everything passes — report summary. If not — fix and review again.
"#),
        ("plan", r#"---
description: Plan implementation approach before coding
allowed-tools: Read, Grep, Glob, Bash
---

Plan the implementation:
- Which files will be affected
- Architectural approach and patterns
- Dependencies and risks
- Edge cases to consider

Do not write code — only the plan.
"#),
        ("pipeline-feature", r#"---
type: pipeline
description: Standard feature pipeline (plan → review → ship)
allowed-tools: Read, Grep, Glob, Bash, Agent
---

Run these commands in order. Do NOT skip steps. Do NOT spawn other agents headless.

1. /plan
2. /review
3. /review-iterate

Stop when /review-iterate reports all areas pass.
"#),
    ];

    // Sidecar manifests for cross-agent rendering. Mirrors the .md
    // ordering above so we can match by name. Each manifest:
    // - id == name (matches filename basename, validated by manifest::load)
    // - claude.source points at the sibling .md (Claude reads it directly)
    // - codex/cursor get a section heading; opencode gets the default
    //   per-id fragment file.
    let default_sidecars: Vec<(&str, &str)> = vec![
        (
            "review",
            r#"id: review
version: 1.0.0
author: weplex
agents:
  claude:
    source: ./review.md
  codex:
    section: Code Review
  cursor:
    section: Code Review
  opencode: {}
permissions:
  - read_files
  - run_bash
mcp_servers: []
"#,
        ),
        (
            "review-iterate",
            r#"id: review-iterate
version: 1.0.0
author: weplex
agents:
  claude:
    source: ./review-iterate.md
  codex:
    section: Iterate Review
  cursor:
    section: Iterate Review
  opencode: {}
permissions:
  - read_files
  - run_bash
mcp_servers: []
"#,
        ),
        (
            "plan",
            r#"id: plan
version: 1.0.0
author: weplex
agents:
  claude:
    source: ./plan.md
  codex:
    section: Implementation Plan
  cursor:
    section: Implementation Plan
  opencode: {}
permissions:
  - read_files
mcp_servers: []
"#,
        ),
        (
            "pipeline-feature",
            r#"id: pipeline-feature
version: 1.0.0
author: weplex
agents:
  claude:
    source: ./pipeline-feature.md
  codex:
    section: Feature Pipeline
  cursor:
    section: Feature Pipeline
  opencode: {}
permissions:
  - read_files
  - run_bash
mcp_servers: []
"#,
        ),
    ];

    let sidecar_for = |name: &str| -> Option<&'static str> {
        for (n, body) in &default_sidecars {
            if *n == name {
                return Some(*body);
            }
        }
        None
    };

    let mut result = EnsureDefaultCommandsResult {
        md_created: 0,
        sidecars_created: 0,
        sidecar_warnings: Vec::new(),
    };
    for (name, content) in defaults {
        let md_path = format!("{}/{}.md", dir, name);
        let sidecar_path = format!("{}/{}.weplex.yaml", dir, name);
        let md_existed = std::path::Path::new(&md_path).exists();
        let sidecar_existed = std::path::Path::new(&sidecar_path).exists();
        let want_sidecar = sidecar_for(name).is_some();

        // Self-heal semantics: never clobber a user-edited .md, but
        // re-write a missing sidecar even when the .md is present.
        // Three cases:
        // 1) Neither exists → install both via lockfile.
        // 2) .md exists, sidecar missing (or no sidecar in defaults) →
        //    write only the missing piece directly. Don't touch the
        //    lockfile when the user already owns the .md.
        // 3) Both exist → no-op.
        if !md_existed {
            // Case 1: full install through lockfile.
            let body_for_install = content.to_string();
            let sidecar_body = sidecar_for(name).map(|s| s.to_string());
            match crate::lockfile::apply_resource_mutation(
                &profile_dir,
                crate::manifest::ResourceKind::Command,
                name,
                crate::lockfile::ResourceSource::Builtin,
                crate::lockfile::MutationKind::Upsert {
                    body: body_for_install,
                    sidecar: sidecar_body,
                },
            ) {
                Ok(_) => {
                    result.md_created += 1;
                    if want_sidecar {
                        result.sidecars_created += 1;
                    }
                }
                Err(e) => {
                    let msg = format!(
                        "ensure_default_commands: failed to install {}: {}",
                        name, e
                    );
                    log::warn!("{}", msg);
                    result.sidecar_warnings.push(msg);
                }
            }
        } else if want_sidecar && !sidecar_existed {
            // Case 2: self-heal a missing sidecar without touching the
            // user's .md. Write it directly — best-effort, never fail
            // the whole call.
            let sidecar_body = sidecar_for(name).expect("checked above");
            match std::fs::write(&sidecar_path, sidecar_body) {
                Ok(()) => result.sidecars_created += 1,
                Err(e) => {
                    let msg = format!(
                        "ensure_default_commands: failed to write sidecar {}: {}",
                        sidecar_path, e
                    );
                    log::warn!("{}", msg);
                    result.sidecar_warnings.push(msg);
                }
            }
        }
        // Case 3 (both exist): nothing to do.
    }
    Ok(result)
}

/// Resolve a profile_config_dir argument. None = default `~/.claude`.
fn resolve_profile_config_dir(profile_config_dir: Option<String>) -> Result<String, String> {
    match profile_config_dir {
        None => {
            let home = crate::utils::get_home();
            Ok(format!("{}/.claude", home))
        }
        Some(p) if p.is_empty() => {
            let home = crate::utils::get_home();
            Ok(format!("{}/.claude", home))
        }
        Some(p) => crate::utils::validate_config_dir(&p),
    }
}

/// List all Claude commands: user-level (~/.claude/commands/) + project-level ({cwd}/.claude/commands/).
#[tauri::command]
pub fn list_commands(cwd: Option<String>) -> Result<Vec<CommandFile>, String> {
    let home = crate::utils::get_home();
    let user_dir = format!("{}/.claude/commands", home);
    let mut commands = read_commands_from_dir(&user_dir, "user");

    if let Some(cwd) = cwd {
        let resolved = crate::utils::resolve_cwd(&cwd);
        let project_dir = format!("{}/.claude/commands", resolved);
        if project_dir != user_dir {
            commands.extend(read_commands_from_dir(&project_dir, "project"));
        }
    }

    commands.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(commands)
}

/// Save a Claude command file (frontmatter + body).
///
/// `scope = "user"` routes through the lockfile so the install is
/// tracked with `source: User`. `profile_config_dir = None` defaults to
/// `~/.claude`.
///
/// `scope = "project"` keeps the legacy behavior: writes to
/// `<cwd>/.claude/commands/` without lockfile bookkeeping. Project
/// commands are not part of any profile.
#[tauri::command]
pub fn save_command(
    profile_config_dir: Option<String>,
    name: String,
    scope: String,
    cwd: Option<String>,
    description: String,
    argument_hint: String,
    allowed_tools: Vec<String>,
    model: String,
    body: String,
    command_type: Option<String>,
) -> Result<String, String> {
    let safe_name = crate::utils::sanitize_name(&name)?;

    // Whitelist: None or Some("command") = command, Some("pipeline") = pipeline.
    // Anything else is rejected with a clear error so the frontend can't
    // smuggle arbitrary frontmatter through this field.
    let is_pipeline = match command_type.as_deref() {
        None | Some("command") => false,
        Some("pipeline") => true,
        Some(other) => {
            return Err(format!(
                "invalid command_type {:?}: expected \"command\" or \"pipeline\"",
                other
            ));
        }
    };

    // Sanitize frontmatter values: strip newlines and "---" to prevent injection
    let sanitize_fm = |s: &str| -> String {
        s.replace('\n', " ").replace('\r', "").replace("---", "")
    };

    let mut frontmatter = String::from("---\n");
    if !description.is_empty() {
        frontmatter.push_str(&format!("description: {}\n", sanitize_fm(&description)));
    }
    if is_pipeline {
        // Emit `type: pipeline` right after description (or at the top of
        // the frontmatter if no description). Only "pipeline" is ever
        // emitted — the implicit default is "command" so we don't write it.
        frontmatter.push_str("type: pipeline\n");
    }
    if !argument_hint.is_empty() {
        frontmatter.push_str(&format!("argument-hint: {}\n", sanitize_fm(&argument_hint)));
    }
    if !allowed_tools.is_empty() {
        frontmatter.push_str(&format!("allowed-tools: {}\n", sanitize_fm(&allowed_tools.join(", "))));
    }
    if !model.is_empty() {
        frontmatter.push_str(&format!("model: {}\n", sanitize_fm(&model)));
    }
    frontmatter.push_str("---\n\n");
    let content = format!("{}{}", frontmatter, body);

    if scope == "project" {
        // Project-scoped commands live with the repo and are not part
        // of any profile. Keep the legacy behavior.
        let cwd = cwd.ok_or("cwd required for project commands")?;
        let resolved = crate::utils::resolve_cwd(&cwd);
        let dir = format!("{}/.claude/commands", resolved);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = format!("{}/{}.md", dir, safe_name);
        std::fs::write(&path, content).map_err(|e| e.to_string())?;
        return Ok(path);
    }

    // User scope: route through lockfile.
    let profile_dir = resolve_profile_config_dir(profile_config_dir)?;
    crate::lockfile::apply_resource_mutation(
        &profile_dir,
        crate::manifest::ResourceKind::Command,
        &safe_name,
        crate::lockfile::ResourceSource::User,
        crate::lockfile::MutationKind::Upsert {
            body: content,
            sidecar: None,
        },
    )
    .map_err(|e| format!("{}", e))?;

    Ok(format!("{}/commands/{}.md", profile_dir, safe_name))
}

/// Delete a Claude command file.
///
/// `profile_config_dir = None` is interpreted as either:
///   - the default `~/.claude` profile (user-scope command), OR
///   - a project-scope command living in `<cwd>/.claude/commands/`.
///
/// For user-scope commands routed through a known profile (the lockfile
/// has an entry for them), the deletion goes through the lockfile so
/// the previous version lands in history (rollback-able). For
/// project-scope and unrecorded commands, falls back to a direct
/// filesystem delete after the same path validations as before.
#[tauri::command]
pub fn delete_command(
    profile_config_dir: Option<String>,
    path: String,
) -> Result<(), String> {
    let canon = std::fs::canonicalize(&path).map_err(|e| e.to_string())?;
    let canon_str = canon.to_string_lossy().to_string();

    // Must be a .md file
    if !canon_str.ends_with(".md") {
        return Err("Can only delete .md files".to_string());
    }

    // Validate: parent dir must be named "commands"
    let parent = canon.parent().ok_or("Invalid path")?;
    let parent_name = parent.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if parent_name != "commands" {
        return Err("Path is not within a commands/ directory".to_string());
    }

    // Additionally verify it's under $HOME
    let home = crate::utils::get_home();
    if !canon_str.starts_with(&home) {
        return Err("Path must be under home directory".to_string());
    }

    // Try lockfile routing if a profile dir is supplied.
    if let Some(p) = profile_config_dir.as_ref()
        && !p.is_empty()
    {
        let profile_dir = crate::utils::validate_config_dir(p)?;
        let expected_dir = format!("{}/commands", profile_dir);
        if let Some(parent_str) = parent.to_str()
            && parent_str == expected_dir
        {
            // Compute the resource name from the file stem.
            let name = canon
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or("Invalid filename")?
                .to_string();
            // Check the lockfile actually has it.
            let lf = crate::lockfile::load_lockfile(&profile_dir);
            let id = format!("commands/{}", name);
            if lf.resources.iter().any(|r| r.id == id) {
                crate::lockfile::apply_resource_mutation(
                    &profile_dir,
                    crate::manifest::ResourceKind::Command,
                    &name,
                    crate::lockfile::ResourceSource::User,
                    crate::lockfile::MutationKind::Delete,
                )
                .map_err(|e| format!("{}", e))?;
                return Ok(());
            }
        }
    }

    // Fallback: direct delete (project-scope or unrecorded user-scope).
    // We still require the grandparent to be `.claude` for the
    // project-scope safety check, which the original code enforced.
    let grandparent = parent.parent().ok_or("Invalid path")?;
    let grandparent_name = grandparent
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    if grandparent_name != ".claude" && profile_config_dir.is_none() {
        return Err("Path is not within a .claude/commands/ directory".to_string());
    }
    if canon.exists() {
        std::fs::remove_file(&canon).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::test_support::HOME_ENV_LOCK as ENV_LOCK;

    fn tmpdir(label: &str) -> std::path::PathBuf {
        let base = std::env::temp_dir().join(format!(
            "weplex-commands-test-{}-{}-{}",
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
    fn parse_command_file_pipeline_type() {
        let content = "---\ndescription: x\ntype: pipeline\n---\nbody";
        let f = parse_command_file(content, "/x.md", "user");
        assert_eq!(f.command_type, "pipeline");
    }

    #[test]
    fn parse_command_file_unknown_type_defaults_to_command() {
        let content = "---\ndescription: x\ntype: weird\n---\nbody";
        let f = parse_command_file(content, "/x.md", "user");
        assert_eq!(f.command_type, "command");
    }

    #[test]
    fn parse_command_file_no_type_defaults_to_command() {
        let content = "---\ndescription: x\n---\nbody";
        let f = parse_command_file(content, "/x.md", "user");
        assert_eq!(f.command_type, "command");
    }

    #[test]
    fn save_command_pipeline_emits_type_in_frontmatter() {
        let _g = ENV_LOCK.lock().unwrap();
        let home = tmpdir("save-pipeline");
        let canon_home = std::fs::canonicalize(&home).unwrap();
        let prev = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", &canon_home); }

        let path = save_command(
            None,
            "my-pipe".to_string(),
            "user".to_string(),
            None,
            "demo pipeline".to_string(),
            String::new(),
            Vec::new(),
            String::new(),
            "step 1\nstep 2".to_string(),
            Some("pipeline".to_string()),
        )
        .expect("save_command should succeed");

        let written = std::fs::read_to_string(&path).expect("read written .md");
        assert!(
            written.contains("type: pipeline\n"),
            "expected 'type: pipeline' in frontmatter, got: {}",
            written
        );

        // Round-trip through parse_command_file to confirm.
        let parsed = parse_command_file(&written, &path, "user");
        assert_eq!(parsed.command_type, "pipeline");
        assert_eq!(parsed.description, "demo pipeline");

        if let Some(p) = prev {
            unsafe { std::env::set_var("HOME", p); }
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn save_command_command_type_omits_type_in_frontmatter() {
        // Default and explicit "command" must NOT emit a `type:` line —
        // existing .md files have never had one, and we don't want to
        // start writing it for the implicit case.
        let _g = ENV_LOCK.lock().unwrap();
        let home = tmpdir("save-default");
        let canon_home = std::fs::canonicalize(&home).unwrap();
        let prev = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", &canon_home); }

        let path = save_command(
            None,
            "plain".to_string(),
            "user".to_string(),
            None,
            "regular command".to_string(),
            String::new(),
            Vec::new(),
            String::new(),
            "do thing".to_string(),
            None,
        )
        .expect("save_command should succeed");

        let written = std::fs::read_to_string(&path).expect("read written .md");
        assert!(
            !written.contains("type:"),
            "expected no 'type:' line for default command, got: {}",
            written
        );

        if let Some(p) = prev {
            unsafe { std::env::set_var("HOME", p); }
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn save_command_invalid_type_returns_error() {
        let _g = ENV_LOCK.lock().unwrap();
        let home = tmpdir("save-invalid");
        let canon_home = std::fs::canonicalize(&home).unwrap();
        let prev = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", &canon_home); }

        let err = save_command(
            None,
            "bogus".to_string(),
            "user".to_string(),
            None,
            "x".to_string(),
            String::new(),
            Vec::new(),
            String::new(),
            "body".to_string(),
            Some("garbage".to_string()),
        )
        .expect_err("invalid command_type must return Err");
        assert!(
            err.contains("invalid command_type") && err.contains("garbage"),
            "expected clear error mentioning the bad value, got: {}",
            err
        );

        if let Some(p) = prev {
            unsafe { std::env::set_var("HOME", p); }
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn ensure_default_commands_creates_md_and_sidecars() {
        let _g = ENV_LOCK.lock().unwrap();
        let home = tmpdir("ensure-defaults");
        let canon_home = std::fs::canonicalize(&home).unwrap();
        let prev = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", &canon_home); }

        let result = ensure_default_commands(None).unwrap();
        // Four default commands ship: review, review-iterate, plan, pipeline-feature.
        assert_eq!(result.md_created, 4);
        assert_eq!(result.sidecars_created, 4);
        assert!(result.sidecar_warnings.is_empty());

        let cmd_dir = canon_home.join(".claude").join("commands");
        for name in ["review", "review-iterate", "plan", "pipeline-feature"] {
            assert!(cmd_dir.join(format!("{}.md", name)).exists());
            assert!(cmd_dir.join(format!("{}.weplex.yaml", name)).exists());
        }

        if let Some(p) = prev {
            unsafe { std::env::set_var("HOME", p); }
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn ensure_default_commands_self_heals_missing_sidecars() {
        // Second call after .md files exist: md_created=0, but sidecars
        // we delete by hand should be re-created.
        let _g = ENV_LOCK.lock().unwrap();
        let home = tmpdir("ensure-self-heal");
        let canon_home = std::fs::canonicalize(&home).unwrap();
        let prev = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", &canon_home); }

        // First run: full creation.
        let _ = ensure_default_commands(None).unwrap();
        let cmd_dir = canon_home.join(".claude").join("commands");
        // Delete one sidecar by hand.
        std::fs::remove_file(cmd_dir.join("review.weplex.yaml")).unwrap();

        // Second run: md untouched, only the deleted sidecar is re-created.
        let result = ensure_default_commands(None).unwrap();
        assert_eq!(result.md_created, 0);
        assert_eq!(result.sidecars_created, 1);
        assert!(result.sidecar_warnings.is_empty());
        assert!(cmd_dir.join("review.weplex.yaml").exists());

        if let Some(p) = prev {
            unsafe { std::env::set_var("HOME", p); }
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn ensure_default_commands_writes_lockfile_with_source_builtin() {
        let _g = ENV_LOCK.lock().unwrap();
        let home = tmpdir("ensure-lockfile");
        let canon_home = std::fs::canonicalize(&home).unwrap();
        let prev = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", &canon_home); }

        let _ = ensure_default_commands(None).unwrap();

        // Read the lockfile and confirm each builtin landed with the
        // Builtin source marker.
        let profile_dir = canon_home.join(".claude");
        let lf = crate::lockfile::load_lockfile(profile_dir.to_str().unwrap());
        assert_eq!(lf.resources.len(), 4);
        for entry in &lf.resources {
            assert_eq!(entry.source, crate::lockfile::ResourceSource::Builtin);
            assert!(entry.id.starts_with("commands/"));
        }

        if let Some(p) = prev {
            unsafe { std::env::set_var("HOME", p); }
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn ensure_default_commands_pipeline_feature_has_pipeline_type() {
        // The shipped pipeline-feature.md must declare type: pipeline so the
        // frontend renders it as a pipeline, not as a regular command.
        let _g = ENV_LOCK.lock().unwrap();
        let home = tmpdir("ensure-pipeline-type");
        let canon_home = std::fs::canonicalize(&home).unwrap();
        let prev = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", &canon_home); }

        let _ = ensure_default_commands(None).unwrap();

        let md_path = canon_home
            .join(".claude")
            .join("commands")
            .join("pipeline-feature.md");
        let content = std::fs::read_to_string(&md_path).unwrap();
        assert!(
            content.contains("type: pipeline\n"),
            "pipeline-feature.md must declare type: pipeline"
        );

        // And it must round-trip through parse_command_file as a pipeline.
        let parsed = parse_command_file(&content, md_path.to_str().unwrap(), "user");
        assert_eq!(parsed.command_type, "pipeline");

        if let Some(p) = prev {
            unsafe { std::env::set_var("HOME", p); }
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn ensure_default_commands_idempotent_third_call() {
        // After a full creation + self-heal, a third call must report
        // zero work but still succeed.
        let _g = ENV_LOCK.lock().unwrap();
        let home = tmpdir("ensure-idempotent");
        let canon_home = std::fs::canonicalize(&home).unwrap();
        let prev = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", &canon_home); }

        let _ = ensure_default_commands(None).unwrap();
        let result = ensure_default_commands(None).unwrap();
        assert_eq!(result.md_created, 0);
        assert_eq!(result.sidecars_created, 0);
        assert!(result.sidecar_warnings.is_empty());

        if let Some(p) = prev {
            unsafe { std::env::set_var("HOME", p); }
        }
        let _ = std::fs::remove_dir_all(&home);
    }
}

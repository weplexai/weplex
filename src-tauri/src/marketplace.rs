//! Marketplace package install: routes through the lockfile module so
//! every install lands in `.weplex.lock.yaml` with `source: marketplace`.
//!
//! Phase 3 rewrite: previously these wrote to `~/.weplex/agents/` and
//! `~/.weplex/skills/<name>/SKILL.md` (global, off-profile). Now every
//! install targets a specific profile config dir and goes through
//! `lockfile::apply_resource_mutation` so:
//!
//! - the install is recorded with provenance `Marketplace`
//! - prior versions land in history (cache-backed)
//! - the lockfile is updated atomically under flock

use crate::lockfile::{
    self, MutationKind, MutationReport, ResourceSource,
};
use crate::manifest::ResourceKind;
use crate::utils::{sanitize_name, validate_config_dir};

/// Install a marketplace package (agent / rule / skill / command) into
/// the target profile.
///
/// `target_config_dir`: absolute path to the Claude profile (e.g.
///   `~/.claude` or `~/.claude-work`). Validated to be under HOME.
/// `kind`: which resource directory to write to (agents/rules/skills/commands).
/// `name`: filename without extension. Sanitized.
/// `content`: body to write as `<kind>/<name>.md` (or `skills/<name>/SKILL.md`).
/// `sidecar`: optional `*.weplex.yaml` cross-agent manifest.
#[tauri::command]
pub fn install_marketplace_package(
    target_config_dir: String,
    name: String,
    content: String,
    sidecar: Option<String>,
    kind: ResourceKind,
) -> Result<MutationReport, String> {
    let dir = validate_config_dir(&target_config_dir)
        .map_err(|e| redact_home(&e))?;
    let safe_name = sanitize_name(&name).map_err(|e| redact_home(&e))?;

    log::info!(
        "marketplace install: profile={}, kind={:?}, name={}",
        dir,
        kind,
        safe_name
    );

    lockfile::apply_resource_mutation(
        &dir,
        kind,
        &safe_name,
        ResourceSource::Marketplace,
        MutationKind::Upsert {
            body: content,
            sidecar,
        },
    )
    .map_err(|e| redact_home(&format!("{}", e)))
}

/// Replace a leading $HOME with `~` so error strings handed back to the
/// frontend don't leak the user's home path.
fn redact_home(s: &str) -> String {
    let home = crate::utils::get_home();
    if !home.is_empty() && s.starts_with(&home) {
        format!("~{}", &s[home.len()..])
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::test_support::HOME_ENV_LOCK as ENV_LOCK;

    fn tmpdir(label: &str) -> std::path::PathBuf {
        let base = std::env::temp_dir().join(format!(
            "weplex-marketplace-test-{}-{}-{}",
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
    fn install_marketplace_writes_lockfile_with_source_marketplace() {
        let _g = ENV_LOCK.lock().unwrap();
        let home = tmpdir("install");
        let canon = std::fs::canonicalize(&home).unwrap();
        let prev = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", &canon); }

        let profile_dir = canon.join(".claude");
        std::fs::create_dir_all(&profile_dir).unwrap();

        let report = install_marketplace_package(
            profile_dir.to_string_lossy().into_owned(),
            "reviewer".to_string(),
            "# reviewer".to_string(),
            None,
            ResourceKind::Agent,
        )
        .unwrap();

        assert!(!report.no_op);
        assert_eq!(report.resource_id, "agents/reviewer");
        assert!(profile_dir.join("agents/reviewer.md").exists());

        let lf = lockfile::load_lockfile(profile_dir.to_str().unwrap());
        assert_eq!(lf.resources.len(), 1);
        assert_eq!(lf.resources[0].source, ResourceSource::Marketplace);

        if let Some(p) = prev {
            unsafe { std::env::set_var("HOME", p); }
        }
        let _ = std::fs::remove_dir_all(&home);
    }
}

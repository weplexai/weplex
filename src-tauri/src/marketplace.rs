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
/// `pack`: optional federated pack id (`<owner>/<repo>` lowercase). Set
///   when installing a resource as part of a federated pack so the
///   lockfile records its provenance and rejects later collisions from
///   a different pack — or from a stray single-resource publish.
/// `pack_commit_sha`: optional commit sha the federated install was
///   pinned to. Required alongside `pack` for federated installs so the
///   lockfile carries the exact upstream version on disk; ignored when
///   `pack` is `None`.
#[tauri::command]
pub fn install_marketplace_package(
    target_config_dir: String,
    name: String,
    content: String,
    sidecar: Option<String>,
    kind: ResourceKind,
    pack: Option<String>,
    pack_commit_sha: Option<String>,
) -> Result<MutationReport, String> {
    let dir = validate_config_dir(&target_config_dir)
        .map_err(|e| redact_home(&e))?;
    let safe_name = sanitize_name(&name).map_err(|e| redact_home(&e))?;

    log::info!(
        "marketplace install: profile={}, kind={:?}, name={}, pack={:?}, pack_commit_sha={:?}",
        dir,
        kind,
        safe_name,
        pack,
        pack_commit_sha,
    );

    lockfile::apply_resource_mutation(
        &dir,
        kind,
        &safe_name,
        ResourceSource::Marketplace,
        MutationKind::Upsert {
            body: content,
            sidecar,
            pack,
            pack_commit_sha,
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
            None,
            None,
        )
        .unwrap();

        assert!(!report.no_op);
        assert_eq!(report.resource_id, "agents/reviewer");
        assert!(profile_dir.join("agents/reviewer.md").exists());

        let lf = lockfile::load_lockfile(profile_dir.to_str().unwrap());
        assert_eq!(lf.resources.len(), 1);
        assert_eq!(lf.resources[0].source, ResourceSource::Marketplace);
        assert_eq!(lf.resources[0].pack, None);
        assert_eq!(lf.resources[0].pack_commit_sha, None);

        if let Some(p) = prev {
            unsafe { std::env::set_var("HOME", p); }
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn install_marketplace_records_pack_provenance() {
        let _g = ENV_LOCK.lock().unwrap();
        let home = tmpdir("install-pack");
        let canon = std::fs::canonicalize(&home).unwrap();
        let prev = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", &canon); }

        let profile_dir = canon.join(".claude");
        std::fs::create_dir_all(&profile_dir).unwrap();

        let report = install_marketplace_package(
            profile_dir.to_string_lossy().into_owned(),
            "architect".to_string(),
            "# architect".to_string(),
            None,
            ResourceKind::Agent,
            Some("acme/awesome-claude-agents".to_string()),
            Some("0123abc".to_string()),
        )
        .unwrap();

        assert!(!report.no_op);
        let lf = lockfile::load_lockfile(profile_dir.to_str().unwrap());
        assert_eq!(lf.resources.len(), 1);
        assert_eq!(lf.resources[0].source, ResourceSource::Marketplace);
        assert_eq!(
            lf.resources[0].pack.as_deref(),
            Some("acme/awesome-claude-agents")
        );
        assert_eq!(
            lf.resources[0].pack_commit_sha.as_deref(),
            Some("0123abc")
        );

        if let Some(p) = prev {
            unsafe { std::env::set_var("HOME", p); }
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    /// I3 round-trip: a federated install carries pack_commit_sha from
    /// the registry into the on-disk lockfile, and a non-federated
    /// install (pack=None) stores pack_commit_sha=None even if a sha is
    /// passed (defensive — we don't record commits for non-pack flows).
    #[test]
    fn pack_commit_sha_round_trips_through_lockfile() {
        let _g = ENV_LOCK.lock().unwrap();
        let home = tmpdir("install-sha");
        let canon = std::fs::canonicalize(&home).unwrap();
        let prev = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", &canon); }

        let profile_dir = canon.join(".claude");
        std::fs::create_dir_all(&profile_dir).unwrap();

        // Federated install with a commit sha — should be persisted.
        install_marketplace_package(
            profile_dir.to_string_lossy().into_owned(),
            "fed-agent".to_string(),
            "# fed".to_string(),
            None,
            ResourceKind::Agent,
            Some("acme/pack".to_string()),
            Some("deadbeefcafe".to_string()),
        )
        .unwrap();

        // Reload from disk to confirm the sha survived YAML serialisation.
        let lf = lockfile::load_lockfile(profile_dir.to_str().unwrap());
        let entry = lf.resources.iter().find(|e| e.id == "agents/fed-agent").unwrap();
        assert_eq!(entry.pack.as_deref(), Some("acme/pack"));
        assert_eq!(entry.pack_commit_sha.as_deref(), Some("deadbeefcafe"));

        // Non-federated install (different name): pack and pack_commit_sha both None.
        install_marketplace_package(
            profile_dir.to_string_lossy().into_owned(),
            "single".to_string(),
            "# single".to_string(),
            None,
            ResourceKind::Agent,
            None,
            None,
        )
        .unwrap();
        let lf = lockfile::load_lockfile(profile_dir.to_str().unwrap());
        let single = lf.resources.iter().find(|e| e.id == "agents/single").unwrap();
        assert_eq!(single.pack, None);
        assert_eq!(single.pack_commit_sha, None);

        if let Some(p) = prev {
            unsafe { std::env::set_var("HOME", p); }
        }
        let _ = std::fs::remove_dir_all(&home);
    }
}

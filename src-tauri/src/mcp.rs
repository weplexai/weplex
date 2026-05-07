/// MCP server binary discovery and registration in Claude config.

/// Find the weplex-mcp binary path based on build mode and platform.
/// In production, Tauri externalBin places it next to the main executable.
pub fn find_mcp_binary(_app: &tauri::AppHandle) -> Result<String, String> {
    // Dev mode: weplex-mcp is a workspace member, so cargo emits it into the
    // shared workspace target dir (src-tauri/target/) — that's the canonical
    // path for `pnpm tauri dev`. The legacy `mcp-server/target/` path is only
    // a fallback for stale checkouts; remove it once the workspace migration
    // has shipped one release. We never mix profiles (debug Tauri host must
    // not load a release binary just because its mtime is newer).
    if cfg!(debug_assertions) {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let candidates = [
            manifest.join("target/debug/weplex-mcp"),
            manifest.join("mcp-server/target/debug/weplex-mcp"),
            // Release fallbacks — only if no debug binary exists at all.
            manifest.join("target/release/weplex-mcp"),
            manifest.join("mcp-server/target/release/weplex-mcp"),
        ];
        for path in &candidates {
            if path.exists() {
                return Ok(path.to_string_lossy().to_string());
            }
        }
        return Err("weplex-mcp binary not found. Run: (cd src-tauri && cargo build -p weplex-mcp)".to_string());
    }

    // Production: Tauri externalBin places sidecar next to main executable
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let sidecar_path = dir.join("weplex-mcp");
            if sidecar_path.exists() {
                return Ok(sidecar_path.to_string_lossy().to_string());
            }
        }
    }

    Err("weplex-mcp binary not found".to_string())
}

/// Get the path to the weplex-mcp binary.
/// In dev mode: looks in src-tauri/mcp-server/target/debug/
/// In release: looks next to the main binary (Contents/MacOS/)
#[tauri::command]
pub fn get_mcp_binary_path(app: tauri::AppHandle) -> Result<String, String> {
    find_mcp_binary(&app)
}

/// Register the weplex MCP server in every discovered profile's `.claude.json`.
/// Default profile (`~/.claude.json`) plus every `.claude-*` and `CLAUDE_CONFIG_DIR`
/// profile found by `profile::discover_profiles()`.
#[tauri::command]
pub fn register_mcp_in_claude(app: tauri::AppHandle) -> Result<(), String> {
    do_register_mcp_in_claude(&app)
}

/// Register or update the weplex MCP server entry across all profiles.
/// Errors writing one profile do not block the others — every failure is logged
/// and the function returns the first error after attempting all of them.
pub fn do_register_mcp_in_claude(app: &tauri::AppHandle) -> Result<(), String> {
    let binary_path = match find_mcp_binary(app) {
        Ok(p) => p,
        Err(e) => {
            log::warn!("MCP registration skipped: {}", e);
            return Ok(()); // Don't fail startup if binary not found
        }
    };

    let mut config_paths: Vec<String> = Vec::new();
    config_paths.push(format!("{}/.claude.json", crate::utils::get_home()));
    if let Ok(profiles) = crate::profile::discover_profiles() {
        for profile in profiles {
            config_paths.push(format!("{}/.claude.json", profile.path));
        }
    }

    // Deduplicate (a profile path could collide with $HOME in unusual setups).
    config_paths.sort();
    config_paths.dedup();

    let mut first_error: Option<String> = None;
    for path in &config_paths {
        if let Err(e) = register_mcp_in_config_file(path, &binary_path) {
            log::error!("MCP registration failed for {}: {}", path, e);
            if first_error.is_none() {
                first_error = Some(e);
            }
        }
    }

    match first_error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

/// Register or update the `mcpServers.weplex` entry in a single `.claude.json`.
/// Atomic tmp+rename with 0600 mode so a crash mid-write can't corrupt the file
/// (it holds every MCP server config for this user, some of which may be API keys).
fn register_mcp_in_config_file(claude_json_path: &str, binary_path: &str) -> Result<(), String> {
    // Read existing config or create empty object
    let mut config: serde_json::Value = if let Ok(content) = std::fs::read_to_string(claude_json_path) {
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    // Ensure mcpServers object exists
    if config.get("mcpServers").is_none() {
        config["mcpServers"] = serde_json::json!({});
    }

    // Check if weplex entry exists and matches
    let current_command = config
        .get("mcpServers")
        .and_then(|s| s.get("weplex"))
        .and_then(|w| w.get("command"))
        .and_then(|c| c.as_str())
        .unwrap_or("");

    if current_command == binary_path {
        return Ok(());
    }

    config["mcpServers"]["weplex"] = serde_json::json!({
        "command": binary_path
    });

    let json_str = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    crate::utils::atomic_write_owner_only(claude_json_path, &json_str)?;

    log::info!("MCP server registered in {} (binary: {})", claude_json_path, binary_path);
    Ok(())
}

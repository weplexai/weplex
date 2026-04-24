/// MCP server binary discovery and registration in Claude config.

/// Find the weplex-mcp binary path based on build mode and platform.
/// In production, Tauri externalBin places it next to the main executable.
pub fn find_mcp_binary(_app: &tauri::AppHandle) -> Result<String, String> {
    // Dev mode: check mcp-server build directory
    if cfg!(debug_assertions) {
        let dev_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("mcp-server/target/debug/weplex-mcp");
        if dev_path.exists() {
            return Ok(dev_path.to_string_lossy().to_string());
        }
        let dev_release = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("mcp-server/target/release/weplex-mcp");
        if dev_release.exists() {
            return Ok(dev_release.to_string_lossy().to_string());
        }
        return Err("weplex-mcp binary not found. Run: cd src-tauri/mcp-server && cargo build".to_string());
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

/// Register the weplex MCP server in ~/.claude.json.
/// Creates the file if it doesn't exist, adds/updates the mcpServers.weplex entry.
#[tauri::command]
pub fn register_mcp_in_claude(app: tauri::AppHandle) -> Result<(), String> {
    do_register_mcp_in_claude(&app)
}

/// Register or update the weplex MCP server entry in ~/.claude.json.
pub fn do_register_mcp_in_claude(app: &tauri::AppHandle) -> Result<(), String> {
    let binary_path = match find_mcp_binary(app) {
        Ok(p) => p,
        Err(e) => {
            log::warn!("MCP registration skipped: {}", e);
            return Ok(()); // Don't fail startup if binary not found
        }
    };

    let home = crate::utils::get_home();
    let claude_json_path = format!("{}/.claude.json", home);

    // Read existing config or create empty object
    let mut config: serde_json::Value = if let Ok(content) = std::fs::read_to_string(&claude_json_path) {
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    // Ensure mcpServers object exists
    if !config.get("mcpServers").is_some() {
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
        // Already up to date
        return Ok(());
    }

    // Add/update weplex entry
    config["mcpServers"]["weplex"] = serde_json::json!({
        "command": binary_path
    });

    // Write back — preserve formatting. Atomic tmp+rename with 0600 mode so a
    // crash mid-write can't corrupt ~/.claude.json (which holds every MCP
    // server config for this user, some of which may be API keys).
    let json_str = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    crate::utils::atomic_write_owner_only(&claude_json_path, &json_str)?;

    log::info!("MCP server registered in ~/.claude.json (binary: {})", binary_path);
    Ok(())
}

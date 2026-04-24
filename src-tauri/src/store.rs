/// Persistent JSON store with atomic writes and backup rotation.

use tauri::Manager;

/// Validate store key: only alphanumeric, underscore, hyphen allowed.
fn validate_store_key(key: &str) -> Result<(), String> {
    if key.is_empty() || key.len() > 64 {
        return Err("Invalid store key length".to_string());
    }
    if !key
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(format!("Invalid store key: {}", key));
    }
    Ok(())
}

fn stores_dir_name() -> &'static str {
    if cfg!(debug_assertions) {
        "stores-dev"
    } else {
        "stores"
    }
}

#[tauri::command]
pub fn persist_store(app: tauri::AppHandle, key: String, value: String) -> Result<(), String> {
    validate_store_key(&key)?;

    let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let stores_dir = app_data.join(stores_dir_name());
    std::fs::create_dir_all(&stores_dir).map_err(|e| e.to_string())?;

    let path = stores_dir.join(format!("{}.json", key));
    let tmp_path = stores_dir.join(format!("{}.json.tmp", key));
    let backup_path = stores_dir.join(format!("{}.json.backup", key));

    // Write to temp file first
    std::fs::write(&tmp_path, &value).map_err(|e| e.to_string())?;

    // Rotate: current → backup (ignore error if current doesn't exist yet)
    if path.exists() {
        let _ = std::fs::rename(&path, &backup_path);
    }

    // Atomic rename: tmp → current
    std::fs::rename(&tmp_path, &path).map_err(|e| e.to_string())?;

    // Restrict permissions for sensitive keys (auth tokens)
    #[cfg(unix)]
    if key.contains("auth") {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }

    Ok(())
}

#[tauri::command]
pub fn load_store(app: tauri::AppHandle, key: String) -> Result<Option<String>, String> {
    validate_store_key(&key)?;

    let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let stores_dir = app_data.join(stores_dir_name());

    let path = stores_dir.join(format!("{}.json", key));
    let backup_path = stores_dir.join(format!("{}.json.backup", key));

    // Try primary
    if path.exists()
        && let Ok(content) = std::fs::read_to_string(&path)
        && !content.is_empty()
    {
        return Ok(Some(content));
    }

    // Fallback to backup
    if backup_path.exists()
        && let Ok(content) = std::fs::read_to_string(&backup_path)
        && !content.is_empty()
    {
        return Ok(Some(content));
    }

    Ok(None)
}

use keyring::Entry;

/// Use different keychain service names for dev and release builds
/// to prevent cross-contamination when both are running simultaneously.
#[cfg(debug_assertions)]
const SERVICE_NAME: &str = "com.weplex.app.dev";
#[cfg(not(debug_assertions))]
const SERVICE_NAME: &str = "com.weplex.app";

#[tauri::command]
pub fn keychain_save(key: String, value: String) -> Result<(), String> {
    let entry = Entry::new(SERVICE_NAME, &key).map_err(|e| e.to_string())?;
    entry.set_password(&value).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn keychain_load(key: String) -> Result<Option<String>, String> {
    let entry = Entry::new(SERVICE_NAME, &key).map_err(|e| e.to_string())?;
    match entry.get_password() {
        Ok(password) => Ok(Some(password)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub fn keychain_delete(key: String) -> Result<(), String> {
    let entry = Entry::new(SERVICE_NAME, &key).map_err(|e| e.to_string())?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()), // already deleted, fine
        Err(e) => Err(e.to_string()),
    }
}

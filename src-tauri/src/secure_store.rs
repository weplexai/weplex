//! Encrypted file storage for sensitive data (auth tokens, etc.).
//! Uses AES-256-GCM with a machine-derived key.
//! This is the fallback when OS keychain is unavailable (e.g., after app updates).

use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use ring::digest;
use std::path::PathBuf;
use tauri::Manager;

const APP_SALT: &[u8] = b"weplex-secure-store-v1";

/// HKDF output length wrapper required by ring
struct HkdfLen(usize);
impl ring::hkdf::KeyType for HkdfLen {
    fn len(&self) -> usize {
        self.0
    }
}

/// Derive a 256-bit encryption key from machine-specific data.
/// Key = SHA-256(hostname + username + app_salt)
/// This ties the encrypted file to this specific machine/user.
fn derive_key() -> [u8; 32] {
    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown-host".to_string());
    let username = whoami::username().unwrap_or_else(|_| "unknown-user".to_string());

    // Use HKDF-SHA256 with null-separated inputs to prevent concatenation collisions
    // IKM = hostname\0username\0salt, info = "weplex-token-encryption"
    let salt = ring::hkdf::Salt::new(ring::hkdf::HKDF_SHA256, APP_SALT);
    let mut ikm = Vec::new();
    ikm.extend_from_slice(hostname.as_bytes());
    ikm.push(0); // null separator prevents "ab"+"cdef" == "abc"+"def"
    ikm.extend_from_slice(username.as_bytes());
    let prk = salt.extract(&ikm);
    let okm = prk
        .expand(&[b"weplex-token-encryption"], HkdfLen(32))
        .expect("HKDF expand failed");
    let mut key = [0u8; 32];
    okm.fill(&mut key).expect("HKDF fill failed");
    key
}

/// Encrypt plaintext with AES-256-GCM.
/// Returns: nonce (12 bytes) || ciphertext (variable length)
fn encrypt(plaintext: &[u8]) -> Result<Vec<u8>, String> {
    let key = derive_key();
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| format!("Cipher init: {}", e))?;

    // Generate random 96-bit nonce
    let nonce_bytes: [u8; 12] = {
        use aes_gcm::aead::rand_core::RngCore;
        let mut buf = [0u8; 12];
        OsRng.fill_bytes(&mut buf);
        buf
    };
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| format!("Encrypt: {}", e))?;

    // Prepend nonce to ciphertext
    let mut result = Vec::with_capacity(12 + ciphertext.len());
    result.extend_from_slice(&nonce_bytes);
    result.extend_from_slice(&ciphertext);
    Ok(result)
}

/// Decrypt data produced by encrypt().
/// Input: nonce (12 bytes) || ciphertext
fn decrypt(data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() < 13 {
        return Err("Data too short for decryption".to_string());
    }

    let key = derive_key();
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| format!("Cipher init: {}", e))?;

    let nonce = Nonce::from_slice(&data[..12]);
    let ciphertext = &data[12..];

    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| format!("Decrypt: {}", e))
}

/// Get the secure store directory path.
/// Uses "secure-dev" in debug mode to prevent dev/release cross-contamination.
fn secure_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let dir_name = if cfg!(debug_assertions) { "secure-dev" } else { "secure" };
    let dir = app_data.join(dir_name);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    // Restrict directory permissions to owner only
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    }

    Ok(dir)
}

/// Save encrypted data to a file in the secure store.
#[tauri::command]
pub fn secure_store_save(
    app: tauri::AppHandle,
    key: String,
    value: String,
) -> Result<(), String> {
    // Validate key (alphanumeric + underscore only)
    if !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err("Invalid key format".to_string());
    }

    let dir = secure_dir(&app)?;
    let path = dir.join(format!("{}.enc", key));
    let tmp_path = dir.join(format!("{}.enc.tmp", key));

    let encrypted = encrypt(value.as_bytes())?;

    // Write to temp, then atomic rename
    std::fs::write(&tmp_path, &encrypted).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp_path, &path).map_err(|e| e.to_string())?;

    // Restrict file permissions
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }

    Ok(())
}

/// Load and decrypt data from the secure store.
#[tauri::command]
pub fn secure_store_load(
    app: tauri::AppHandle,
    key: String,
) -> Result<Option<String>, String> {
    if !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err("Invalid key format".to_string());
    }

    let dir = secure_dir(&app)?;
    let path = dir.join(format!("{}.enc", key));

    if !path.exists() {
        return Ok(None);
    }

    let data = std::fs::read(&path).map_err(|e| e.to_string())?;
    let decrypted = decrypt(&data)?;
    let plaintext =
        String::from_utf8(decrypted).map_err(|e| format!("Invalid UTF-8: {}", e))?;

    Ok(Some(plaintext))
}

/// Delete a file from the secure store.
#[tauri::command]
pub fn secure_store_delete(
    app: tauri::AppHandle,
    key: String,
) -> Result<(), String> {
    if !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err("Invalid key format".to_string());
    }

    let dir = secure_dir(&app)?;
    let path = dir.join(format!("{}.enc", key));
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

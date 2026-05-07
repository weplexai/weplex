//! AES-256-GCM encryption for session activity notes.
//!
//! ## Threat model
//!
//! Notes contain agent-authored summaries of work — file paths, decisions,
//! sometimes fragments of code or sensitive identifiers. Plaintext on disk
//! means any process running as the user (info-stealer malware, accidental
//! cloud-sync, Time Machine without FileVault) reads them. Encryption raises
//! the bar to "attacker must also unlock the macOS Keychain" — which is the
//! same gate that already protects OAuth tokens, passwords, etc.
//!
//! ## Design
//!
//! - One symmetric key per profile, stored in macOS Keychain via the existing
//!   `keyring` crate (service = `com.weplex.app[.dev]`, account derived from
//!   profile id). Different profiles can't decrypt each other's notes.
//! - AES-256-GCM with a fresh 12-byte nonce per file write. The nonce is
//!   stored alongside the ciphertext in a JSON envelope.
//! - Authenticated Additional Data binds each file to its session id, so a
//!   ciphertext copied to a different `<sid>.json` fails to decrypt — closes
//!   the "swap files between sessions" mischief.
//! - The envelope keeps `updated_at` in plaintext so the stop-hook freshness
//!   check (hook_server.rs) can read it without needing the Keychain.

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use ring::digest::{digest, SHA256};
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};

/// On-disk file format. `v` and `updated_at` are intentionally plaintext —
/// `updated_at` drives the stop-hook freshness check, `v` exists so future
/// versions can refuse old envelopes cleanly.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EncryptedFile {
    pub v: u32,
    pub updated_at: u64,
    pub encrypted: EncryptedPayload,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EncryptedPayload {
    /// base64(12-byte AES-GCM nonce)
    pub nonce: String,
    /// base64(ciphertext + 16-byte GCM auth tag)
    pub ct: String,
}

pub const FORMAT_VERSION: u32 = 1;
const NONCE_LEN: usize = 12;

#[cfg(debug_assertions)]
const KEYCHAIN_SERVICE: &str = "com.weplex.app.dev";
#[cfg(not(debug_assertions))]
const KEYCHAIN_SERVICE: &str = "com.weplex.app";

// Keychain ACL on macOS — implementation note for future maintainers.
//
// The `keyring` crate's `apple-native` backend uses
// `SecKeychainAddGenericPassword`, which creates a `kSecClassGenericPassword`
// item with `kSecAttrAccessible = kSecAttrAccessibleWhenUnlocked` and an ACL
// limited to the binary that created the entry (the standard
// "trusted application" model).
//
// If the `keyring` major version is bumped, verify both invariants still
// hold — a relaxed default like `kSecAttrAccessibleAlways`, or an open ACL,
// would let any process running as the user read the key without prompt and
// break the encryption-at-rest property of this module. The dependency is
// pinned to an exact version in Cargo.toml for that reason.

/// Derive a stable 64-hex-char Keychain account fragment from a profile id.
/// `profile_id` is whatever TerminalView passes via `WEPLEX_PROFILE_ID` —
/// usually an absolute path to the profile config dir, or `"default"`.
/// Full SHA-256 (no truncation) — `kSecAttrAccount` accepts well over 256
/// bytes, so there's no reason to risk birthday collisions.
fn keychain_account(profile_id: &str) -> String {
    let h = digest(&SHA256, profile_id.as_bytes());
    let bytes = h.as_ref();
    let mut hex = String::with_capacity(64);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(hex, "{:02x}", b);
    }
    format!("notes-key-{}", hex)
}

/// Fetch an existing per-profile encryption key from Keychain. Returns `None`
/// when no entry exists — callers on the read path use this to render a
/// "🔒 locked" placeholder instead of silently creating a fresh key (which
/// would orphan all previously-encrypted notes).
pub fn get_key(profile_id: &str) -> Result<Option<[u8; 32]>, String> {
    let account = keychain_account(profile_id);
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, &account)
        .map_err(|e| format!("Keychain entry init: {}", e))?;
    match entry.get_password() {
        Ok(stored) => {
            let raw = B64
                .decode(stored.as_bytes())
                .map_err(|e| format!("Keychain key b64 decode: {}", e))?;
            if raw.len() != 32 {
                return Err(format!("Keychain key wrong length: {}", raw.len()));
            }
            let mut out = [0u8; 32];
            out.copy_from_slice(&raw);
            Ok(Some(out))
        }
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("Keychain get: {}", e)),
    }
}

/// Fetch-or-create per-profile encryption key. Use only on write paths.
/// Read paths must use `get_key` to avoid orphaning existing ciphertext when
/// the Keychain has been wiped or restored from a backup with no entry.
pub fn get_or_create_key(profile_id: &str) -> Result<[u8; 32], String> {
    if let Some(k) = get_key(profile_id)? {
        return Ok(k);
    }
    let account = keychain_account(profile_id);
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, &account)
        .map_err(|e| format!("Keychain entry init: {}", e))?;
    let mut key = [0u8; 32];
    SystemRandom::new()
        .fill(&mut key)
        .map_err(|e| format!("RNG fill: {}", e))?;
    entry
        .set_password(&B64.encode(key))
        .map_err(|e| format!("Keychain set: {}", e))?;
    Ok(key)
}

/// AAD bound to: format version + session id + `updated_at`. Forging
/// `updated_at` in the plaintext envelope therefore breaks decryption,
/// closing the "silence the Stop-hook by setting future updated_at" vector.
fn aad(session_id: &str, updated_at: u64) -> Vec<u8> {
    let mut v = Vec::with_capacity(40 + session_id.len());
    v.extend_from_slice(b"weplex-activity-v1|");
    v.extend_from_slice(session_id.as_bytes());
    v.push(b'|');
    v.extend_from_slice(updated_at.to_string().as_bytes());
    v
}

/// Encrypt `plaintext`. The caller supplies `updated_at`, which is bound into
/// the AAD and also written plaintext into the envelope by the caller.
pub fn encrypt(
    plaintext: &[u8],
    key: &[u8; 32],
    session_id: &str,
    updated_at: u64,
) -> Result<EncryptedPayload, String> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));

    let mut nonce_bytes = [0u8; NONCE_LEN];
    SystemRandom::new()
        .fill(&mut nonce_bytes)
        .map_err(|e| format!("RNG nonce: {}", e))?;
    let nonce = Nonce::from_slice(&nonce_bytes);

    let aad = aad(session_id, updated_at);
    let ct = cipher
        .encrypt(
            nonce,
            Payload {
                msg: plaintext,
                aad: &aad,
            },
        )
        .map_err(|e| format!("AES-GCM encrypt: {}", e))?;

    Ok(EncryptedPayload {
        nonce: B64.encode(nonce_bytes),
        ct: B64.encode(ct),
    })
}

/// Decrypt the envelope. Caller passes the *plaintext* `updated_at` from the
/// envelope; if it was tampered with, AAD won't match and decrypt fails.
pub fn decrypt(
    payload: &EncryptedPayload,
    key: &[u8; 32],
    session_id: &str,
    updated_at: u64,
) -> Result<Vec<u8>, String> {
    let nonce_bytes = B64
        .decode(payload.nonce.as_bytes())
        .map_err(|e| format!("Nonce b64: {}", e))?;
    if nonce_bytes.len() != NONCE_LEN {
        return Err("Bad nonce length".to_string());
    }
    let ct = B64
        .decode(payload.ct.as_bytes())
        .map_err(|e| format!("CT b64: {}", e))?;

    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let aad = aad(session_id, updated_at);
    cipher
        .decrypt(
            Nonce::from_slice(&nonce_bytes),
            Payload {
                msg: &ct,
                aad: &aad,
            },
        )
        .map_err(|_| {
            "AES-GCM decrypt failed (tampered, wrong key, wrong session, or forged updated_at)"
                .to_string()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k() -> [u8; 32] {
        let mut k = [0u8; 32];
        SystemRandom::new().fill(&mut k).unwrap();
        k
    }

    #[test]
    fn round_trip() {
        let key = k();
        let pt = br#"{"summary":"hello","files_changed":[],"decisions":[]}"#;
        let env = encrypt(pt, &key, "42", 1700).unwrap();
        let got = decrypt(&env, &key, "42", 1700).unwrap();
        assert_eq!(got, pt);
    }

    #[test]
    fn tamper_ct_detected() {
        let key = k();
        let env = encrypt(b"secret", &key, "42", 1700).unwrap();
        let mut bad = env.clone();
        let mut ct_bytes = B64.decode(&bad.ct).unwrap();
        ct_bytes[0] ^= 1;
        bad.ct = B64.encode(&ct_bytes);
        assert!(decrypt(&bad, &key, "42", 1700).is_err());
    }

    #[test]
    fn wrong_session_id_rejected() {
        let key = k();
        let env = encrypt(b"secret", &key, "42", 1700).unwrap();
        // AAD binds the file to its session — copying to another sid fails.
        assert!(decrypt(&env, &key, "43", 1700).is_err());
    }

    #[test]
    fn wrong_key_rejected() {
        let env = encrypt(b"secret", &k(), "42", 1700).unwrap();
        assert!(decrypt(&env, &k(), "42", 1700).is_err());
    }

    #[test]
    fn forged_updated_at_rejected() {
        // Closes the "edit plaintext updated_at to silence the Stop hook
        // forever" attack: AAD binds the field, so any change breaks decrypt.
        let key = k();
        let env = encrypt(b"secret", &key, "42", 1700).unwrap();
        assert!(decrypt(&env, &key, "42", 9_999_999_999).is_err());
    }

    #[test]
    fn keychain_account_stable() {
        let a = keychain_account("/Users/x/.claude-work");
        let b = keychain_account("/Users/x/.claude-work");
        assert_eq!(a, b);
        assert!(a.starts_with("notes-key-"));
        // Full SHA-256 hex = 64 chars — no birthday-collision exposure.
        assert_eq!(a.len(), "notes-key-".len() + 64);
    }

    #[test]
    fn keychain_account_differs_per_profile() {
        assert_ne!(
            keychain_account("/Users/x/.claude-work"),
            keychain_account("/Users/x/.claude")
        );
    }
}

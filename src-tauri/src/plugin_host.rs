//! Plugin Host — discovers, validates, and manages Weplex plugins.
//!
//! Plugins live in ~/.weplex/plugins/<id>/ with a weplex-plugin.json manifest.
//! The host reads manifests, tracks activation state, and provides IPC for
//! the frontend plugin loader.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Plugin manifest (weplex-plugin.json).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub icon: String,
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub license: String,
    /// Relative path to compiled JS entry point
    #[serde(default = "default_entry")]
    pub entry: String,
    /// Required permissions
    #[serde(default)]
    pub permissions: Vec<String>,
    /// Minimum Weplex version required
    #[serde(default)]
    pub min_deck_version: String,
    /// Whether this plugin provides a new session type
    #[serde(default)]
    pub session_type: Option<PluginSessionType>,
}

fn default_entry() -> String {
    "dist/index.js".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSessionType {
    #[serde(rename = "type")]
    pub type_name: String,
    pub label: String,
    pub icon: String,
}

/// Runtime state of an installed plugin.
#[derive(Debug, Clone, Serialize)]
pub struct PluginInfo {
    pub manifest: PluginManifest,
    pub path: String,
    pub active: bool,
    pub entry_path: String,
}

/// Plugins directory.
fn plugins_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    PathBuf::from(format!("{}/.weplex/plugins", home))
}

/// Discover all installed plugins by reading their manifests.
pub fn list_plugins() -> Vec<PluginInfo> {
    let dir = plugins_dir();
    if !dir.exists() {
        return Vec::new();
    }

    let mut plugins = Vec::new();
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let manifest_path = path.join("weplex-plugin.json");
        if !manifest_path.exists() {
            continue;
        }

        let content = match std::fs::read_to_string(&manifest_path) {
            Ok(c) => c,
            Err(e) => {
                log::warn!("failed to read plugin manifest {:?}: {}", manifest_path, e);
                continue;
            }
        };

        let manifest: PluginManifest = match serde_json::from_str(&content) {
            Ok(m) => m,
            Err(e) => {
                log::warn!("invalid plugin manifest {:?}: {}", manifest_path, e);
                continue;
            }
        };

        // Check if active (activation state stored in config)
        let active = is_plugin_active(&manifest.id);

        // Canonicalize entry path and ensure it stays inside the plugin directory.
        // This prevents a malicious manifest like `"entry": "../../etc/passwd"`
        // from escaping the plugin sandbox.
        let raw_entry = path.join(&manifest.entry);
        let canonical_entry = match std::fs::canonicalize(&raw_entry) {
            Ok(p) => p,
            Err(e) => {
                log::warn!("plugin entry path invalid {:?}: {}", raw_entry, e);
                continue;
            }
        };
        let canonical_plugin_dir = match std::fs::canonicalize(&path) {
            Ok(p) => p,
            Err(_) => continue,
        };
        if !canonical_entry.starts_with(&canonical_plugin_dir) {
            log::warn!(
                "plugin entry escapes plugin dir: {:?} -> {:?}",
                manifest.id, canonical_entry
            );
            continue;
        }
        let entry_path = canonical_entry.to_string_lossy().to_string();

        plugins.push(PluginInfo {
            manifest,
            path: path.to_string_lossy().to_string(),
            active,
            entry_path,
        });
    }

    plugins.sort_by(|a, b| a.manifest.name.cmp(&b.manifest.name));
    plugins
}

/// Check if a plugin is activated (persisted in ~/.weplex/plugin-state.json).
fn is_plugin_active(plugin_id: &str) -> bool {
    let state = load_plugin_state();
    state.get(plugin_id).copied().unwrap_or(false)
}

/// Activate a plugin.
pub fn activate_plugin(plugin_id: &str) -> Result<(), String> {
    let mut state = load_plugin_state();
    state.insert(plugin_id.to_string(), true);
    save_plugin_state(&state)
}

/// Deactivate a plugin.
pub fn deactivate_plugin(plugin_id: &str) -> Result<(), String> {
    let mut state = load_plugin_state();
    state.insert(plugin_id.to_string(), false);
    save_plugin_state(&state)
}

/// Load activation state from disk.
fn load_plugin_state() -> HashMap<String, bool> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    let path = format!("{}/.weplex/plugin-state.json", home);
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_default()
}

/// Save activation state to disk.
fn save_plugin_state(state: &HashMap<String, bool>) -> Result<(), String> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    let dir = format!("{}/.weplex", home);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = format!("{}/plugin-state.json", dir);
    let content = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    std::fs::write(&path, content).map_err(|e| e.to_string())
}

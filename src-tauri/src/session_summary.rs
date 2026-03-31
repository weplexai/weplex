use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub summary: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files_changed: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decisions: Vec<String>,
    pub updated_at: u64,
}

/// Return the path to ~/.weplex/summaries/
pub fn summaries_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home).join(".weplex/summaries")
}

/// Read a session summary from disk.
pub fn read_summary(session_id: &str) -> Option<SessionSummary> {
    let path = summaries_dir().join(format!("{}.json", session_id));
    let content = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Remove summary files older than 7 days.
pub fn cleanup_old_summaries() {
    let dir = summaries_dir();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        let cutoff = std::time::SystemTime::now()
            .checked_sub(std::time::Duration::from_secs(7 * 24 * 3600));
        let Some(cutoff) = cutoff else { return };

        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if let Ok(modified) = meta.modified() {
                    if modified < cutoff {
                        let _ = std::fs::remove_file(entry.path());
                    }
                }
            }
        }
    }
}

/// Ensure the summaries directory exists.
pub fn ensure_summaries_dir() {
    let dir = summaries_dir();
    let _ = std::fs::create_dir_all(&dir);
}

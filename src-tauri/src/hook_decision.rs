//! Per-session stop-hook attempt tracking.
//!
//! The Stop hook is fault-tolerant by construction: every decision to ask the
//! agent for notes is taken here and capped at 1 ask per (Weplex session,
//! process lifetime). If anything downstream fails — tool missing from the
//! agent's cached tool list, MCP server crashed, server decided wrong — the
//! next Stop event silently resolves to `ExitOk` instead of retrying.
//!
//! In-memory only. The cap resets when Weplex restarts, which is the desired
//! behaviour: each Weplex launch gets one fresh attempt per session.

use std::collections::HashMap;
use std::sync::Mutex;

/// Maximum asks per Weplex session per Weplex process lifetime.
pub const MAX_ASKS_PER_SESSION: u8 = 1;

/// Shared state registered via Tauri `.manage()`.
pub struct HookDecisionState {
    attempts: Mutex<HashMap<u32, u8>>,
}

impl HookDecisionState {
    pub fn new() -> Self {
        Self {
            attempts: Mutex::new(HashMap::new()),
        }
    }

    /// Try to reserve an ask for the given session. Returns true if the caller
    /// is allowed to ask, false if the cap has been reached.
    ///
    /// Must be called BEFORE the ask is emitted so that a crash after this
    /// point still costs the session an attempt (fail-safe, no re-ask storm).
    pub fn try_reserve_ask(&self, session_id: u32) -> bool {
        let mut map = match self.attempts.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        let entry = map.entry(session_id).or_insert(0);
        if *entry >= MAX_ASKS_PER_SESSION {
            return false;
        }
        *entry += 1;
        true
    }

}

impl Default for HookDecisionState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_at_one_per_session() {
        let s = HookDecisionState::new();
        assert!(s.try_reserve_ask(1));
        assert!(!s.try_reserve_ask(1));
        assert!(!s.try_reserve_ask(1));
    }

    #[test]
    fn independent_sessions() {
        let s = HookDecisionState::new();
        assert!(s.try_reserve_ask(1));
        assert!(s.try_reserve_ask(2));
        assert!(!s.try_reserve_ask(1));
        assert!(!s.try_reserve_ask(2));
    }

    #[test]
    fn reservation_is_atomic_across_threads() {
        use std::sync::Arc;
        use std::thread;
        let s = Arc::new(HookDecisionState::new());
        let handles: Vec<_> = (0..32)
            .map(|_| {
                let s = Arc::clone(&s);
                thread::spawn(move || s.try_reserve_ask(42))
            })
            .collect();
        let allowed: usize = handles.into_iter().filter_map(|h| h.join().ok()).filter(|b| *b).count();
        assert_eq!(allowed, 1, "exactly one thread should win the reservation");
    }
}

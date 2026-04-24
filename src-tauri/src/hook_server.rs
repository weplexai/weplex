//! Lightweight local HTTP server that receives Claude Code hook events.
//!
//! Hook scripts (PreToolUse, PostToolUse, Stop) POST JSON payloads here.
//! The server resolves session_id from the payload and emits Tauri events
//! to the frontend for real-time UI updates.
//!
//! Security: Bearer token auth required. Token generated at startup,
//! written to ~/.weplex/hook-token (mode 0600).

use crate::hook_decision::HookDecisionState;
use crate::session_summary;
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::sync::Arc;
use tauri::{Emitter, Manager};
use weplex_mcp_contract::{StopDecision, TOOL_LOG_ACTIVITY};

/// If an activity entry was written less than this many seconds ago, don't
/// ask again. Covers "agent called the tool in the very same turn it's
/// stopping" case.
const NOTE_FRESHNESS_SECS: u64 = 120;

/// Message passed to the agent when we do ask for an activity entry. Kept
/// short so it doesn't crowd the conversation. Framed as a message from the
/// user to future-you, not a surveillance nudge.
const REQUEST_NOTES_MESSAGE: &str = "Before you stop, call weplex_log_activity with a 1-3 sentence summary of what you accomplished plus any files you changed. This is a personal session journal — future you (or another Claude run in this same Weplex session) can read it back to pick up context.";

/// Hook event types matching Claude Code hook lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum HookEventType {
    PreToolUse,
    PostToolUse,
    Stop,
    SubagentStart,
    SubagentStop,
    SessionStart,
}

/// Payload sent by hook scripts to the Weplex HTTP listener.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookEvent {
    /// Hook type (pre_tool_use, post_tool_use, stop)
    pub event_type: HookEventType,
    /// Weplex session ID (resolved by hook script from session-map)
    pub session_id: u32,
    /// Tool name (e.g. "Write", "Edit", "Bash", "Read")
    pub tool_name: Option<String>,
    /// File path affected by the tool (for Write/Edit/Read)
    pub file_path: Option<String>,
    /// Working directory of the Claude session
    pub cwd: Option<String>,
    /// Tool input (truncated for large payloads)
    pub tool_input: Option<String>,
    /// Tool output/result (truncated, PostToolUse only)
    pub tool_output: Option<String>,
    /// Sub-agent type (e.g. "Explore", "Plan", "Bash") for SubagentStart/Stop
    pub agent_type: Option<String>,
    /// Sub-agent unique ID for matching start/stop events
    pub agent_id: Option<String>,
    /// Claude Code session UUID (from SessionStart hook)
    pub claude_session_id: Option<String>,
}

/// Tauri event payload emitted to the frontend.
#[derive(Clone, Serialize)]
pub struct HookEventPayload {
    pub event_type: HookEventType,
    pub session_id: u32,
    pub tool_name: Option<String>,
    pub file_path: Option<String>,
    pub cwd: Option<String>,
    pub tool_input: Option<String>,
    pub tool_output: Option<String>,
    pub agent_type: Option<String>,
    pub agent_id: Option<String>,
    pub claude_session_id: Option<String>,
    pub timestamp: u64,
}

/// Generate a cryptographically random hex token (32 bytes = 64 hex chars).
fn generate_token() -> String {
    use std::fmt::Write;
    let mut bytes = [0u8; 32];
    // Use /dev/urandom on Unix
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        let _ = std::io::Read::read_exact(&mut f, &mut bytes);
    } else {
        // Fallback: use timestamp + pid (less secure but functional)
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = ((seed >> (i % 16 * 8)) & 0xff) as u8;
        }
    }
    let mut hex = String::with_capacity(64);
    for b in &bytes {
        let _ = write!(hex, "{:02x}", b);
    }
    hex
}

/// Start the hook HTTP listener on a random localhost port.
/// Returns the port number. Writes port and auth token to ~/.weplex/.
pub fn start_hook_server(app_handle: tauri::AppHandle) -> Result<u16, String> {
    let server = tiny_http::Server::http("127.0.0.1:0")
        .map_err(|e| format!("Failed to start hook server: {}", e))?;

    let port = server
        .server_addr()
        .to_ip()
        .ok_or("Failed to get server address")?
        .port();

    // Generate auth token and write both port and token
    let token = generate_token();
    write_hook_files(port, &token)?;

    let server = Arc::new(server);
    let expected_token = format!("Bearer {}", token);

    log::info!("hook server listening on 127.0.0.1:{}", port);

    // Spawn listener thread
    let app = app_handle.clone();
    std::thread::spawn(move || {
        run_hook_server(server, app, &expected_token);
    });

    Ok(port)
}

/// Write port and token files with secure permissions (0600).
///
/// Atomic writes: the token file is sensitive (any local process that reads
/// it can forge hook events), so we create it owner-only from birth rather
/// than via a post-hoc chmod that leaves a sub-millisecond window where the
/// file is world-readable under default umask.
fn write_hook_files(port: u16, token: &str) -> Result<(), String> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    let weplex_dir = format!("{}/.weplex", home);
    std::fs::create_dir_all(&weplex_dir)
        .map_err(|e| format!("Failed to create ~/.weplex: {}", e))?;

    // Harden directory permissions (owner-only) so other local users
    // cannot list files inside, even if umask is permissive.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&weplex_dir, std::fs::Permissions::from_mode(0o700));
    }

    let port_path = format!("{}/hook-port", weplex_dir);
    let token_path = format!("{}/hook-token", weplex_dir);

    crate::utils::atomic_write_owner_only(&port_path, &port.to_string())?;
    crate::utils::atomic_write_owner_only(&token_path, token)?;

    Ok(())
}

/// Clean up hook-port and hook-token files on shutdown.
pub fn cleanup_hook_files() {
    let home = std::env::var("HOME").unwrap_or_default();
    let _ = std::fs::remove_file(format!("{}/.weplex/hook-port", home));
    let _ = std::fs::remove_file(format!("{}/.weplex/hook-token", home));
}

/// Current unix time in seconds.
fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Decide what to tell the stop hook for a given Weplex session.
///
/// Fault-tolerance rules (enforced in `decide_stop_with_summary`):
/// - If the agent already wrote a note within `NOTE_FRESHNESS_SECS`, return
///   `ExitOk` WITHOUT consuming an ask slot.
/// - Otherwise attempt to reserve an ask slot. Cap is 1 ask per (session,
///   process lifetime). Reservation mutates state before the function returns
///   — even if something panics afterwards, the next call returns `ExitOk`
///   and no loop can form.
/// - All other failure modes are handled on the bash side (non-200 response,
///   timeout, malformed body → `exit 0`).
pub fn decide_stop(state: &HookDecisionState, session_id: u32) -> StopDecision {
    let summary = session_summary::read_summary(&session_id.to_string());
    decide_stop_with_summary(state, session_id, summary.as_ref(), now_secs())
}

/// Pure decision function. Split out so unit tests don't need to manipulate
/// `$HOME` or the global summaries directory to cover all paths.
pub fn decide_stop_with_summary(
    state: &HookDecisionState,
    session_id: u32,
    summary: Option<&session_summary::SessionSummary>,
    now: u64,
) -> StopDecision {
    if let Some(s) = summary {
        if now.saturating_sub(s.updated_at) < NOTE_FRESHNESS_SECS {
            return StopDecision::ExitOk;
        }
    }

    if !state.try_reserve_ask(session_id) {
        return StopDecision::ExitOk;
    }

    StopDecision::RequestNotes {
        tool: TOOL_LOG_ACTIVITY.to_string(),
        message: REQUEST_NOTES_MESSAGE.to_string(),
    }
}

/// Validate the method, path, and Authorization header of an incoming
/// request. Returns `Ok(())` for a valid POST /hook with the expected bearer,
/// or `Err((status, body))` otherwise.
///
/// Extracted so routing/auth regressions are caught by unit tests — a
/// refactor that accidentally drops the token check would silently let any
/// local process push fake hook events without this safety net.
pub fn classify_request(
    method: &str,
    url: &str,
    auth_header: Option<&str>,
    expected_token: &str,
) -> Result<(), (u16, &'static str)> {
    if method != "POST" {
        return Err((405, "Method not allowed"));
    }
    if url != "/hook" {
        return Err((404, "Not found"));
    }
    match auth_header {
        Some(h) if h == expected_token => Ok(()),
        _ => Err((401, "Unauthorized")),
    }
}

/// Render the HTTP response body + Content-Type for an incoming hook event.
///
/// Extracted from `run_hook_server` so the wire format can be covered by
/// unit tests without spinning up a Tauri app. Stop events carry a
/// `StopDecision` payload (`Some(...)`); all other events produce the
/// lightweight `"ok"` body the bash hooks don't inspect.
pub fn render_hook_response(decision: Option<StopDecision>) -> (String, &'static str) {
    match decision {
        Some(d) => {
            // `StopDecision` only has `String` / unit variants; serialization
            // cannot fail in practice, so we use `expect` rather than a
            // hardcoded fallback that would silently drift if the enum grows.
            let json = serde_json::to_string(&d).expect("StopDecision serialization is infallible");
            (json, "application/json")
        }
        None => ("ok".to_string(), "text/plain"),
    }
}

/// Main server loop — processes incoming hook events.
fn run_hook_server(server: Arc<tiny_http::Server>, app: tauri::AppHandle, expected_token: &str) {
    loop {
        let mut request = match server.recv() {
            Ok(req) => req,
            Err(_) => break, // Server shut down
        };

        // Validate method, path, and bearer token. Any failure → short-circuit
        // with the error body; the actual decision logic below is only reached
        // for authenticated POST /hook requests.
        let method_str = request.method().as_str().to_string();
        let url_str = request.url().to_string();
        let auth_header = request
            .headers()
            .iter()
            .find(|h| h.field.equiv("Authorization"))
            .map(|h| h.value.as_str().to_string());

        if let Err((status, body)) = classify_request(
            &method_str,
            &url_str,
            auth_header.as_deref(),
            expected_token,
        ) {
            let response = tiny_http::Response::from_string(body).with_status_code(status);
            let _ = request.respond(response);
            continue;
        }

        // Read body (limit to 64KB to prevent abuse)
        let mut body = String::new();
        let mut reader = request.as_reader().take(65_536);
        if reader.read_to_string(&mut body).is_err() {
            let response = tiny_http::Response::from_string("Bad request")
                .with_status_code(400);
            let _ = request.respond(response);
            continue;
        }

        // Parse hook event
        match serde_json::from_str::<HookEvent>(&body) {
            Ok(event) => {
                let payload = HookEventPayload {
                    event_type: event.event_type.clone(),
                    session_id: event.session_id,
                    tool_name: event.tool_name.clone(),
                    file_path: event.file_path.clone(),
                    cwd: event.cwd.clone(),
                    tool_input: event.tool_input.clone(),
                    tool_output: event.tool_output.clone(),
                    agent_type: event.agent_type.clone(),
                    agent_id: event.agent_id.clone(),
                    claude_session_id: event.claude_session_id.clone(),
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64,
                };

                log::debug!(
                    "hook event: {:?} session={} tool={:?}",
                    event.event_type, event.session_id, event.tool_name
                );

                // Emit to frontend
                let _ = app.emit("hook-event", payload);

                // Stop events: respond with a courier decision so the bash
                // script knows whether to emit a block. Other events stay
                // on the lightweight "ok" body for backward compat.
                let decision_opt: Option<StopDecision> =
                    if event.event_type == HookEventType::Stop {
                        match app.try_state::<HookDecisionState>() {
                            Some(state) => Some(decide_stop(state.inner(), event.session_id)),
                            None => Some(StopDecision::ExitOk),
                        }
                    } else {
                        None
                    };
                let (body, content_type) = render_hook_response(decision_opt);

                let response = tiny_http::Response::from_string(body)
                    .with_status_code(200)
                    .with_header(
                        tiny_http::Header::from_bytes(&b"Content-Type"[..], content_type.as_bytes())
                            .unwrap(),
                    );
                let _ = request.respond(response);
            }
            Err(e) => {
                log::warn!("invalid hook payload: {}", e);
                let response = tiny_http::Response::from_string(format!("Invalid JSON: {}", e))
                    .with_status_code(400);
                let _ = request.respond(response);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_summary::SessionSummary;

    fn fresh_summary(at: u64) -> SessionSummary {
        SessionSummary {
            summary: String::new(),
            files_changed: vec![],
            decisions: vec![],
            updated_at: at,
            notes: vec![],
        }
    }

    fn is_request_notes(d: &StopDecision) -> bool {
        matches!(d, StopDecision::RequestNotes { .. })
    }

    fn is_exit_ok(d: &StopDecision) -> bool {
        matches!(d, StopDecision::ExitOk)
    }

    #[test]
    fn no_summary_first_call_asks_second_is_silent() {
        let s = HookDecisionState::new();
        let d1 = decide_stop_with_summary(&s, 1, None, 1_000_000);
        let d2 = decide_stop_with_summary(&s, 1, None, 1_000_001);
        assert!(is_request_notes(&d1));
        assert!(is_exit_ok(&d2));
    }

    #[test]
    fn stale_summary_asks_once_then_silent() {
        let s = HookDecisionState::new();
        let stale = fresh_summary(1_000_000 - NOTE_FRESHNESS_SECS - 1);
        let d1 = decide_stop_with_summary(&s, 1, Some(&stale), 1_000_000);
        let d2 = decide_stop_with_summary(&s, 1, Some(&stale), 1_000_001);
        assert!(is_request_notes(&d1));
        assert!(is_exit_ok(&d2));
    }

    #[test]
    fn fresh_summary_returns_exit_ok_without_reserving() {
        let s = HookDecisionState::new();
        // Recent note → no ask needed.
        let fresh = fresh_summary(1_000_000);
        let d = decide_stop_with_summary(&s, 1, Some(&fresh), 1_000_000);
        assert!(is_exit_ok(&d));

        // Slot must still be available: a later stale summary should ask.
        let stale = fresh_summary(0);
        let d_after = decide_stop_with_summary(&s, 1, Some(&stale), 1_000_000);
        assert!(is_request_notes(&d_after), "freshness path must not consume the reservation");
    }

    #[test]
    fn reservation_persists_even_if_summary_becomes_fresh_between_calls() {
        // Once the slot is used, no amount of fresh/stale summary churn can
        // cause another ask. This is the core anti-loop invariant.
        let s = HookDecisionState::new();
        let d1 = decide_stop_with_summary(&s, 7, None, 1_000_000);
        assert!(is_request_notes(&d1));

        // Agent writes a fresh note — next stop is silent (freshness path).
        let fresh = fresh_summary(1_000_050);
        let d2 = decide_stop_with_summary(&s, 7, Some(&fresh), 1_000_060);
        assert!(is_exit_ok(&d2));

        // Much later, the freshness window expires. We must STILL be silent —
        // the session already spent its one ask.
        let old = fresh_summary(1_000_050);
        let d3 = decide_stop_with_summary(&s, 7, Some(&old), 1_000_050 + NOTE_FRESHNESS_SECS + 10);
        assert!(is_exit_ok(&d3), "second ask in same session must never happen");
    }

    #[test]
    fn cross_session_isolation() {
        let s = HookDecisionState::new();
        // Exhaust session 1.
        assert!(is_request_notes(&decide_stop_with_summary(&s, 1, None, 1_000_000)));
        assert!(is_exit_ok(&decide_stop_with_summary(&s, 1, None, 1_000_000)));
        // Session 2 still has its own slot.
        assert!(is_request_notes(&decide_stop_with_summary(&s, 2, None, 1_000_000)));
    }

    #[test]
    fn freshness_boundary_is_strictly_less_than_window() {
        let s = HookDecisionState::new();
        // Exactly at the boundary: not "less than" → should ask.
        let at_boundary = fresh_summary(1_000_000 - NOTE_FRESHNESS_SECS);
        let d = decide_stop_with_summary(&s, 1, Some(&at_boundary), 1_000_000);
        assert!(is_request_notes(&d));
    }

    /// End-to-end wire-format test: simulates the path a POST /hook takes
    /// inside `run_hook_server` for a Stop event, bypassing only the tiny_http
    /// transport itself. Parses the response body back through the contract
    /// type so a field rename in `StopDecision` fails this test cleanly
    /// instead of slipping past a substring match.
    #[test]
    fn wire_format_two_stops_same_session_asks_then_silent() {
        let state = HookDecisionState::new();
        let sid: u32 = 42;

        // First stop on sid — no prior summary.
        let d1 = decide_stop_with_summary(&state, sid, None, 1_000_000);
        let (body1, ct1) = render_hook_response(Some(d1));
        assert_eq!(ct1, "application/json");
        let parsed1: StopDecision =
            serde_json::from_str(&body1).expect("body1 must deserialize into StopDecision");
        match parsed1 {
            StopDecision::RequestNotes { tool, message } => {
                assert_eq!(tool, TOOL_LOG_ACTIVITY);
                assert!(!message.is_empty(), "message must be non-empty");
            }
            StopDecision::ExitOk => panic!("first stop should request notes"),
        }

        // Second stop same sid — must deserialize to ExitOk.
        let d2 = decide_stop_with_summary(&state, sid, None, 1_000_001);
        let (body2, ct2) = render_hook_response(Some(d2));
        assert_eq!(ct2, "application/json");
        let parsed2: StopDecision =
            serde_json::from_str(&body2).expect("body2 must deserialize into StopDecision");
        assert!(matches!(parsed2, StopDecision::ExitOk));
    }

    #[test]
    fn wire_format_non_stop_events_return_plain_ok() {
        let (body, ct) = render_hook_response(None);
        assert_eq!(body, "ok");
        assert_eq!(ct, "text/plain");
    }

    // ── classify_request ──────────────────────────────────────────────────
    //
    // Every branch matters: a regression that silently accepts GET, or skips
    // the token check, would let any local process push fake hook events and
    // trigger the agent-facing `block` response.

    const TOKEN: &str = "Bearer deadbeef";

    #[test]
    fn classify_happy_path() {
        assert!(classify_request("POST", "/hook", Some(TOKEN), TOKEN).is_ok());
    }

    #[test]
    fn classify_rejects_non_post_method() {
        for m in ["GET", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"] {
            assert_eq!(
                classify_request(m, "/hook", Some(TOKEN), TOKEN),
                Err((405, "Method not allowed")),
                "method {m} must be rejected",
            );
        }
    }

    #[test]
    fn classify_rejects_wrong_path() {
        for p in ["/", "/hooks", "/hook/", "/HOOK", "/hook?x=1", ""] {
            assert_eq!(
                classify_request("POST", p, Some(TOKEN), TOKEN),
                Err((404, "Not found")),
                "path {p} must be rejected",
            );
        }
    }

    #[test]
    fn classify_rejects_missing_token() {
        assert_eq!(
            classify_request("POST", "/hook", None, TOKEN),
            Err((401, "Unauthorized"))
        );
    }

    #[test]
    fn classify_rejects_wrong_token() {
        assert_eq!(
            classify_request("POST", "/hook", Some("Bearer wrong"), TOKEN),
            Err((401, "Unauthorized"))
        );
    }

    #[test]
    fn classify_rejects_bare_token_without_bearer_prefix() {
        // Even if the hex matches, the header must be the full `Bearer <hex>`.
        assert_eq!(
            classify_request("POST", "/hook", Some("deadbeef"), TOKEN),
            Err((401, "Unauthorized"))
        );
    }

    #[test]
    fn future_updated_at_does_not_underflow() {
        // Clock skew: summary's updated_at is in the "future" relative to now.
        // saturating_sub should clamp to 0, which is less than the window,
        // so we should take the freshness path (ExitOk) without panicking.
        let s = HookDecisionState::new();
        let future = fresh_summary(2_000_000);
        let d = decide_stop_with_summary(&s, 1, Some(&future), 1_000_000);
        assert!(is_exit_ok(&d));
    }
}

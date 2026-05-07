// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(clippy::too_many_arguments)]

mod agents;
mod claude;
mod commands;
mod compiler;
mod context;
mod git;
mod guard;
mod hook_decision;
mod hook_server;
mod hooks;
mod ipc_server;
mod keychain;
mod lockfile;
mod manifest;
mod marketplace;
mod mcp;
mod notes_crypto;
mod oauth_server;
mod platform;
mod plugin_host;
mod plugins;
mod profile;
mod pty_manager;
mod resources;
mod secure_store;
mod session_summary;
mod skills;
mod store;
mod utils;
mod yaml;

use log::{info, error};
use pty_manager::PtyManager;
use std::sync::Mutex;
use tauri::{Manager, State};

pub(crate) struct AppState {
    pty_manager: std::sync::Arc<Mutex<PtyManager>>,
    ipc_pool: Mutex<ipc_server::IpcSocketPool>,
}

// ── PTY commands (require State<AppState>) ────────────────────────────────

#[tauri::command]
fn create_pty(
    state: State<AppState>,
    app: tauri::AppHandle,
    session_id: u32,
    cols: u16,
    rows: u16,
    command: Option<String>,
    cwd: Option<String>,
    env_vars: Option<std::collections::HashMap<String, String>>,
) -> Result<(), String> {
    if let Some(ref cwd_path) = cwd {
        let _ = write_session_map(session_id, cwd_path);
    }
    let mut manager = state.pty_manager.lock().map_err(|e| e.to_string())?;
    manager
        .create(session_id, cols, rows, command, cwd, env_vars, app)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn write_pty(state: State<AppState>, session_id: u32, data: String) -> Result<(), String> {
    let mut manager = state.pty_manager.lock().map_err(|e| e.to_string())?;
    manager.write(session_id, &data).map_err(|e| e.to_string())
}

#[tauri::command]
fn resize_pty(state: State<AppState>, session_id: u32, cols: u16, rows: u16) -> Result<(), String> {
    let mut manager = state.pty_manager.lock().map_err(|e| e.to_string())?;
    manager
        .resize(session_id, cols, rows)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn kill_pty(state: State<AppState>, session_id: u32) -> Result<(), String> {
    let mut manager = state.pty_manager.lock().map_err(|e| e.to_string())?;
    manager.kill(session_id).map_err(|e| e.to_string())
}

/// Clean stale session-map files on startup.
///
/// Only removes files older than 1 hour to avoid racing with `write_session_map`
/// that may run concurrently on the main thread when the frontend boots up
/// immediately and starts creating sessions before this background task fires.
fn clean_session_map() -> Result<(), String> {
    let home = utils::get_home();
    let map_dir = format!("{}/.weplex/session-map", home);
    let Ok(entries) = std::fs::read_dir(&map_dir) else {
        return Ok(());
    };
    let now = std::time::SystemTime::now();
    let one_hour = std::time::Duration::from_secs(3600);
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        let Ok(modified) = meta.modified() else { continue };
        if now.duration_since(modified).unwrap_or_default() > one_hour {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    Ok(())
}

/// Write cwd → session_id mapping for stop hook resolution.
fn write_session_map(session_id: u32, cwd: &str) -> Result<(), String> {
    let home = utils::get_home();
    let map_dir = format!("{}/.weplex/session-map", home);
    std::fs::create_dir_all(&map_dir).map_err(|e| e.to_string())?;
    let normalized = if cwd.starts_with(&home) {
        format!("~{}", &cwd[home.len()..])
    } else {
        cwd.to_string()
    };
    let encoded = normalized.replace('/', "_");
    let map_path = format!("{}/{}", map_dir, encoded);
    std::fs::write(&map_path, session_id.to_string()).map_err(|e| e.to_string())?;
    Ok(())
}

// ── Plugin/browser thin wrappers ──────────────────────────────────────────

#[tauri::command]
fn list_installed_plugins() -> Vec<plugin_host::PluginInfo> {
    plugin_host::list_plugins()
}

#[tauri::command]
fn activate_plugin(plugin_id: String) -> Result<(), String> {
    plugin_host::activate_plugin(&plugin_id)
}

#[tauri::command]
fn deactivate_plugin(plugin_id: String) -> Result<(), String> {
    plugin_host::deactivate_plugin(&plugin_id)
}

#[tauri::command]
fn browser_detect() -> Vec<plugins::browser::BrowserInfo> {
    plugins::browser::detect_browsers()
}

#[tauri::command]
fn browser_launch(browser: String, url: String) -> Result<serde_json::Value, String> {
    // Validate URL: only https:// and http://localhost allowed
    let is_safe = url.starts_with("https://")
        || url.starts_with("http://localhost")
        || url.starts_with("http://127.0.0.1");
    if !is_safe {
        return Err("Blocked: only https:// and http://localhost URLs are allowed".to_string());
    }
    if url.chars().any(|c| matches!(c, '`' | '$' | '|' | ';' | '&' | '\n' | '\r' | '"' | '\'' | '\\' | '<' | '>' | '(' | ')')) {
        return Err("Blocked: URL contains invalid characters".to_string());
    }
    let port = plugins::browser::next_cdp_port();
    let pid = plugins::browser::launch_browser(&browser, port, &url)?;
    Ok(serde_json::json!({ "pid": pid, "port": port }))
}

// ── Application entry point ───────────────────────────────────────────────

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("weplex=info"))
        .init();

    tauri::Builder::default()
        .manage(AppState {
            pty_manager: std::sync::Arc::new(Mutex::new(PtyManager::new())),
            ipc_pool: Mutex::new(ipc_server::IpcSocketPool::new()),
        })
        .manage(hook_decision::HookDecisionState::new())
        .setup(|app| {
            // Clean up stale socket files from previous crashes
            ipc_server::IpcSocketPool::cleanup_stale_socket_files();

            // Migrate pre-pivot ~/.weplex/summaries/ → ~/.weplex/activity/
            // (idempotent, no-op after first run). Must run BEFORE ensure_activity_dir
            // so the atomic-rename happy path can fire.
            session_summary::migrate_summaries_to_activity();
            session_summary::ensure_activity_dir();
            session_summary::cleanup_old_activity();

            // Start global MCP socket for cross-session tools (MCP v2)
            {
                let state: tauri::State<AppState> = app.state();
                let pty_arc = std::sync::Arc::clone(&state.pty_manager);
                let app_handle = app.handle().clone();
                match state.ipc_pool.lock() {
                    Ok(mut pool) => match pool.start_global_socket(pty_arc, app_handle) {
                        Ok(path) => info!("global MCP socket started: {}", path),
                        Err(e) => error!("failed to start global MCP socket: {}", e),
                    },
                    Err(e) => error!("ipc_pool mutex poisoned on setup: {}", e),
                }
            }

            // Start hook event listener (must be before hook registration)
            let hook_handle = app.handle().clone();
            match hook_server::start_hook_server(hook_handle) {
                Ok(port) => info!("hook server started on port {}", port),
                Err(e) => error!("failed to start hook server: {}", e),
            }

            // Register MCP server, generate hook scripts and source.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let _ = mcp::do_register_mcp_in_claude(&handle);
                let _ = hooks::ensure_hook_script();
                let _ = hooks::write_weplex_hooks_source();
                let _ = clean_session_map();
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // PTY
            create_pty,
            write_pty,
            resize_pty,
            kill_pty,
            // Claude integration
            claude::get_claude_usage,
            claude::get_claude_state,
            claude::get_session_summary,
            // Profile & directory
            profile::list_dirs,
            profile::discover_profiles,
            // Agents
            agents::list_agents,
            agents::list_project_agents,
            // Commands
            commands::ensure_default_commands,
            commands::list_commands,
            commands::save_command,
            commands::delete_command,
            // Cross-agent manifests + compiler
            manifest::list_profile_manifests,
            compiler::compile_profile_to_external_agents,
            compiler::dry_run_compile_profile,
            // Lockfile (Phase 3)
            lockfile::read_lockfile,
            lockfile::restore_resource_version,
            // Cross-agent guard
            guard::scan_resource,
            guard::scan_profile,
            guard::scan_mcp_server,
            guard::set_override_decision,
            guard::list_overrides,
            // Git & project config
            git::get_project_config,
            git::get_git_branch,
            git::get_git_status,
            // Context injection
            context::inject_context_block,
            // Skills
            skills::list_skills,
            skills::read_skill_content,
            // Persistent store
            store::persist_store,
            store::load_store,
            // OAuth
            oauth_server::start_oauth_server,
            // Platform
            platform::open_url,
            // Marketplace
            marketplace::save_marketplace_package,
            marketplace::save_marketplace_skill,
            // Plugins & browser
            list_installed_plugins,
            activate_plugin,
            deactivate_plugin,
            browser_detect,
            browser_launch,
            // Keychain & secure store
            keychain::keychain_save,
            keychain::keychain_load,
            keychain::keychain_delete,
            secure_store::secure_store_save,
            secure_store::secure_store_load,
            secure_store::secure_store_delete,
            // MCP
            mcp::get_mcp_binary_path,
            mcp::register_mcp_in_claude,
            // Hooks
            hooks::sync_hooks_for_profiles,
            hooks::sync_hooks_for_profile,
            // Resources
            profile::discover_resources,
            profile::count_profile_resources,
            profile::copy_resource_to_profile,
            profile::copy_all_resources_to_profile,
            profile::create_resource_in_profile,
            profile::delete_resource_file,
            // macOS
            platform::set_traffic_lights_visible,
        ])
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_notification::init())
        .build(tauri::generate_context!())
        .expect("error while building Weplex")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                let state: State<AppState> = app.state();
                match state.ipc_pool.lock() {
                    Ok(mut pool) => pool.cleanup_for_exit(),
                    Err(e) => error!("ipc_pool mutex poisoned on exit: {}", e),
                }
                hook_server::cleanup_hook_files();
            }
        });
}

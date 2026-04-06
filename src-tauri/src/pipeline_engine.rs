#![allow(clippy::too_many_arguments, clippy::type_complexity)]

use crate::pipeline_parser::{self, PipelineStage};
use crate::weplex_agents;
use serde::Serialize;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write as IoWrite};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

/// Maximum size of the output buffer per stage (10 MB).
const OUTPUT_BUFFER_CAP: usize = 10_000_000;

/// Maximum number of completed/failed/cancelled runs to keep in history.
const MAX_FINISHED_RUNS: usize = 50;

// ── State types ─────────────────────────────────────────────────────────────

/// Execution state of a single pipeline stage.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status")]
pub enum StageState {
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "running")]
    Running,
    #[serde(rename = "completed")]
    Completed { artifact: String, duration_ms: u64 },
    #[serde(rename = "failed")]
    Failed {
        exit_code: i32,
        output: String,
        duration_ms: u64,
    },
    #[serde(rename = "skipped")]
    Skipped,
}

/// Runtime info for a pipeline stage, including its current state.
#[derive(Debug, Clone, Serialize)]
pub struct StageRunInfo {
    pub name: String,
    pub agent: String,
    pub state: StageState,
    pub parallel_group: Option<Vec<StageRunInfo>>,
}

/// Overall status of a pipeline run.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum RunStatus {
    #[allow(dead_code)]
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "running")]
    Running,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "failed")]
    Failed,
    #[serde(rename = "cancelled")]
    Cancelled,
}

/// A single pipeline execution with its stages, status, and timing.
/// A run is always bound to a single profile — all stages share the same
/// profile environment.  This is enforced at creation time.
#[derive(Debug, Clone, Serialize)]
pub struct PipelineRun {
    pub id: String,
    pub pipeline_name: String,
    pub pipeline_file: String,
    pub task: String,
    pub cwd: String,
    /// Profile used for this run (all stages inherit it).
    pub profile_name: String,
    pub status: RunStatus,
    pub stages: Vec<StageRunInfo>,
    pub started_at: Option<u64>,
    pub finished_at: Option<u64>,
}

// ── Event payloads ──────────────────────────────────────────────────────────

#[derive(Clone, Serialize)]
struct StageChangedPayload {
    run_id: String,
    stage_name: String,
    state: StageState,
}

#[derive(Clone, Serialize)]
struct RunChangedPayload {
    run_id: String,
    status: RunStatus,
}

#[derive(Clone, Serialize)]
struct StageOutputPayload {
    run_id: String,
    stage_name: String,
    chunk: String,
}

// ── Mutex helper ────────────────────────────────────────────────────────────

/// Lock a mutex, recovering from poison if another thread panicked while
/// holding it.  The data may be in an inconsistent state but the app
/// won't crash.
fn lock_or_recover<T>(mutex: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

// ── Prepared run (returned by prepare_run, consumed by launch_run) ──────────

/// Data needed to launch a pipeline run on a background thread.
/// Created by `PipelineEngine::prepare_run()`, consumed by `PipelineEngine::launch_run()`.
pub struct PreparedRun {
    pub run_id: String,
    run_arc: Arc<Mutex<PipelineRun>>,
    cancel_flag: Arc<std::sync::atomic::AtomicBool>,
    pipeline_stages: Vec<PipelineStage>,
    agent_map: HashMap<String, weplex_agents::WeplexAgent>,
    task: String,
    cwd: String,
    profile_env: HashMap<String, String>,
}

// ── Engine ───────────────────────────────────────────────────────────────────

/// Orchestrates pipeline runs: parses YAML configs, spawns agent processes,
/// tracks stage states, and emits progress events to the frontend.
pub struct PipelineEngine {
    runs: HashMap<String, Arc<Mutex<PipelineRun>>>,
    /// Flags for cancellation — checked by orchestrator threads
    cancel_flags: HashMap<String, Arc<std::sync::atomic::AtomicBool>>,
    /// MCP artifacts stored by (run_id, stage_name) — set via deck_stage_complete
    mcp_artifacts: HashMap<(String, String), String>,
}

impl PipelineEngine {
    pub fn new() -> Self {
        Self {
            runs: HashMap::new(),
            cancel_flags: HashMap::new(),
            mcp_artifacts: HashMap::new(),
        }
    }

    /// Store an MCP artifact for a stage (called from IPC handler).
    pub fn set_mcp_artifact(&mut self, run_id: &str, stage_name: &str, artifact: &str) {
        self.mcp_artifacts.insert(
            (run_id.to_string(), stage_name.to_string()),
            artifact.to_string(),
        );
    }

    /// Retrieve an MCP artifact for a stage.
    pub fn get_mcp_artifact(&self, run_id: &str, stage_name: &str) -> Option<&String> {
        self.mcp_artifacts
            .get(&(run_id.to_string(), stage_name.to_string()))
    }

    /// Prepare a pipeline run: parse config, create run record, emit initial event.
    /// Returns a `PreparedRun` that the caller can launch with `launch_run()`.
    /// This allows the caller to start the MCP socket BEFORE spawning the
    /// orchestration thread, avoiding a race condition in headless mode.
    pub fn prepare_run(
        &mut self,
        pipeline_file: &str,
        task: &str,
        cwd: &str,
        profile_name: &str,
        profile_env: HashMap<String, String>,
        agent_map: HashMap<String, weplex_agents::WeplexAgent>,
        app: &AppHandle,
    ) -> Result<PreparedRun, String> {
        // Cleanup old finished runs before adding a new one
        self.cleanup_old_runs();

        let content = std::fs::read_to_string(pipeline_file).map_err(|e| e.to_string())?;
        let config = pipeline_parser::parse(&content, pipeline_file)?;

        // agent_map is pre-collected by the caller (all sources merged)

        let run_id = uuid::Uuid::new_v4().to_string();
        let now_ms = epoch_ms();

        let stages = build_stage_info(&config.stages);

        let run = PipelineRun {
            id: run_id.clone(),
            pipeline_name: config.name.clone(),
            pipeline_file: pipeline_file.to_string(),
            task: task.to_string(),
            cwd: resolve_cwd(cwd),
            profile_name: profile_name.to_string(),
            status: RunStatus::Running,
            stages,
            started_at: Some(now_ms),
            finished_at: None,
        };

        let run_arc = Arc::new(Mutex::new(run));
        let cancel_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));

        self.runs.insert(run_id.clone(), Arc::clone(&run_arc));
        self.cancel_flags
            .insert(run_id.clone(), Arc::clone(&cancel_flag));

        let _ = app.emit(
            "pipeline-run-changed",
            RunChangedPayload {
                run_id: run_id.clone(),
                status: RunStatus::Running,
            },
        );

        Ok(PreparedRun {
            run_id,
            run_arc,
            cancel_flag,
            pipeline_stages: config.stages,
            agent_map,
            task: task.to_string(),
            cwd: resolve_cwd(cwd),
            profile_env,
        })
    }

    /// Launch a previously prepared run on a background thread.
    /// `engine_arc` is needed so the orchestrator can read MCP artifacts.
    pub fn launch_run(
        prepared: PreparedRun,
        engine_arc: Arc<Mutex<PipelineEngine>>,
        app: AppHandle,
    ) -> String {
        let run_id = prepared.run_id.clone();

        std::thread::spawn(move || {
            orchestrate(
                prepared.run_id,
                prepared.run_arc,
                prepared.cancel_flag,
                prepared.pipeline_stages,
                prepared.agent_map,
                prepared.task,
                prepared.cwd,
                prepared.profile_env,
                engine_arc,
                app,
            );
        });

        run_id
    }

    pub fn cancel_run(&mut self, run_id: &str) -> Result<(), String> {
        if let Some(flag) = self.cancel_flags.get(run_id) {
            flag.store(true, std::sync::atomic::Ordering::Relaxed);
            if let Some(run) = self.runs.get(run_id) {
                let mut r = lock_or_recover(run);
                r.status = RunStatus::Cancelled;
                r.finished_at = Some(epoch_ms());
            }
            Ok(())
        } else {
            Err(format!("Run '{}' not found", run_id))
        }
    }

    pub fn get_run(&self, run_id: &str) -> Option<PipelineRun> {
        self.runs.get(run_id).map(|r| lock_or_recover(r).clone())
    }

    pub fn list_runs(&self) -> Vec<PipelineRun> {
        self.runs
            .values()
            .map(|r| lock_or_recover(r).clone())
            .collect()
    }

    pub fn get_artifact(&self, run_id: &str, stage_name: &str) -> Option<String> {
        let run = self.runs.get(run_id)?;
        let r = lock_or_recover(run);
        find_artifact(&r.stages, stage_name)
    }

    /// Remove completed/failed/cancelled runs when the total exceeds MAX_FINISHED_RUNS.
    fn cleanup_old_runs(&mut self) {
        let mut finished: Vec<(String, u64)> = self
            .runs
            .iter()
            .filter_map(|(id, run_arc)| {
                let r = lock_or_recover(run_arc);
                match r.status {
                    RunStatus::Completed | RunStatus::Failed | RunStatus::Cancelled => {
                        Some((id.clone(), r.finished_at.unwrap_or(0)))
                    }
                    _ => None,
                }
            })
            .collect();

        if finished.len() <= MAX_FINISHED_RUNS {
            return;
        }

        // Sort by finished_at ascending (oldest first)
        finished.sort_by_key(|(_, ts)| *ts);
        let to_remove = finished.len() - MAX_FINISHED_RUNS;
        for (id, _) in finished.into_iter().take(to_remove) {
            self.runs.remove(&id);
            self.cancel_flags.remove(&id);
        }
    }
}

// ── Orchestrator (runs on dedicated thread) ─────────────────────────────────

fn orchestrate(
    run_id: String,
    run: Arc<Mutex<PipelineRun>>,
    cancel_flag: Arc<std::sync::atomic::AtomicBool>,
    stages: Vec<PipelineStage>,
    agents: HashMap<String, weplex_agents::WeplexAgent>,
    task: String,
    cwd: String,
    profile_env: HashMap<String, String>,
    engine_arc: Arc<Mutex<PipelineEngine>>,
    app: AppHandle,
) {
    let mut artifacts: HashMap<String, String> = HashMap::new();
    let mut failed = false;

    for stage_def in &stages {
        if cancel_flag.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }

        if let Some(ref parallel_subs) = stage_def.parallel {
            // Parallel execution
            let results = execute_parallel(
                &run_id,
                parallel_subs,
                &agents,
                &task,
                &cwd,
                &artifacts,
                &cancel_flag,
                &run,
                &profile_env,
                &app,
            );

            for (name, result) in results {
                match result {
                    Ok(artifact) => {
                        // Prefer MCP artifact over stdout capture
                        let final_artifact = lock_or_recover(&engine_arc)
                            .get_mcp_artifact(&run_id, &name)
                            .cloned()
                            .unwrap_or(artifact);
                        artifacts.insert(name, final_artifact);
                    }
                    Err((is_optional, _)) => {
                        if !is_optional {
                            failed = true;
                        }
                    }
                }
            }

            if failed {
                break;
            }
        } else {
            // Sequential execution
            let stage_name = stage_def.name.clone().unwrap_or_default();
            let agent_name = stage_def.agent.clone().unwrap_or_default();
            let role = stage_def.role.clone().unwrap_or_default();
            let optional = stage_def.optional == Some(true);

            // Gather artifacts from `receives`
            let context = gather_artifacts(&stage_def.receives, &artifacts);

            update_stage_state(&run, &stage_name, StageState::Running);
            emit_stage_changed(&app, &run_id, &stage_name, StageState::Running);

            // Load skill contents for this stage
            let skill_contents = load_skill_contents(&stage_def.skills);

            match execute_stage(
                &run_id,
                &stage_name,
                &agent_name,
                &role,
                &task,
                &context,
                &cwd,
                &agents,
                &skill_contents,
                &profile_env,
                &cancel_flag,
                &app,
            ) {
                Ok((artifact, duration_ms)) => {
                    // Prefer MCP artifact over stdout capture
                    let final_artifact = lock_or_recover(&engine_arc)
                        .get_mcp_artifact(&run_id, &stage_name)
                        .cloned()
                        .unwrap_or(artifact);
                    let state = StageState::Completed {
                        artifact: final_artifact.clone(),
                        duration_ms,
                    };
                    update_stage_state(&run, &stage_name, state.clone());
                    emit_stage_changed(&app, &run_id, &stage_name, state);
                    artifacts.insert(stage_name, final_artifact);
                }
                Err((exit_code, output, duration_ms)) => {
                    if optional {
                        update_stage_state(&run, &stage_name, StageState::Skipped);
                        emit_stage_changed(&app, &run_id, &stage_name, StageState::Skipped);
                    } else {
                        let state = StageState::Failed {
                            exit_code,
                            output,
                            duration_ms,
                        };
                        update_stage_state(&run, &stage_name, state.clone());
                        emit_stage_changed(&app, &run_id, &stage_name, state);
                        failed = true;
                        break;
                    }
                }
            }
        }
    }

    let final_status = if cancel_flag.load(std::sync::atomic::Ordering::Relaxed) {
        RunStatus::Cancelled
    } else if failed {
        RunStatus::Failed
    } else {
        RunStatus::Completed
    };

    {
        let mut r = lock_or_recover(&run);
        r.status = final_status.clone();
        r.finished_at = Some(epoch_ms());
    }

    let _ = app.emit(
        "pipeline-run-changed",
        RunChangedPayload {
            run_id,
            status: final_status,
        },
    );
}

// ── Parallel execution ──────────────────────────────────────────────────────

fn execute_parallel(
    run_id: &str,
    subs: &[PipelineStage],
    agents: &HashMap<String, weplex_agents::WeplexAgent>,
    task: &str,
    cwd: &str,
    artifacts: &HashMap<String, String>,
    cancel_flag: &Arc<std::sync::atomic::AtomicBool>,
    run: &Arc<Mutex<PipelineRun>>,
    profile_env: &HashMap<String, String>,
    app: &AppHandle,
) -> Vec<(String, Result<String, (bool, String)>)> {
    let results = Arc::new(Mutex::new(Vec::new()));

    std::thread::scope(|scope| {
        for sub in subs {
            let stage_name = sub.name.clone().unwrap_or_default();
            let agent_name = sub.agent.clone().unwrap_or_default();
            let role = sub.role.clone().unwrap_or_default();
            let optional = sub.optional == Some(true);
            let context = gather_artifacts(&sub.receives, artifacts);
            let skill_contents = load_skill_contents(&sub.skills);
            let results = Arc::clone(&results);

            update_stage_state(run, &stage_name, StageState::Running);
            emit_stage_changed(app, run_id, &stage_name, StageState::Running);

            scope.spawn(move || {
                let result = execute_stage(
                    run_id,
                    &stage_name,
                    &agent_name,
                    &role,
                    task,
                    &context,
                    cwd,
                    agents,
                    &skill_contents,
                    profile_env,
                    cancel_flag,
                    app,
                );

                match result {
                    Ok((artifact, duration_ms)) => {
                        let state = StageState::Completed {
                            artifact: artifact.clone(),
                            duration_ms,
                        };
                        update_stage_state(run, &stage_name, state.clone());
                        emit_stage_changed(app, run_id, &stage_name, state);
                        lock_or_recover(&results).push((stage_name, Ok(artifact)));
                    }
                    Err((exit_code, output, duration_ms)) => {
                        if optional {
                            update_stage_state(run, &stage_name, StageState::Skipped);
                            emit_stage_changed(app, run_id, &stage_name, StageState::Skipped);
                            lock_or_recover(&results).push((stage_name, Err((true, output))));
                        } else {
                            let state = StageState::Failed {
                                exit_code,
                                output: output.clone(),
                                duration_ms,
                            };
                            update_stage_state(run, &stage_name, state.clone());
                            emit_stage_changed(app, run_id, &stage_name, state);
                            lock_or_recover(&results).push((stage_name, Err((false, output))));
                        }
                    }
                }
            });
        }
    });

    match Arc::try_unwrap(results) {
        Ok(mutex) => mutex.into_inner().unwrap_or_else(|e| e.into_inner()),
        Err(arc) => lock_or_recover(&arc).clone(),
    }
}

/// Load skill markdown contents by name from ~/.weplex/skills/ and ~/.claude/skills/.
fn load_skill_contents(skill_names: &[String]) -> Vec<String> {
    if skill_names.is_empty() {
        return Vec::new();
    }

    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    let mut contents = Vec::new();

    for name in skill_names {
        // Validate name
        if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
            continue;
        }

        // Check Weplex skills first, then Claude skills
        let paths = [
            format!("{}/.weplex/skills/{}/SKILL.md", home, name),
            format!("{}/.claude/skills/{}/SKILL.md", home, name),
        ];

        for path in &paths {
            if let Ok(content) = std::fs::read_to_string(path) {
                contents.push(content);
                break;
            }
        }
    }

    contents
}

// ── Single stage execution ──────────────────────────────────────────────────

fn execute_stage(
    run_id: &str,
    stage_name: &str,
    agent_name: &str,
    role: &str,
    task: &str,
    artifacts_context: &str,
    cwd: &str,
    agents: &HashMap<String, weplex_agents::WeplexAgent>,
    stage_skills: &[String],
    profile_env: &HashMap<String, String>,
    cancel_flag: &Arc<std::sync::atomic::AtomicBool>,
    app: &AppHandle,
) -> Result<(String, u64), (i32, String, u64)> {
    let agent = agents.get(agent_name).ok_or_else(|| {
        (
            -1,
            format!(
                "Agent '{}' not found. Searched ~/.weplex/agents/ and .claude/agents/",
                agent_name
            ),
            0u64,
        )
    })?;

    let (binary, args, prompt) =
        weplex_agents::resolve_command(agent, role, task, artifacts_context, stage_skills);

    // Reject untrusted binaries
    if !weplex_agents::is_trusted_binary(&binary) {
        return Err((
            -1,
            format!(
                "Untrusted binary '{}'. Only claude, codex, aider, gemini are allowed.",
                binary
            ),
            0u64,
        ));
    }

    // Resolve binary full path — Tauri apps don't inherit shell PATH
    let resolved_binary = resolve_binary_path(&binary);

    // Filter denied env vars from agent config
    let safe_env: HashMap<String, String> = agent
        .env
        .iter()
        .filter(|(k, _)| !weplex_agents::DENIED_ENV_VARS.contains(&k.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    let start = Instant::now();

    // Spawn process with piped stdin/stdout/stderr
    // Note — agent output may contain secrets (API keys, tokens) from the
    // spawned process. For MVP we stream raw output; a future version should add
    // a configurable redaction filter before emitting to the frontend.
    // Merge profile env (auth, config) with agent-specific safe env
    let mut merged_env = profile_env.clone();
    merged_env.extend(safe_env);

    // Inject MCP env vars so the weplex-mcp server can connect to the right socket
    merged_env.insert("WEPLEX_RUN_ID".to_string(), run_id.to_string());
    merged_env.insert("WEPLEX_STAGE_NAME".to_string(), stage_name.to_string());
    merged_env.insert(
        "WEPLEX_MCP_SOCKET".to_string(),
        crate::ipc_server::IpcSocketPool::socket_path_for_run(run_id)
            .to_string_lossy()
            .to_string(),
    );

    let mut child = Command::new(&resolved_binary)
        .args(&args)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .envs(&merged_env)
        .spawn()
        .map_err(|e| (-1, format!("Failed to spawn '{}': {}", binary, e), 0u64))?;

    // Write prompt to stdin (for claude -p which reads from stdin)
    // For aider, prompt is already in args, so this is a no-op write
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(prompt.as_bytes());
        // stdin dropped here → closes pipe → signals EOF to child
    }

    // Spawn a dedicated thread to read stderr concurrently with stdout
    let stderr_handle = child.stderr.take().map(|stderr| {
        std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            let mut stderr_buf = String::new();
            for line in reader.lines().map_while(Result::ok) {
                stderr_buf.push_str(&line);
                stderr_buf.push('\n');
            }
            stderr_buf
        })
    });

    // Read stdout in a streaming fashion
    let stdout = child.stdout.take();
    let mut output_buf = String::new();
    let mut output_capped = false;

    if let Some(stdout) = stdout {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            if cancel_flag.load(std::sync::atomic::Ordering::Relaxed) {
                let _ = child.kill();
                // Wait up to 10s for process to exit after kill
                let kill_deadline = Instant::now() + Duration::from_secs(10);
                loop {
                    match child.try_wait() {
                        Ok(Some(_)) => break,
                        Ok(None) if Instant::now() < kill_deadline => {
                            std::thread::sleep(Duration::from_millis(100));
                        }
                        _ => {
                            eprintln!(
                                "[weplex] process for stage '{}' did not exit within 10s after kill",
                                stage_name
                            );
                            break;
                        }
                    }
                }
                let duration_ms = start.elapsed().as_millis() as u64;
                return Err((-2, "Cancelled".to_string(), duration_ms));
            }

            match line {
                Ok(line) => {
                    // Cap output buffer at OUTPUT_BUFFER_CAP bytes
                    if !output_capped {
                        if output_buf.len() + line.len() + 1 > OUTPUT_BUFFER_CAP {
                            output_buf.push_str("\n...(output truncated at 10MB)...");
                            output_capped = true;
                        } else {
                            output_buf.push_str(&line);
                            output_buf.push('\n');
                        }
                    }

                    // Stream chunk to frontend regardless of cap
                    let _ = app.emit(
                        "pipeline-stage-output",
                        StageOutputPayload {
                            run_id: run_id.to_string(),
                            stage_name: stage_name.to_string(),
                            chunk: format!("{}\n", line),
                        },
                    );
                }
                Err(_) => break,
            }
        }
    }

    // Join stderr thread and append its output
    if let Some(handle) = stderr_handle
        && let Ok(stderr_output) = handle.join()
        && !stderr_output.is_empty()
        && !output_capped
    {
        let remaining = OUTPUT_BUFFER_CAP.saturating_sub(output_buf.len());
        if stderr_output.len() <= remaining {
            output_buf.push_str(&stderr_output);
        } else if remaining > 0 {
            // Truncate stderr to fit within cap
            output_buf.push_str(&stderr_output[..remaining]);
            output_buf.push_str("\n...(output truncated at 10MB)...");
        }
    }

    let exit_status = child.wait().map_err(|e| {
        let duration_ms = start.elapsed().as_millis() as u64;
        (
            -1,
            format!("Failed to wait for process: {}", e),
            duration_ms,
        )
    })?;

    let duration_ms = start.elapsed().as_millis() as u64;
    let exit_code = exit_status.code().unwrap_or(-1);

    if exit_code == 0 {
        Ok((output_buf, duration_ms))
    } else {
        Err((exit_code, output_buf, duration_ms))
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn build_stage_info(stages: &[PipelineStage]) -> Vec<StageRunInfo> {
    stages
        .iter()
        .map(|s| {
            if let Some(ref parallel) = s.parallel {
                StageRunInfo {
                    name: s.name.clone().unwrap_or_else(|| "parallel".to_string()),
                    agent: String::new(),
                    state: StageState::Pending,
                    parallel_group: Some(
                        parallel
                            .iter()
                            .map(|ps| StageRunInfo {
                                name: ps.name.clone().unwrap_or_default(),
                                agent: ps.agent.clone().unwrap_or_default(),
                                state: StageState::Pending,
                                parallel_group: None,
                            })
                            .collect(),
                    ),
                }
            } else {
                StageRunInfo {
                    name: s.name.clone().unwrap_or_default(),
                    agent: s.agent.clone().unwrap_or_default(),
                    state: StageState::Pending,
                    parallel_group: None,
                }
            }
        })
        .collect()
}

fn gather_artifacts(receives: &[String], artifacts: &HashMap<String, String>) -> String {
    if receives.is_empty() {
        return String::new();
    }

    let mut context = String::new();
    for name in receives {
        if let Some(artifact) = artifacts.get(name) {
            context.push_str(&format!("=== Output from: {} ===\n", name));
            // Truncate very large artifacts to last 8000 chars (L4: UTF-8 safe)
            if artifact.len() > 8000 {
                context.push_str("...(truncated)...\n");
                // Find a valid char boundary at or before (len - 8000)
                let start = artifact.len() - 8000;
                let safe_start = artifact.floor_char_boundary(start);
                context.push_str(&artifact[safe_start..]);
            } else {
                context.push_str(artifact);
            }
            context.push_str("\n\n");
        }
    }
    context
}

fn update_stage_state(run: &Arc<Mutex<PipelineRun>>, stage_name: &str, state: StageState) {
    let mut r = lock_or_recover(run);
    for stage in &mut r.stages {
        if stage.name == stage_name {
            stage.state = state.clone();
            return;
        }
        if let Some(ref mut parallel) = stage.parallel_group {
            for ps in parallel {
                if ps.name == stage_name {
                    ps.state = state;
                    return;
                }
            }
        }
    }
}

fn emit_stage_changed(app: &AppHandle, run_id: &str, stage_name: &str, state: StageState) {
    let _ = app.emit(
        "pipeline-stage-changed",
        StageChangedPayload {
            run_id: run_id.to_string(),
            stage_name: stage_name.to_string(),
            state,
        },
    );
}

fn find_artifact(stages: &[StageRunInfo], stage_name: &str) -> Option<String> {
    for stage in stages {
        if stage.name == stage_name
            && let StageState::Completed { ref artifact, .. } = stage.state
        {
            return Some(artifact.clone());
        }
        if let Some(ref parallel) = stage.parallel_group
            && let Some(a) = find_artifact(parallel, stage_name)
        {
            return Some(a);
        }
    }
    None
}

/// Resolve a binary name to its full path.
/// Tauri apps on macOS don't inherit shell PATH, so "claude" won't be found.
/// We check common install locations and also try the user's login shell.
fn resolve_binary_path(binary: &str) -> String {
    // If already a full path, use as-is
    if binary.starts_with('/') {
        return binary.to_string();
    }

    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());

    // Common locations for CLI tools
    let candidates = [
        format!("{}/.local/bin/{}", home, binary),
        format!("/usr/local/bin/{}", binary),
        format!("/opt/homebrew/bin/{}", binary),
        format!("{}/.cargo/bin/{}", home, binary),
        format!("/usr/bin/{}", binary),
    ];

    for path in &candidates {
        if std::path::Path::new(path).exists() {
            return path.clone();
        }
    }

    // Fallback: ask the user's shell to resolve it
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    if let Ok(output) = Command::new(&shell)
        .args(["-lc", &format!("command -v {}", binary)])
        .output()
        && output.status.success()
    {
        let resolved = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !resolved.is_empty() {
            return resolved;
        }
    }

    // Last resort: return as-is and let Command::new() try
    binary.to_string()
}

fn epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn resolve_cwd(cwd: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    if cwd == "~" {
        home
    } else if let Some(rest) = cwd.strip_prefix("~/") {
        format!("{}/{}", home, rest)
    } else {
        cwd.to_string()
    }
}

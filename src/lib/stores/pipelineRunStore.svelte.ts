import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { PipelineRunInfo, StageRunInfo, StageStatus, PipelineRunStatus, StageDefinitionPayload } from '../types';
import type { PipelineConfig, PipelineStage } from '../components/overlays/types';
import { sessionStore } from './sessionStore.svelte';
import { collabPipelineStore } from './collabPipelineStore.svelte';

// ── MCP event payload ─────────────────────────────────────────────────────

interface McpStageCompletePayload {
  run_id: string;
  stage_name: string;
  artifact: string;
  status: string; // "success" or "failure"
  error: string;
}

// ── Interactive Pipeline Run ────────────────────────────────────────────────

interface InteractiveRun {
  id: string;
  pipelineName: string;
  pipelineFile: string;
  task: string;
  cwd: string;
  /** Profile bound to this run — all stages use the same profile. */
  profileName: string;
  envVars: Record<string, string>;
  status: PipelineRunStatus;
  stages: InteractiveStage[];
  currentStageIndex: number;
  startedAt: number;
  finishedAt: number | null;
  /** MCP socket path for this run (set after start_mcp_for_run) */
  mcpSocketPath: string | null;
}

interface InteractiveStage {
  name: string;
  agent: string;
  role: string;
  status: StageStatus;
  sessionId: number | null;
  optional: boolean;
  parallel: InteractiveStage[] | null;
  /** Track if prompt was injected into Claude */
  promptInjected: boolean;
  /** Track if Claude was ever 'active' — only then does 'idle' mean "finished" */
  wasActive: boolean;
  /** Artifact from MCP completion (takes priority over PTY output) */
  mcpArtifact: string | null;
  /** Whether stage was completed via MCP (skip idle detection) */
  completedViaMcp: boolean;
}

let runs = $state<InteractiveRun[]>([]);
let activeRunId = $state<string | null>(null);
let watchInterval: ReturnType<typeof setInterval> | null = null;
let mcpUnlisten: UnlistenFn | null = null;

// ── MCP event handler ─────────────────────────────────────────────────────

function handleMcpStageComplete(payload: McpStageCompletePayload) {
  const run = runs.find((r) => r.id === payload.run_id);
  if (!run || run.status !== 'running') {
    console.warn(`[Weplex] MCP event for unknown/inactive run: ${payload.run_id}`);
    return;
  }

  const currentStage = run.stages[run.currentStageIndex];
  if (!currentStage || currentStage.name !== payload.stage_name) {
    console.warn(
      `[Weplex] MCP event for non-current stage "${payload.stage_name}" (current: "${currentStage?.name}")`,
    );
    return;
  }

  if (currentStage.status !== 'running') return;

  if (payload.status === 'success') {
    currentStage.completedViaMcp = true;
    currentStage.mcpArtifact = payload.artifact;
    currentStage.status = 'completed';
    console.log(`[Weplex] Stage "${payload.stage_name}" completed via MCP`);
    advanceRun(run);
  } else {
    currentStage.completedViaMcp = true;
    currentStage.status = 'failed';
    run.status = 'failed';
    run.finishedAt = Date.now();
    console.error(`[Weplex] Stage "${payload.stage_name}" failed via MCP: ${payload.error}`);
    // Stop MCP socket on failure
    stopMcpForRun(run.id);
  }

  runs = [...runs]; // trigger reactivity
}

// ── MCP lifecycle ─────────────────────────────────────────────────────────

async function startMcpForRun(runId: string): Promise<string | null> {
  try {
    const socketPath = await invoke<string>('start_mcp_for_run', { runId });
    console.log(`[Weplex] MCP socket started for run ${runId}: ${socketPath}`);
    return socketPath;
  } catch (e) {
    console.warn(`[Weplex] Failed to start MCP for run ${runId}:`, e);
    return null;
  }
}

async function stopMcpForRun(runId: string): Promise<void> {
  try {
    await invoke('stop_mcp_for_run', { runId });
    console.log(`[Weplex] MCP socket stopped for run ${runId}`);
  } catch (e) {
    console.warn(`[Weplex] Failed to stop MCP for run ${runId}:`, e);
  }
}

async function setupMcpListener(): Promise<void> {
  if (mcpUnlisten) return; // already listening
  mcpUnlisten = await listen<McpStageCompletePayload>('mcp-stage-complete', (event) => {
    handleMcpStageComplete(event.payload);
  });
}

function teardownMcpListener(): void {
  mcpUnlisten?.();
  mcpUnlisten = null;
}

function flattenPipelineStages(stages: PipelineStage[]): InteractiveStage[] {
  const result: InteractiveStage[] = [];
  for (const s of stages) {
    if (s.parallel) {
      // For interactive mode, run parallel stages sequentially (user needs to interact with each)
      for (const ps of s.parallel) {
        result.push({
          name: ps.name || ps.agent || 'stage',
          agent: ps.agent || '',
          role: ps.role || '',
          status: 'pending',
          sessionId: null,
          optional: ps.optional === true,
          parallel: null,
          promptInjected: false,
          wasActive: false,
          mcpArtifact: null,
          completedViaMcp: false,
        });
      }
    } else {
      result.push({
        name: s.name || s.agent || 'stage',
        agent: s.agent || '',
        role: s.role || '',
        status: 'pending',
        sessionId: null,
        optional: s.optional === true,
        parallel: null,
        promptInjected: false,
        wasActive: false,
        mcpArtifact: null,
        completedViaMcp: false,
      });
    }
  }
  return result;
}

function buildStagePrompt(
  stage: InteractiveStage,
  task: string,
  stageIndex: number,
  totalStages: number,
): string {
  let prompt = '';

  prompt += `## Pipeline Stage ${stageIndex + 1}/${totalStages}: ${stage.agent}\n\n`;

  if (stage.role) {
    prompt += `**Your role:** ${stage.role}\n\n`;
  }

  prompt += `## Task\n\n${task}`;

  return prompt;
}

function startWatching() {
  if (watchInterval) return;
  watchInterval = setInterval(checkRunProgress, 2000);
}

function stopWatching() {
  if (watchInterval) {
    clearInterval(watchInterval);
    watchInterval = null;
  }
}

function checkRunProgress() {
  for (const run of runs) {
    if (run.status !== 'running') continue;

    const currentStage = run.stages[run.currentStageIndex];
    if (!currentStage || currentStage.status !== 'running') continue;

    // Skip if already completed via MCP (MCP has priority over idle detection)
    if (currentStage.completedViaMcp) continue;

    if (currentStage.sessionId !== null) {
      const session = sessionStore.sessions.find((s) => s.id === currentStage.sessionId);
      if (!session) continue;

      // Step 1: Wait for Claude to load and become idle, then inject prompt
      if (session.status === 'idle' && !currentStage.promptInjected) {
        currentStage.promptInjected = true;
        const prompt = buildStagePrompt(
          currentStage,
          run.task,
          run.currentStageIndex,
          run.stages.length,
        );
        // Write prompt to PTY, then mark session as active to trigger JSONL polling
        // (write_pty doesn't go through xterm onData, so TerminalView won't auto-detect input)
        invoke('write_pty', { sessionId: currentStage.sessionId, data: prompt + '\r' })
          .then(() => {
            sessionStore.updateStatus(currentStage.sessionId!, 'active');
          })
          .catch((e) =>
            console.error(`[Weplex] Failed to inject prompt for stage ${currentStage.name}:`, e),
          );
        runs = [...runs];
        continue;
      }

      // Step 2: Track if Claude started processing
      if (session.status === 'active' && currentStage.promptInjected && !currentStage.wasActive) {
        currentStage.wasActive = true;
        runs = [...runs];
      }

      // Step 3: Complete when Claude goes idle AFTER having been active
      if (session.status === 'idle' && currentStage.wasActive) {
        currentStage.status = 'completed';
        advanceRun(run);
        runs = [...runs];
      }
    }
  }

  // Stop watching if no running pipelines
  if (!runs.some((r) => r.status === 'running')) {
    stopWatching();
  }
}

function advanceRun(run: InteractiveRun) {
  run.currentStageIndex++;

  if (run.currentStageIndex >= run.stages.length) {
    // All stages done
    run.status = 'completed';
    run.finishedAt = Date.now();
    // Stop MCP socket when pipeline completes
    stopMcpForRun(run.id);
    return;
  }

  // Start next stage
  startStage(run, run.currentStageIndex);
}

function startStage(run: InteractiveRun, index: number) {
  const stage = run.stages[index];
  if (!stage) return;

  stage.status = 'running';
  stage.promptInjected = false;
  stage.wasActive = false;
  stage.completedViaMcp = false;
  stage.mcpArtifact = null;

  // Build extra env vars for MCP integration
  const extraEnvVars: Record<string, string> = {
    ...run.envVars,
    WEPLEX_RUN_ID: run.id,
    WEPLEX_STAGE_NAME: stage.name,
  };
  if (run.mcpSocketPath) {
    extraEnvVars['WEPLEX_MCP_SOCKET'] = run.mcpSocketPath;
  }

  // Determine parentId: first stage is the parent, subsequent stages are children
  const firstStageSession = run.stages[0]?.sessionId;
  const parentId = index > 0 && firstStageSession ? firstStageSession : undefined;

  // Create a real PTY session — prompt injection handled by checkRunProgress
  const session = sessionStore.create({
    command: 'claude',
    cwd: run.cwd,
    name: `${run.pipelineName}: ${stage.agent}`,
    spaceId: sessionStore.activeSession?.spaceId || 'default',
    extraEnvVars,
    parentId,
  });

  stage.sessionId = session.id;

  // Activate this session so user sees it
  sessionStore.activate(session.id);

  runs = [...runs]; // trigger reactivity
}

// ── Collaborative check ────────────────────────────────────────────────────

/** Check if any stage has an owner field — means it's a collaborative pipeline. */
function hasCollaborativeStages(stages: PipelineStage[]): boolean {
  for (const s of stages) {
    if (s.owner) return true;
    if (s.parallel) {
      for (const ps of s.parallel) {
        if (ps.owner) return true;
      }
    }
  }
  return false;
}

/** Convert PipelineStage[] to StageDefinitionPayload[] for the collab API. */
function toCollabStages(
  stages: PipelineStage[],
): StageDefinitionPayload[] {
  const result: StageDefinitionPayload[] = [];
  for (const s of stages) {
    if (s.parallel) {
      for (const ps of s.parallel) {
        result.push({
          name: ps.name || ps.agent || 'stage',
          agent: ps.agent || 'unknown',
          role: ps.role || 'unknown',
          receives: ps.receives || [],
          optional: ps.optional || undefined,
          ownerEmail: ps.owner || undefined,
        });
      }
    } else {
      result.push({
        name: s.name || s.agent || 'stage',
        agent: s.agent || 'unknown',
        role: s.role || 'unknown',
        receives: s.receives || [],
        optional: s.optional || undefined,
        ownerEmail: s.owner || undefined,
      });
    }
  }
  return result;
}

// ── Local stage execution for collaborative runs ───────────────────────────

/** Max bytes to capture from PTY output for artifact */
const MAX_ARTIFACT_BYTES = 512 * 1024;

/**
 * Execute a single pipeline stage locally as a PTY session.
 * Creates a session, injects the prompt with artifacts, waits for idle, and
 * returns captured PTY output as the artifact string.
 */
async function executeStageLocally(
  stage: { name: string; agent: string; role: string; receives: string[] },
  task: string,
  cwd: string,
  artifacts: Record<string, string>,
  mcpContext?: { runId: string; socketPath: string },
): Promise<string> {
  // Build prompt with artifact context
  let prompt = `## Collaborative Pipeline Stage: ${stage.name}\n\n`;
  if (stage.role) {
    prompt += `**Your role:** ${stage.role}\n\n`;
  }
  prompt += `## Task\n\n${task}\n`;

  // Inject artifacts from previous stages
  if (stage.receives.length > 0) {
    prompt += '\n## Artifacts from previous stages\n\n';
    for (const dep of stage.receives) {
      const art = artifacts[dep];
      if (art) {
        prompt += `### ${dep}\n\n${art}\n\n`;
      }
    }
  }

  // Build extra env vars for MCP integration
  const extraEnvVars: Record<string, string> = {};
  if (mcpContext) {
    extraEnvVars['WEPLEX_RUN_ID'] = mcpContext.runId;
    extraEnvVars['WEPLEX_STAGE_NAME'] = stage.name;
    extraEnvVars['WEPLEX_MCP_SOCKET'] = mcpContext.socketPath;
  }

  // Create PTY session for this stage
  const session = sessionStore.create({
    command: stage.agent || 'claude',
    cwd,
    name: `Collab: ${stage.name}`,
    spaceId: sessionStore.activeSession?.spaceId || 'default',
    extraEnvVars: Object.keys(extraEnvVars).length > 0 ? extraEnvVars : undefined,
  });

  sessionStore.activate(session.id);

  // Start capturing PTY output via Tauri events
  let outputBuffer = '';
  let unlistenOutput: UnlistenFn | null = null;

  const outputReady = listen<string>(`pty-output-${session.id}`, (event) => {
    // PTY output is base64-encoded; decode to text
    try {
      const decoded = atob(event.payload);
      outputBuffer += decoded;
      // Trim to last MAX_ARTIFACT_BYTES to avoid unbounded growth
      if (outputBuffer.length > MAX_ARTIFACT_BYTES) {
        outputBuffer = outputBuffer.slice(-MAX_ARTIFACT_BYTES);
      }
    } catch {
      // If base64 decode fails, append raw payload
      outputBuffer += event.payload;
    }
  });

  // Await the listener setup
  unlistenOutput = await outputReady;

  // Listen for MCP stage completion (takes priority over idle detection)
  let mcpUnlistenLocal: UnlistenFn | null = null;
  let mcpResult: { artifact: string; status: string; error: string } | null = null;

  if (mcpContext) {
    mcpUnlistenLocal = await listen<McpStageCompletePayload>('mcp-stage-complete', (event) => {
      const p = event.payload;
      if (p.run_id === mcpContext.runId && p.stage_name === stage.name && !mcpResult) {
        mcpResult = { artifact: p.artifact, status: p.status, error: p.error };
      }
    });
  }

  // Wait for the agent to become idle (ready for input), then inject prompt
  return new Promise<string>((resolve, reject) => {
    let promptInjected = false;
    let wasActive = false;

    function cleanup() {
      clearInterval(checkInterval);
      unlistenOutput?.();
      mcpUnlistenLocal?.();
    }

    const checkInterval = setInterval(() => {
      // MCP completion takes priority over idle detection
      if (mcpResult) {
        cleanup();
        if (mcpResult.status === 'success') {
          resolve(mcpResult.artifact || `Stage "${stage.name}" completed via MCP`);
        } else {
          reject(new Error(`Stage "${stage.name}" failed via MCP: ${mcpResult.error}`));
        }
        return;
      }

      const s = sessionStore.sessions.find((sess) => sess.id === session.id);
      if (!s) {
        cleanup();
        reject(new Error('Session disappeared'));
        return;
      }

      // Wait for idle → inject prompt
      if (s.status === 'idle' && !promptInjected) {
        promptInjected = true;
        // Clear buffer before injecting — we only want output from the stage execution
        outputBuffer = '';
        invoke('write_pty', { sessionId: session.id, data: prompt + '\r' })
          .then(() => sessionStore.updateStatus(session.id, 'active'))
          .catch((e) => {
            cleanup();
            reject(new Error(`Failed to inject prompt: ${e}`));
          });
        return;
      }

      // Track activation
      if (s.status === 'active' && promptInjected && !wasActive) {
        wasActive = true;
      }

      // Completed: idle after being active — return captured output
      if (s.status === 'idle' && wasActive) {
        cleanup();
        const artifact = outputBuffer.trim() || `Stage "${stage.name}" completed (no output captured)`;
        resolve(artifact);
      }

      // Handle error
      if (s.status === 'error') {
        cleanup();
        reject(new Error(`Stage "${stage.name}" errored`));
      }
    }, 2000);

    // Timeout after 30 minutes
    setTimeout(() => {
      cleanup();
      reject(new Error(`Stage "${stage.name}" timed out`));
    }, 30 * 60 * 1000);
  });
}

// ── Exported store ──────────────────────────────────────────────────────────

export const pipelineRunStore = {
  get runs() {
    return runs;
  },
  get activeRunId() {
    return activeRunId;
  },

  get activeRun(): InteractiveRun | undefined {
    return runs.find((r) => r.id === activeRunId);
  },

  init() {
    // Set up MCP event listener for stage completion signals from Rust IPC
    setupMcpListener().catch((e) =>
      console.warn('[Weplex] Failed to set up MCP listener:', e),
    );
  },

  /** Clean up MCP listener. Call on app shutdown if needed. */
  destroy() {
    teardownMcpListener();
  },

  /** Execute a single stage locally for collaborative pipeline delegation. */
  executeStageLocally: executeStageLocally,

  async startRun(
    pipelineFile: string,
    task: string,
    cwd: string,
    profileName?: string,
    envVars?: Record<string, string>,
  ): Promise<string> {
    const profile = profileName || 'Default';

    // Load pipeline config
    const pipelines = await invoke<PipelineConfig[]>('list_pipelines');
    const config = pipelines.find((p) => p.file_path === pipelineFile);
    if (!config) throw new Error('Pipeline not found');

    // If any stage has an owner → delegate to collaborative pipeline store
    if (hasCollaborativeStages(config.stages)) {
      const collabStages = toCollabStages(config.stages);
      return collabPipelineStore.startCollabRun(config.name, task, collabStages);
    }

    const runId = crypto.randomUUID();
    const stages = flattenPipelineStages(config.stages);

    // Start MCP socket for this run before creating the run object
    const mcpSocketPath = await startMcpForRun(runId);

    const run: InteractiveRun = {
      id: runId,
      pipelineName: config.name,
      pipelineFile: pipelineFile,
      task,
      cwd: cwd.replace(/\/+$/, '') || '~',
      profileName: profile,
      envVars: envVars || {},
      status: 'running',
      stages,
      currentStageIndex: 0,
      startedAt: Date.now(),
      finishedAt: null,
      mcpSocketPath,
    };

    runs = [...runs, run];
    activeRunId = runId;

    // Start first stage
    startStage(run, 0);

    // Auto-create orchestration dashboard for multi-stage pipelines
    if (run.stages.length > 1 && run.stages[0].sessionId) {
      sessionStore.createDashboard(run.stages[0].sessionId);
    }

    // Start watching for completion
    startWatching();

    return runId;
  },

  cancelRun(runId: string) {
    const run = runs.find((r) => r.id === runId);
    if (!run) return;
    run.status = 'cancelled';
    run.finishedAt = Date.now();
    // Kill the current stage's session
    const currentStage = run.stages[run.currentStageIndex];
    if (currentStage?.sessionId) {
      sessionStore.kill(currentStage.sessionId);
    }
    // Stop MCP socket on cancel
    stopMcpForRun(runId);
    runs = [...runs];
  },

  /** Manually advance to next stage (user clicks "Next Stage") */
  advanceCurrentStage(runId: string) {
    const run = runs.find((r) => r.id === runId);
    if (!run || run.status !== 'running') return;
    const currentStage = run.stages[run.currentStageIndex];
    if (currentStage) {
      currentStage.status = 'completed';
    }
    advanceRun(run);
    runs = [...runs];
    startWatching();
  },

  /** Skip current stage (for optional stages) */
  skipCurrentStage(runId: string) {
    const run = runs.find((r) => r.id === runId);
    if (!run || run.status !== 'running') return;
    const currentStage = run.stages[run.currentStageIndex];
    if (currentStage) {
      currentStage.status = 'skipped';
      if (currentStage.sessionId) {
        sessionStore.kill(currentStage.sessionId);
      }
    }
    advanceRun(run);
    runs = [...runs];
  },

  setActive(runId: string | null) {
    activeRunId = runId;
  },

  // Compat with old headless store interface
  getStageOutput(): string {
    return '';
  },
  async getArtifact(): Promise<string | null> {
    return null;
  },

  getStageStatus(runId: string, stageName: string): StageStatus {
    const run = runs.find((r) => r.id === runId);
    if (!run) return 'pending';
    const stage = run.stages.find((s) => s.name === stageName);
    return stage?.status || 'pending';
  },

  get runningCount(): number {
    return runs.filter((r) => r.status === 'running').length;
  },
};

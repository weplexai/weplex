import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type {
  CollaborativeRun,
  StageDefinitionPayload,
  PipelineNotification,
} from '../types';
import { pipelineCollabService } from '../services/pipelineCollabService';
import { pipelineWsService } from '../services/pipelineWsService';
import { getAccessToken } from '../services/apiClient';
import { showNativeNotification } from '../utils/notifications';
import { authStore } from './authStore.svelte';
import { teamStore } from './teamStore.svelte';

// ── MCP event payload (same as pipelineRunStore) ──────────────────────────

interface McpStageCompletePayload {
  run_id: string;
  stage_name: string;
  artifact: string;
  status: string;
  error: string;
}

// ── State ──────────────────────────────────────────────────────────────────

let runs = $state<CollaborativeRun[]>([]);
let activeRunId = $state<string | null>(null);
let loading = $state(false);
let error = $state<string | null>(null);

// WebSocket unsubscribe handles
let unsubRunUpdated: (() => void) | null = null;
let unsubStageReady: (() => void) | null = null;
let unsubNotification: (() => void) | null = null;
let mcpUnlisten: UnlistenFn | null = null;

// ── Helpers ────────────────────────────────────────────────────────────────

function updateRunInList(updatedRun: CollaborativeRun): void {
  const idx = runs.findIndex((r) => r.id === updatedRun.id);
  if (idx >= 0) {
    const next = [...runs];
    next[idx] = updatedRun;
    runs = next;
  } else {
    runs = [...runs, updatedRun];
  }
}

function handleRunUpdated(run: CollaborativeRun): void {
  updateRunInList(run);
}

function handleStageReady(data: { runId: string; stageName: string; ownerEmail: string }): void {
  const currentUser = authStore.user;
  if (!currentUser) return;

  // Check if this stage is assigned to the current user
  if (data.ownerEmail === currentUser.email) {
    console.log(
      `[Weplex] Stage "${data.stageName}" in run ${data.runId} is ready for you`,
    );
    // Trigger native notification via Tauri
    triggerNativeNotification(
      'Pipeline Stage Ready',
      `Stage "${data.stageName}" is waiting for you`,
    );
  }
}

function handleNotification(data: PipelineNotification): void {
  console.log('[Weplex] Pipeline notification:', data.type, data.title);
  triggerNativeNotification(data.title, data.body);
}

async function triggerNativeNotification(title: string, body: string): Promise<void> {
  await showNativeNotification(title, body);
}

/** Handle MCP stage completion for collaborative runs — sync artifact to server. */
async function handleMcpStageComplete(payload: McpStageCompletePayload): Promise<void> {
  const run = runs.find((r) => r.id === payload.run_id);
  if (!run || run.status !== 'running') return;

  const stage = run.stages.find((s) => s.name === payload.stage_name);
  if (!stage || stage.status !== 'running') return;

  try {
    if (payload.status === 'success') {
      const updatedRun = await pipelineCollabService.completeStage(
        payload.run_id,
        payload.stage_name,
        payload.artifact,
      );
      updateRunInList(updatedRun);
      console.log(`[Weplex] Collab stage "${payload.stage_name}" completed via MCP`);
    } else {
      const updatedRun = await pipelineCollabService.failStage(
        payload.run_id,
        payload.stage_name,
        payload.error || 'MCP stage failed',
      );
      updateRunInList(updatedRun);
      console.error(`[Weplex] Collab stage "${payload.stage_name}" failed via MCP: ${payload.error}`);
    }
  } catch (e) {
    console.error(`[Weplex] Failed to sync MCP stage result to server:`, e);
  }
}

async function setupMcpListener(): Promise<void> {
  if (mcpUnlisten) return;
  mcpUnlisten = await listen<McpStageCompletePayload>('mcp-stage-complete', (event) => {
    // Only handle events for collaborative runs (check if run ID matches a collab run)
    const run = runs.find((r) => r.id === event.payload.run_id);
    if (run) {
      handleMcpStageComplete(event.payload);
    }
  });
}

/** Pre-fetch artifacts for a collaborative stage's dependencies. */
async function prefetchArtifacts(runId: string, receives: string[]): Promise<void> {
  for (const dep of receives) {
    try {
      const artifact = await pipelineCollabService.getArtifact(runId, dep);
      // Store artifact locally so MCP deck_get_artifact can find it
      await invoke('set_run_artifact', { runId, stageName: dep, artifact });
    } catch (e) {
      console.warn(`[Weplex] Failed to pre-fetch artifact for "${dep}":`, e);
    }
  }
}

/** Clean up pipeline-specific subscriptions without disconnecting the shared WS. */
function cleanupPipelineSubscriptions(): void {
  unsubRunUpdated?.();
  unsubStageReady?.();
  unsubNotification?.();
  mcpUnlisten?.();
  unsubRunUpdated = null;
  unsubStageReady = null;
  unsubNotification = null;
  mcpUnlisten = null;
}

/** Full WS teardown — only call on logout when nothing else needs the connection. */
function cleanupWs(): void {
  cleanupPipelineSubscriptions();
  pipelineWsService.disconnect();
}

// ── Store ──────────────────────────────────────────────────────────────────

export const collabPipelineStore = {
  get runs() {
    return runs;
  },
  get activeRunId() {
    return activeRunId;
  },
  get activeRun(): CollaborativeRun | undefined {
    return runs.find((r) => r.id === activeRunId);
  },
  get loading() {
    return loading;
  },
  get error() {
    return error;
  },

  /** Fetch active runs and connect WebSocket. Call after auth + team init. */
  async init(): Promise<void> {
    loading = true;
    error = null;

    // Connect WebSocket FIRST — before any fetches, unconditionally
    // WS is needed for presence, team events, space events — not just pipeline runs
    const token = getAccessToken();
    if (token) {
      pipelineWsService.connect(token);
      console.log('[Weplex] WS connect initiated with token');
    } else {
      console.warn('[Weplex] No access token available for WS connection');
    }

    try {
      const teamId = teamStore.activeTeamId;
      if (!teamId) {
        // No active team — nothing to fetch, but WS is already connected
        runs = [];
        loading = false;
        return;
      }

      // Fetch active/pending/running runs for the active team
      const [active, pending] = await Promise.all([
        pipelineCollabService.getRuns(teamId, 'running').catch(() => [] as CollaborativeRun[]),
        pipelineCollabService.getRuns(teamId, 'pending').catch(() => [] as CollaborativeRun[]),
      ]);
      runs = [...active, ...pending];

      // Set up MCP event listener for collaborative stage completions
      setupMcpListener().catch((e) =>
        console.warn('[Weplex] Failed to set up collab MCP listener:', e),
      );

      // Clean up any existing subscriptions before re-subscribing (prevents double handlers)
      cleanupPipelineSubscriptions();

      // Subscribe to WS events (WS already connected above)
      if (token) {

        unsubRunUpdated = pipelineWsService.onRunUpdated(handleRunUpdated);
        unsubStageReady = pipelineWsService.onStageReady(handleStageReady);
        unsubNotification = pipelineWsService.onNotification(handleNotification);

        // Join rooms for all active runs
        for (const run of runs) {
          pipelineWsService.joinRun(run.id);
        }
      }
    } catch (e) {
      console.warn('[Weplex] Collaborative pipeline init failed:', e);
      error = e instanceof Error ? e.message : 'Failed to load collaborative runs';
    } finally {
      loading = false;
    }
  },

  /** Create a new collaborative pipeline run. Returns the run ID. */
  async startCollabRun(
    pipelineName: string,
    task: string,
    stages: StageDefinitionPayload[],
  ): Promise<string> {
    loading = true;
    error = null;
    try {
      const teamId = teamStore.activeTeamId;
      if (!teamId) {
        throw new Error('No active team selected');
      }
      const response = await pipelineCollabService.createRun({
        teamId,
        pipelineName,
        task,
        stages,
      });
      const { run, warnings } = response;
      if (warnings.length > 0) {
        console.warn('[Weplex] Create run warnings:', warnings);
      }
      runs = [...runs, run];
      activeRunId = run.id;

      // Join WS room for this run
      pipelineWsService.joinRun(run.id);

      return run.id;
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to create collaborative run';
      throw e;
    } finally {
      loading = false;
    }
  },

  /** Mark a stage as running (claimed by current user). Pre-fetches dependency artifacts. */
  async claimStage(runId: string, stageName: string): Promise<void> {
    error = null;
    try {
      // Find the stage to get its dependency list
      const run = runs.find((r) => r.id === runId);
      const stage = run?.stages.find((s) => s.name === stageName);

      // Pre-fetch artifacts from dependencies so MCP deck_get_artifact works locally
      if (stage && stage.receives.length > 0) {
        await prefetchArtifacts(runId, stage.receives);
      }

      const updatedRun = await pipelineCollabService.startStage(runId, stageName);
      updateRunInList(updatedRun);
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to claim stage';
      throw e;
    }
  },

  /** Report stage completion with artifact. */
  async reportStageComplete(runId: string, stageName: string, artifact: string): Promise<void> {
    error = null;
    try {
      const updatedRun = await pipelineCollabService.completeStage(runId, stageName, artifact);
      updateRunInList(updatedRun);
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to complete stage';
      throw e;
    }
  },

  /** Report stage failure. */
  async reportStageFail(runId: string, stageName: string, errorMsg: string): Promise<void> {
    error = null;
    try {
      const updatedRun = await pipelineCollabService.failStage(runId, stageName, errorMsg);
      updateRunInList(updatedRun);
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to report stage failure';
      throw e;
    }
  },

  /** Cancel a collaborative run. */
  async cancelRun(runId: string): Promise<void> {
    error = null;
    try {
      const updatedRun = await pipelineCollabService.cancelRun(runId);
      updateRunInList(updatedRun);
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to cancel run';
      throw e;
    }
  },

  /** Fetch artifact for a completed stage. */
  async getArtifact(runId: string, stageName: string): Promise<string> {
    return pipelineCollabService.getArtifact(runId, stageName);
  },

  /** Set the currently viewed run. */
  setActiveRun(runId: string | null): void {
    activeRunId = runId;
  },

  /** Clear pipeline state and unsubscribe events. Does NOT disconnect WS (shared for presence/team/space). */
  reset(): void {
    cleanupPipelineSubscriptions();
    runs = [];
    activeRunId = null;
    loading = false;
    error = null;
  },

  /** Full teardown including WS disconnect. Call only on logout. */
  destroy(): void {
    cleanupWs();
    runs = [];
    activeRunId = null;
    loading = false;
    error = null;
  },

  clearError(): void {
    error = null;
  },

  get runningCount(): number {
    return runs.filter((r) => r.status === 'running').length;
  },
};

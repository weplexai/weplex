/**
 * Hook Event Store — receives real-time tool use events from Claude Code
 * via Tauri hook-event, tracks per-session activity, and detects conflicts.
 */

import { listen } from '@tauri-apps/api/event';
import type {
  HookEventPayload,
  SessionActivity,
  ToolUseEntry,
  SubAgent,
} from '../types';
import { sessionStore } from './sessionStore.svelte';

// ── State ────────────────────────────────────────────────────────────────────

/** Per-session activity data. Key = session_id. */
let activities = $state<Map<number, SessionActivity>>(new Map());

/** Recent file edits for conflict detection: file_path → [session_id, timestamp][] */
let recentEdits = $state<Map<string, { sessionId: number; timestamp: number }[]>>(new Map());

/** Active conflicts: file_path → session_ids that touched it recently */
let conflicts = $state<Map<string, number[]>>(new Map());

/** Last hook event (for detail panel) */
let lastEvent = $state<HookEventPayload | null>(null);

/** Sub-agents per session: session_id → SubAgent[] */
let subAgents = $state<Map<number, SubAgent[]>>(new Map());

/** Sessions that have received native SubagentStart/Stop events (skip PreToolUse fallback) */
const nativeSubagentSessions = new Set<number>();

// ── Constants ────────────────────────────────────────────────────────────────

const MAX_TOOL_USES = 50;
const MAX_FILES_TOUCHED = 200;
const MAX_SUB_AGENTS = 100;
const CONFLICT_WINDOW_MS = 60_000; // 1 minute window for conflict detection
const CLEANUP_INTERVAL_MS = 5 * 60_000; // 5 minutes

// ── Internal helpers ─────────────────────────────────────────────────────────

function getOrCreateActivity(sessionId: number): SessionActivity {
  let activity = activities.get(sessionId);
  if (!activity) {
    activity = { toolUses: [], filesTouched: [], totalToolCalls: 0 };
    activities.set(sessionId, activity);
  }
  return activity;
}

// ── Event handlers (one per event type) ─────────────────────────────────────

function handleToolUse(event: HookEventPayload, activity: SessionActivity) {
  const entry: ToolUseEntry = {
    toolName: event.tool_name || 'unknown',
    filePath: event.file_path || undefined,
    timestamp: event.timestamp,
    type: event.event_type === 'pre_tool_use' ? 'pre' : 'post',
  };

  if (event.event_type === 'post_tool_use') {
    activity.totalToolCalls++;
  }

  const currentStatus = sessionStore.sessions.find((s) => s.id === event.session_id)?.status;
  if (event.event_type === 'pre_tool_use' && (currentStatus === 'idle' || currentStatus === 'thinking')) {
    sessionStore.updateStatus(event.session_id, 'active');
  }

  activity.toolUses.push(entry);
  if (activity.toolUses.length > MAX_TOOL_USES) activity.toolUses.shift();

  if (event.file_path && isFileModifyTool(event.tool_name)) {
    if (!activity.filesTouched.includes(event.file_path)) {
      activity.filesTouched.push(event.file_path);
      if (activity.filesTouched.length > MAX_FILES_TOUCHED) activity.filesTouched.shift();
    }
    if (event.event_type === 'post_tool_use') {
      trackFileEdit(event.file_path, event.session_id, event.timestamp);
    }
  }

  activities = new Map(activities);
}

function handleStop(event: HookEventPayload) {
  sessionStore.updateStatus(event.session_id, 'idle');

  const sessionSubs = subAgents.get(event.session_id);
  if (sessionSubs) {
    let changed = false;
    for (const sub of sessionSubs) {
      if (sub.status === 'running') {
        sub.status = 'completed';
        sub.finishedAt = event.timestamp;
        changed = true;
      }
    }
    if (changed) subAgents = new Map(subAgents);
  }
  activities = new Map(activities);
}

function handleSubagentStart(event: HookEventPayload) {
  if (!event.agent_id) return;
  const sessionSubs = subAgents.get(event.session_id) || [];
  nativeSubagentSessions.add(event.session_id);

  const parentStatus = sessionStore.sessions.find((s) => s.id === event.session_id)?.status;
  if (parentStatus !== 'error') {
    sessionStore.updateStatus(event.session_id, 'active');
  }

  const existing = sessionSubs.find((s) => s.agentId === event.agent_id);
  if (existing) {
    if (existing.agentType === 'unknown' && event.agent_type) {
      existing.agentType = event.agent_type;
    }
  } else {
    sessionSubs.push({
      agentId: event.agent_id,
      agentType: event.agent_type || 'unknown',
      sessionId: event.session_id,
      startedAt: event.timestamp,
      status: 'running',
    });
  }
  capSubAgents(sessionSubs);
  subAgents.set(event.session_id, sessionSubs);
  subAgents = new Map(subAgents);
}

function handleSubagentStop(event: HookEventPayload) {
  if (!event.agent_id) return;
  nativeSubagentSessions.add(event.session_id);
  const sessionSubs = subAgents.get(event.session_id) || [];

  const willBeRunning = sessionSubs.filter(
    (s) => s.status === 'running' && s.agentId !== event.agent_id,
  );
  if (willBeRunning.length === 0) {
    sessionStore.updateStatus(event.session_id, 'active');
  }

  const agent = sessionSubs.find((s) => s.agentId === event.agent_id);
  if (agent) {
    agent.finishedAt = event.timestamp;
    agent.status = 'completed';
  } else {
    sessionSubs.push({
      agentId: event.agent_id,
      agentType: event.agent_type || 'unknown',
      sessionId: event.session_id,
      startedAt: event.timestamp,
      finishedAt: event.timestamp,
      status: 'completed',
    });
    capSubAgents(sessionSubs);
  }
  subAgents.set(event.session_id, sessionSubs);
  subAgents = new Map(subAgents);
}

function handleAgentToolFallback(event: HookEventPayload) {
  if (event.tool_name !== 'Agent' || event.event_type !== 'pre_tool_use') return;
  if (nativeSubagentSessions.has(event.session_id)) return;

  let agentType = 'Agent';
  if (event.tool_input) {
    try {
      const input = JSON.parse(event.tool_input);
      if (input.subagent_type) agentType = input.subagent_type;
      else if (input.description) agentType = `Agent: ${input.description.slice(0, 30)}`;
    } catch { /* ignore */ }
  }

  const sessionSubs = subAgents.get(event.session_id) || [];
  sessionSubs.push({
    agentId: `tool-${event.timestamp}`,
    agentType,
    sessionId: event.session_id,
    startedAt: event.timestamp,
    status: 'running',
  });
  capSubAgents(sessionSubs);
  subAgents.set(event.session_id, sessionSubs);
  subAgents = new Map(subAgents);
}

function handleSessionStart(event: HookEventPayload) {
  if (!event.claude_session_id) return;
  const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
  if (UUID_RE.test(event.claude_session_id)) {
    sessionStore.update(event.session_id, { claudeSessionId: event.claude_session_id });
  }
}

// ── Main event dispatcher ───────────────────────────────────────────────────

function processEvent(event: HookEventPayload) {
  lastEvent = event;
  const activity = getOrCreateActivity(event.session_id);

  switch (event.event_type) {
    case 'pre_tool_use':
    case 'post_tool_use':
      handleToolUse(event, activity);
      handleAgentToolFallback(event);
      break;
    case 'stop':
      handleStop(event);
      break;
    case 'subagent_start':
      handleSubagentStart(event);
      break;
    case 'subagent_stop':
      handleSubagentStop(event);
      break;
    case 'session_start':
      handleSessionStart(event);
      break;
  }
}

/** Cap sub-agents list: remove oldest completed entries when over limit. */
function capSubAgents(subs: SubAgent[]) {
  while (subs.length > MAX_SUB_AGENTS) {
    const oldestCompleted = subs.findIndex((s) => s.status !== 'running');
    if (oldestCompleted >= 0) {
      subs.splice(oldestCompleted, 1);
    } else {
      break; // all running — don't evict
    }
  }
}

function isFileModifyTool(toolName?: string): boolean {
  if (!toolName) return false;
  return ['Write', 'Edit', 'MultiEdit', 'NotebookEdit'].includes(toolName);
}

function trackFileEdit(filePath: string, sessionId: number, timestamp: number) {
  const edits = recentEdits.get(filePath) || [];

  // Clean old entries
  const cutoff = Date.now() - CONFLICT_WINDOW_MS;
  const recent = edits.filter((e) => e.timestamp > cutoff);

  recent.push({ sessionId, timestamp });
  recentEdits.set(filePath, recent);

  // Check for conflict: multiple sessions editing the same file
  const uniqueSessions = [...new Set(recent.map((e) => e.sessionId))];
  if (uniqueSessions.length > 1) {
    conflicts.set(filePath, uniqueSessions);
    conflicts = new Map(conflicts);
  } else {
    if (conflicts.has(filePath)) {
      conflicts.delete(filePath);
      conflicts = new Map(conflicts);
    }
  }
}

// ── Listener setup ───────────────────────────────────────────────────────────

let listenerStarted = false;

function startListener() {
  if (listenerStarted) return;
  listenerStarted = true;

  listen<HookEventPayload>('hook-event', (event) => {
    processEvent(event.payload);
  });
}

// Auto-start listener on import
startListener();

// Periodic cleanup of stale recentEdits entries
setInterval(() => {
  const cutoff = Date.now() - CONFLICT_WINDOW_MS;
  let changed = false;
  for (const [filePath, edits] of recentEdits) {
    const recent = edits.filter((e) => e.timestamp > cutoff);
    if (recent.length === 0) {
      recentEdits.delete(filePath);
      conflicts.delete(filePath);
      changed = true;
    } else if (recent.length !== edits.length) {
      recentEdits.set(filePath, recent);
      changed = true;
    }
  }
  if (changed) {
    recentEdits = new Map(recentEdits);
    conflicts = new Map(conflicts);
  }
}, CLEANUP_INTERVAL_MS);

// ── Public API ───────────────────────────────────────────────────────────────

export const hookStore = {
  /** Get activity for a specific session. */
  getActivity(sessionId: number): SessionActivity | undefined {
    return activities.get(sessionId);
  },

  /** Get all active conflicts. */
  get conflicts() {
    return conflicts;
  },

  /** Get the last hook event (for detail panel). */
  get lastEvent() {
    return lastEvent;
  },

  /** Get all session activities. */
  get activities() {
    return activities;
  },

  /** Check if a session has any recorded activity. */
  hasActivity(sessionId: number): boolean {
    const a = activities.get(sessionId);
    return !!a && a.totalToolCalls > 0;
  },

  /** Get files touched by a session. */
  getFilesTouched(sessionId: number): string[] {
    return activities.get(sessionId)?.filesTouched || [];
  },

  /** Get conflicts involving a specific session. */
  getConflictsForSession(sessionId: number): { filePath: string; otherSessions: number[] }[] {
    const result: { filePath: string; otherSessions: number[] }[] = [];
    for (const [filePath, sessions] of conflicts) {
      if (sessions.includes(sessionId)) {
        result.push({
          filePath,
          otherSessions: sessions.filter((s) => s !== sessionId),
        });
      }
    }
    return result;
  },

  /** Get sub-agents for a session. */
  getSubAgents(sessionId: number): SubAgent[] {
    return subAgents.get(sessionId) || [];
  },

  /** Get currently running sub-agents for a session. */
  getRunningSubAgents(sessionId: number): SubAgent[] {
    return (subAgents.get(sessionId) || []).filter((s) => s.status === 'running');
  },

  /** Clear activity for a closed session. */
  clearSession(sessionId: number) {
    activities.delete(sessionId);
    subAgents.delete(sessionId);
    nativeSubagentSessions.delete(sessionId);
    activities = new Map(activities);
    subAgents = new Map(subAgents);
  },
};

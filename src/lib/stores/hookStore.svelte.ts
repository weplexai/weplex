/**
 * Hook Event Store — receives real-time tool use events from Claude Code
 * via Tauri hook-event, tracks per-session activity, and detects conflicts.
 */

import { listen } from '@tauri-apps/api/event';
import type {
  HookEventPayload,
  SessionActivity,
  ToolUseEntry,
} from '../types';

// ── State ────────────────────────────────────────────────────────────────────

/** Per-session activity data. Key = session_id. */
let activities = $state<Map<number, SessionActivity>>(new Map());

/** Recent file edits for conflict detection: file_path → [session_id, timestamp][] */
let recentEdits = $state<Map<string, { sessionId: number; timestamp: number }[]>>(new Map());

/** Active conflicts: file_path → session_ids that touched it recently */
let conflicts = $state<Map<string, number[]>>(new Map());

/** Last hook event (for detail panel) */
let lastEvent = $state<HookEventPayload | null>(null);

// ── Constants ────────────────────────────────────────────────────────────────

const MAX_TOOL_USES = 50;
const MAX_FILES_TOUCHED = 200;
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

function processEvent(event: HookEventPayload) {
  lastEvent = event;

  const activity = getOrCreateActivity(event.session_id);

  if (event.event_type === 'pre_tool_use' || event.event_type === 'post_tool_use') {
    const entry: ToolUseEntry = {
      toolName: event.tool_name || 'unknown',
      filePath: event.file_path || undefined,
      timestamp: event.timestamp,
      type: event.event_type === 'pre_tool_use' ? 'pre' : 'post',
    };

    // Only count on post (avoid double-counting)
    if (event.event_type === 'post_tool_use') {
      activity.totalToolCalls++;
    }

    // Add to recent tool uses (ring buffer)
    activity.toolUses.push(entry);
    if (activity.toolUses.length > MAX_TOOL_USES) {
      activity.toolUses.shift();
    }

    // Track file touches (capped)
    if (event.file_path && isFileModifyTool(event.tool_name)) {
      if (!activity.filesTouched.includes(event.file_path)) {
        activity.filesTouched.push(event.file_path);
        if (activity.filesTouched.length > MAX_FILES_TOUCHED) {
          activity.filesTouched.shift();
        }
      }

      // Conflict detection: track which sessions touch which files
      if (event.event_type === 'post_tool_use') {
        trackFileEdit(event.file_path, event.session_id, event.timestamp);
      }
    }

    // Trigger reactivity
    activities = new Map(activities);
  }

  if (event.event_type === 'stop') {
    // Session stopped — could update session status via sessionStore
    // For now just record it
    activities = new Map(activities);
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

  /** Clear activity for a closed session. */
  clearSession(sessionId: number) {
    activities.delete(sessionId);
    activities = new Map(activities);
  },
};

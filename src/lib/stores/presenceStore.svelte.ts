// Presence store — tracks team member sessions in shared/team spaces

import type { MemberPresence, SessionMeta, SessionRecord } from '../types';
import { pipelineWsService } from '../services/pipelineWsService';
import { spaceStore } from './spaceStore';
import { sessionStore } from './sessionStore';
import { authStore } from './authStore.svelte';
import { spaceService } from '../services/spaceService';
import { invoke } from '@tauri-apps/api/core';

const SYNC_INTERVAL_MS = 10_000; // 10 seconds

// ── State ──────────────────────────────────────────────────────────────────

let presenceMap = $state<Record<string, MemberPresence[]>>({});
let sessionHistory = $state<Record<string, SessionRecord[]>>({});
let historyLoading = $state<Record<string, boolean>>({});
let historyLoadedAt: Record<string, number> = {}; // timestamp of last load per space (cooldown)
let syncTimer: ReturnType<typeof setInterval> | null = null;
let unsubSessions: (() => void) | null = null;
let unsubOffline: (() => void) | null = null;

// ── Summary cache (read from local files via Tauri command) ───────────────

interface SummaryData {
  summary: string;
  filesChanged: string[];
  decisions: string[];
  updatedAt: number;
}

let summaryCache: Record<string, SummaryData> = {};

// ── Helpers ────────────────────────────────────────────────────────────────

/** Build SessionMeta array from local sessions for a given space. */
function buildLocalSessionMeta(spaceId: string): SessionMeta[] {
  const sessions = sessionStore.sessions.filter((s) => s.spaceId === spaceId);
  const now = new Date().toISOString();

  return sessions.map((s) => {
    const cached = summaryCache[String(s.id)];
    return {
      id: String(s.id),
      name: s.name,
      status: s.status === 'active' ? 'active' : s.status === 'idle' ? 'idle' : 'closed',
      agentType: s.agentType,
      cwd: s.cwd,
      gitBranch: s.branch,
      shared: true,
      createdAt: new Date(s.createdAt).toISOString(),
      updatedAt: now,
      summary: cached?.summary,
      filesChanged: cached?.filesChanged,
      decisions: cached?.decisions,
    };
  });
}

/** Refresh the summary cache by reading files for all active sessions in a space. */
async function refreshSummaryCache(spaceId: string): Promise<void> {
  const sessions = sessionStore.sessions.filter((s) => s.spaceId === spaceId);
  for (const s of sessions) {
    try {
      const data = await invoke<SummaryData | null>('get_session_summary', {
        sessionId: String(s.id),
      });
      if (data) {
        summaryCache[String(s.id)] = data;
      }
    } catch {
      // Summary file may not exist yet — that's fine
    }
  }
}

/** Get all shared/team space IDs that are currently relevant. */
function getSharedSpaceIds(): string[] {
  return spaceStore.spaces
    .filter((s) => (s.shared || s.type === 'team') && s.serverId)
    .map((s) => s.serverId!)
    .filter(Boolean);
}

/** Sync sessions for all active shared spaces. */
async function syncAllSharedSpaces(): Promise<void> {
  if (!authStore.isAuthenticated) {
    console.debug('[presence] skip sync: not authenticated');
    return;
  }
  const activeSpace = spaceStore.activeSpace;
  if (!activeSpace || (!activeSpace.shared && activeSpace.type !== 'team')) {
    console.debug('[presence] skip sync: active space not shared/team', activeSpace?.name, activeSpace?.type, activeSpace?.shared);
    return;
  }
  if (!activeSpace.serverId) {
    console.debug('[presence] skip sync: no serverId on active space');
    return;
  }

  // Refresh summary cache before building metadata
  await refreshSummaryCache(activeSpace.id);

  const sessions = buildLocalSessionMeta(activeSpace.id);
  console.log(`[presence] syncing ${sessions.length} sessions for space "${activeSpace.name}" (${activeSpace.serverId}), WS connected: ${pipelineWsService.isConnected()}`);
  pipelineWsService.syncSessions(activeSpace.serverId, sessions);
}

// ── Store ──────────────────────────────────────────────────────────────────

export const presenceStore = {
  get presenceMap() {
    return presenceMap;
  },

  get sessionHistory() {
    return sessionHistory;
  },

  /** Get members present in a specific space (by serverId). */
  getMembers(serverId: string): MemberPresence[] {
    return presenceMap[serverId] ?? [];
  },

  /** Get session history records for a space (by serverId). */
  getHistory(serverId: string): SessionRecord[] {
    return sessionHistory[serverId] ?? [];
  },

  /** Check if history is currently loading for a space. */
  isHistoryLoading(serverId: string): boolean {
    return historyLoading[serverId] ?? false;
  },

  /** Load session history from the server for a shared/team space. */
  async loadHistory(serverId: string): Promise<void> {
    if (!authStore.isAuthenticated) return;
    // Prevent duplicate/loop: skip if already loading or recently loaded
    if (historyLoading[serverId]) return;
    if (sessionHistory[serverId] && historyLoadedAt[serverId] && Date.now() - historyLoadedAt[serverId] < 30_000) return;
    historyLoading = { ...historyLoading, [serverId]: true };
    try {
      const records = await spaceService.getSessionHistory(serverId);
      sessionHistory = { ...sessionHistory, [serverId]: records };
      historyLoadedAt[serverId] = Date.now();
    } catch (err) {
      console.warn('[Weplex] Failed to load session history:', err);
      // On error, set a cooldown to prevent retry loops
      historyLoadedAt[serverId] = Date.now();
    } finally {
      historyLoading = { ...historyLoading, [serverId]: false };
    }
  },

  /** Subscribe to WebSocket presence events. Call once after WS connects. */
  init(): void {
    // Clean up previous subscriptions
    this.reset();

    unsubSessions = pipelineWsService.onSpaceSessions((data) => {
      presenceMap = { ...presenceMap, [data.spaceId]: data.members };
    });

    unsubOffline = pipelineWsService.onMemberOffline((data) => {
      const members = presenceMap[data.spaceId];
      if (members) {
        presenceMap = {
          ...presenceMap,
          [data.spaceId]: members.filter((m) => m.userId !== data.userId),
        };
      }
    });
  },

  /** Start periodic session sync for active shared/team spaces. */
  startSyncing(): void {
    if (syncTimer) return;
    console.log('[presence] startSyncing called');
    syncAllSharedSpaces();
    syncTimer = setInterval(syncAllSharedSpaces, SYNC_INTERVAL_MS);
  },

  /** Stop periodic syncing. */
  stopSyncing(): void {
    if (syncTimer) {
      clearInterval(syncTimer);
      syncTimer = null;
    }
  },

  /** Force an immediate sync for a specific space. */
  syncNow(spaceId: string): void {
    const space = spaceStore.spaces.find((s) => s.id === spaceId);
    if (!space?.serverId) return;

    const sessions = buildLocalSessionMeta(spaceId);
    pipelineWsService.syncSessions(space.serverId, sessions);
  },

  /** Clean up all state and subscriptions. */
  reset(): void {
    this.stopSyncing();
    if (unsubSessions) {
      unsubSessions();
      unsubSessions = null;
    }
    if (unsubOffline) {
      unsubOffline();
      unsubOffline = null;
    }
    presenceMap = {};
    sessionHistory = {};
    historyLoading = {};
    historyLoadedAt = {};
  },
};

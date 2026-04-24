// Presence store — tracks team member sessions in shared/team spaces.
//
// Deliberate non-goal: broadcasting what agents are doing inside each member's
// sessions. That used to be part of the presence payload (summary/notes/files)
// but constituted real-time surveillance of teammates. It's been removed.
// Agents' activity logs stay private to the user's machine; sharing is now
// opt-in via the 📋 button in SpaceChat, not automatic.

import type { MemberPresence, SessionMeta, SessionRecord } from '../types';
import { wsService } from '../services/wsService';
import { spaceStore } from './spaceStore';
import { sessionStore } from './sessionStore';
import { authStore } from './authStore.svelte';
import { spaceService } from '../services/spaceService';

const SYNC_INTERVAL_MS = 10_000; // 10 seconds

// ── State ──────────────────────────────────────────────────────────────────

let presenceMap = $state<Record<string, MemberPresence[]>>({});
let sessionHistory = $state<Record<string, SessionRecord[]>>({});
let historyLoading = $state<Record<string, boolean>>({});
let historyLoadedAt: Record<string, number> = {}; // timestamp of last load per space (cooldown)
let syncTimer: ReturnType<typeof setInterval> | null = null;
let unsubSessions: (() => void) | null = null;
let unsubOffline: (() => void) | null = null;

// ── Helpers ────────────────────────────────────────────────────────────────

/** Shallow-compare two MemberPresence arrays to avoid unnecessary re-renders. */
function membersEqual(a: MemberPresence[], b: MemberPresence[]): boolean {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) {
    if (a[i].userId !== b[i].userId) return false;
    if (a[i].displayName !== b[i].displayName) return false;
    const sa = a[i].sessions;
    const sb = b[i].sessions;
    if (sa.length !== sb.length) return false;
    for (let j = 0; j < sa.length; j++) {
      if (
        sa[j].id !== sb[j].id ||
        sa[j].status !== sb[j].status ||
        sa[j].cwd !== sb[j].cwd ||
        sa[j].gitBranch !== sb[j].gitBranch
      ) return false;
    }
  }
  return true;
}

/** Build SessionMeta array from local sessions for a given space.
 *
 *  Only identity fields are included — name, status, cwd, git branch.
 *  Activity content (summary/notes/files/decisions) is intentionally NOT
 *  pushed: it's private to the machine that wrote it. See the file header. */
function buildLocalSessionMeta(spaceId: string): SessionMeta[] {
  const sessions = sessionStore.sessions.filter((s) => s.spaceId === spaceId);
  const now = new Date().toISOString();

  return sessions.map((s) => ({
    id: String(s.id),
    name: s.name,
    status: s.status === 'active' ? 'active' : s.status === 'idle' ? 'idle' : 'closed',
    agentType: s.agentType,
    cwd: s.cwd,
    gitBranch: s.branch,
    shared: true,
    createdAt: new Date(s.createdAt).toISOString(),
    updatedAt: now,
  }));
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
  if (!authStore.isAuthenticated) return;
  const activeSpace = spaceStore.activeSpace;
  if (!activeSpace || (!activeSpace.shared && activeSpace.type !== 'team')) return;
  if (!activeSpace.serverId) return;

  const sessions = buildLocalSessionMeta(activeSpace.id);
  wsService.syncSessions(activeSpace.serverId, sessions);
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

    unsubSessions = wsService.onSpaceSessions((data) => {
      const existing = presenceMap[data.spaceId];
      if (existing && membersEqual(existing, data.members)) return;
      presenceMap = { ...presenceMap, [data.spaceId]: data.members };
    });

    unsubOffline = wsService.onMemberOffline((data) => {
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
    wsService.syncSessions(space.serverId, sessions);
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

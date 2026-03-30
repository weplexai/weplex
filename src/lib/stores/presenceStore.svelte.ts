// Presence store — tracks team member sessions in shared/team spaces

import type { MemberPresence, SessionMeta } from '../types';
import { pipelineWsService } from '../services/pipelineWsService';
import { spaceStore } from './spaceStore';
import { sessionStore } from './sessionStore';

const SYNC_INTERVAL_MS = 10_000; // 10 seconds

// ── State ──────────────────────────────────────────────────────────────────

let presenceMap = $state<Record<string, MemberPresence[]>>({});
let syncTimer: ReturnType<typeof setInterval> | null = null;
let unsubSessions: (() => void) | null = null;
let unsubOffline: (() => void) | null = null;

// ── Helpers ────────────────────────────────────────────────────────────────

/** Build SessionMeta array from local sessions for a given space. */
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
function syncAllSharedSpaces(): void {
  const activeSpace = spaceStore.activeSpace;
  if (!activeSpace || (!activeSpace.shared && activeSpace.type !== 'team')) return;
  if (!activeSpace.serverId) return;

  const sessions = buildLocalSessionMeta(activeSpace.id);
  pipelineWsService.syncSessions(activeSpace.serverId, sessions);
}

// ── Store ──────────────────────────────────────────────────────────────────

export const presenceStore = {
  get presenceMap() {
    return presenceMap;
  },

  /** Get members present in a specific space (by serverId). */
  getMembers(serverId: string): MemberPresence[] {
    return presenceMap[serverId] ?? [];
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
  },
};

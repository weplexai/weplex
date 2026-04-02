import type { Session, SessionStatus, HyperspaceGroupBy } from '../types';
import { HYPERSPACE_ID } from '../types';
import { detectAgentType, detectSessionType } from '../utils/detection';
import { durableSave, durableRemove } from '../utils/durablePersist';
import { spaceStore } from './spaceStore';

const STORAGE_KEY = 'weplex_sessions';
const ACTIVE_KEY = 'weplex_active_session';

// Load persisted state
function loadSessions(): { sessions: Session[]; nextId: number; activeId: number | null } {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return { sessions: [], nextId: 1, activeId: null };
    const saved: Session[] = JSON.parse(raw);
    const maxId = saved.reduce((max, s) => Math.max(max, s.id), 0);
    const activeRaw = localStorage.getItem(ACTIVE_KEY);
    const activeId = activeRaw
      ? Number(activeRaw)
      : saved.length > 0
        ? saved[saved.length - 1].id
        : null;
    // Reset status — PTY connections are gone after restart
    // Save previous status so resume logic can skip finished sessions
    // Backfill order for sessions created before order field existed
    for (const s of saved) {
      s.previousStatus = s.status;
      s.status = 'new';
      if (s.order === undefined) s.order = s.createdAt;
    }
    return { sessions: saved, nextId: maxId + 1, activeId };
  } catch {
    return { sessions: [], nextId: 1, activeId: null };
  }
}

const initial = loadSessions();
let nextId = initial.nextId;
let sessions = $state<Session[]>(initial.sessions);
let activeSessionId = $state<number | null>(initial.activeId);

// Track which sessions were restored from persistence (need special handling for PTY)
const restoredSessionIds = new Set<number>(initial.sessions.map((s) => s.id));

function persist() {
  try {
    durableSave(STORAGE_KEY, JSON.stringify(sessions));
    if (activeSessionId !== null) {
      durableSave(ACTIVE_KEY, String(activeSessionId));
    } else {
      durableRemove(ACTIVE_KEY);
    }
  } catch {}
}

export interface SessionGroup {
  key: string;
  label: string;
  color?: string;
  sessions: Session[];
}

export const sessionStore = {
  get sessions() {
    return sessions;
  },
  get activeSessionId() {
    return activeSessionId;
  },
  get activeSession(): Session | undefined {
    return sessions.find((s) => s.id === activeSessionId);
  },

  isRestored(id: number): boolean {
    return restoredSessionIds.has(id);
  },

  clearRestored(id: number) {
    restoredSessionIds.delete(id);
  },

  create(
    opts: {
      name?: string;
      command?: string;
      cwd?: string;
      spaceId?: string;
      profileId?: string;
      folderId?: string;
      pinned?: boolean;
      extraEnvVars?: Record<string, string>;
    } = {},
  ): Session {
    const id = nextId++;
    const now = Date.now();
    const type = opts.command ? detectSessionType(opts.command) : 'terminal';
    const agentType = type === 'agent' && opts.command ? detectAgentType(opts.command) : undefined;

    const session: Session = {
      id,
      name: opts.name || (agentType ? agentType : `session-${id}`),
      type,
      agentType,
      status: 'new',
      spaceId: opts.spaceId || 'default',
      profileId: opts.profileId,
      folderId: opts.folderId,
      pinned: opts.pinned || false,
      order: now,
      createdAt: now,
      lastActivity: now,
      command: opts.command,
      cwd: opts.cwd || '~',
      extraEnvVars: opts.extraEnvVars,
    };

    sessions.push(session);
    activeSessionId = id;
    spaceStore.setActiveSession(session.spaceId, id);
    persist();
    return session;
  },

  activate(id: number) {
    const session = sessions.find((s) => s.id === id);
    if (session) {
      activeSessionId = id;
      spaceStore.setActiveSession(session.spaceId, id);
      // Track in Hyperspace if currently active
      if (spaceStore.activeSpaceId === HYPERSPACE_ID) {
        spaceStore.setActiveSession(HYPERSPACE_ID, id);
      }
      persist();
    }
  },

  /** Activate the last active session for a given space */
  activateForSpace(spaceId: string) {
    if (spaceId === HYPERSPACE_ID) {
      const lastId = spaceStore.getActiveSession(HYPERSPACE_ID);
      if (lastId) {
        const session = sessions.find((s) => s.id === lastId);
        if (session) {
          activeSessionId = session.id;
          persist();
          return;
        }
      }
      // Fall back to most recently active session
      if (sessions.length > 0) {
        const sorted = [...sessions].sort((a, b) => b.lastActivity - a.lastActivity);
        activeSessionId = sorted[0].id;
        persist();
      } else {
        activeSessionId = null;
        persist();
      }
      return;
    }

    const lastId = spaceStore.getActiveSession(spaceId);
    const spaceSessions = sessions.filter((s) => s.spaceId === spaceId);

    if (spaceSessions.length === 0) {
      activeSessionId = null;
      persist();
      return;
    }

    const target = lastId ? spaceSessions.find((s) => s.id === lastId) : null;
    activeSessionId = target ? target.id : spaceSessions[spaceSessions.length - 1].id;
    persist();
  },

  update(id: number, patch: Partial<Session>) {
    const idx = sessions.findIndex((s) => s.id === id);
    if (idx !== -1) {
      sessions[idx] = { ...sessions[idx], ...patch };
      // Don't persist on every status/activity update — too noisy
      const keys = Object.keys(patch);
      const isStatusOnly = keys.every((k) => k === 'status' || k === 'lastActivity');
      if (!isStatusOnly) {
        persist();
      }
    }
  },

  updateStatus(id: number, status: SessionStatus) {
    this.update(id, { status, lastActivity: Date.now() });
  },

  updateActivity(id: number) {
    this.update(id, { lastActivity: Date.now() });
  },

  rename(id: number, name: string) {
    this.update(id, { name });
    persist();
  },

  pin(id: number) {
    const s = sessions.find((s) => s.id === id);
    if (s) {
      this.update(id, { pinned: !s.pinned });
      persist();
    }
  },

  kill(id: number) {
    const idx = sessions.findIndex((s) => s.id === id);
    if (idx === -1) return;

    // Kill the PTY backend
    import('@tauri-apps/api/core')
      .then(({ invoke }) => {
        invoke('kill_pty', { sessionId: id }).catch((e) =>
          console.error(`[Weplex] Failed to kill PTY ${id}:`, e),
        );
      })
      .catch((e) => console.error(`[Weplex] Failed to load Tauri API for killing PTY ${id}:`, e));

    sessions.splice(idx, 1);

    if (activeSessionId === id) {
      activeSessionId = sessions.length > 0 ? sessions[sessions.length - 1].id : null;
    }
    persist();
  },

  moveToSpace(id: number, spaceId: string) {
    this.update(id, { spaceId });
    persist();
  },

  getBySpace(spaceId: string): Session[] {
    return sessions.filter((s) => s.spaceId === spaceId);
  },

  /** Pinned sessions that belong to a specific folder. */
  getByFolder(folderId: string): Session[] {
    return sessions
      .filter((s) => s.pinned && s.folderId === folderId)
      .sort((a, b) => a.order - b.order);
  },

  /** Pinned sessions NOT in any folder. */
  getPinnedStandalone(spaceId: string): Session[] {
    return sessions
      .filter((s) => s.spaceId === spaceId && s.pinned && !s.folderId)
      .sort((a, b) => a.order - b.order);
  },

  /** Unpinned sessions (bottom section). */
  getUnpinned(spaceId: string): Session[] {
    return sessions
      .filter((s) => s.spaceId === spaceId && !s.pinned)
      .sort((a, b) => a.order - b.order);
  },

  /** All sessions grouped for Hyperspace view. */
  getAllGrouped(groupBy: HyperspaceGroupBy): SessionGroup[] {
    // For 'space' grouping, use stable order (matches individual space views)
    // For other groupings, sort by last activity (most recent first)
    const byOrder = [...sessions].sort((a, b) => a.order - b.order);
    const byActivity = [...sessions].sort((a, b) => b.lastActivity - a.lastActivity);

    switch (groupBy) {
      case 'space': {
        const groups = new Map<string, Session[]>();
        for (const s of byOrder) {
          if (!groups.has(s.spaceId)) groups.set(s.spaceId, []);
          groups.get(s.spaceId)!.push(s);
        }
        return spaceStore.spaces
          .filter((sp) => groups.has(sp.id))
          .map((sp) => ({
            key: sp.id,
            label: sp.name.toUpperCase(),
            color: sp.color,
            sessions: groups.get(sp.id)!,
          }));
      }

      case 'status': {
        const statusOrder: SessionStatus[] = [
          'active',
          'waiting',
          'new',
          'idle',
          'error',
          'disconnected',
        ];
        const statusLabels: Record<string, string> = {
          active: 'ACTIVE',
          waiting: 'WAITING',
          new: 'NEW',
          idle: 'IDLE',
          error: 'ERROR',
          disconnected: 'DISCONNECTED',
        };
        const groups = new Map<SessionStatus, Session[]>();
        for (const s of byActivity) {
          if (!groups.has(s.status)) groups.set(s.status, []);
          groups.get(s.status)!.push(s);
        }
        return statusOrder
          .filter((status) => groups.has(status))
          .map((status) => ({
            key: status,
            label: statusLabels[status],
            sessions: groups.get(status)!,
          }));
      }

      case 'project': {
        const groups = new Map<string, Session[]>();
        for (const s of byActivity) {
          const key = s.cwd || '__none__';
          if (!groups.has(key)) groups.set(key, []);
          groups.get(key)!.push(s);
        }
        return Array.from(groups.entries())
          .sort((a, b) => b[1].length - a[1].length)
          .map(([key, sess]) => ({
            key,
            label: key === '__none__' ? 'No directory' : key.split('/').pop() || key,
            sessions: sess,
          }));
      }
    }
  },

  /** Place session before another session (by id). If beforeId is null, place at end. */
  reorder(
    sessionId: number,
    beforeId: number | null,
    opts?: { pinned?: boolean; folderId?: string },
  ) {
    const s = sessions.find((s) => s.id === sessionId);
    if (!s) return;

    const patch: Partial<Session> = {};
    if (opts?.pinned !== undefined) patch.pinned = opts.pinned;
    if (opts?.folderId !== undefined) patch.folderId = opts.folderId;

    // Get siblings in the target zone
    const targetPinned = opts?.pinned ?? s.pinned;
    const targetFolder = opts?.folderId ?? s.folderId;
    const siblings = sessions
      .filter(
        (x) =>
          x.id !== sessionId &&
          x.pinned === targetPinned &&
          x.folderId === targetFolder &&
          x.spaceId === s.spaceId,
      )
      .sort((a, b) => a.order - b.order);

    if (beforeId === null) {
      // Place at end
      const last = siblings[siblings.length - 1];
      patch.order = last ? last.order + 1 : s.order;
    } else {
      const idx = siblings.findIndex((x) => x.id === beforeId);
      if (idx === 0) {
        patch.order = siblings[0].order - 1;
      } else if (idx > 0) {
        patch.order = (siblings[idx - 1].order + siblings[idx].order) / 2;
      }
    }

    Object.assign(patch, { pinned: targetPinned, folderId: targetFolder });
    this.update(sessionId, patch);
    persist();
  },

  get stats() {
    const active = sessions.filter((s) => s.status === 'active').length;
    const waiting = sessions.filter((s) => s.status === 'waiting').length;
    const idle = sessions.filter((s) => s.status === 'idle').length;
    const totalCost = sessions.reduce((sum, s) => sum + (s.cost || 0), 0);
    return { active, waiting, idle, total: sessions.length, totalCost };
  },
};

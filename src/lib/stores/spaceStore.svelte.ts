import type { Space, ServerSpace } from '../types';
import { SPACE_COLORS, HYPERSPACE_ID } from '../types';
import { durableSave } from '../utils/durablePersist';
import { spaceService } from '../services/spaceService';

const STORAGE_KEY = 'weplex_spaces';
const ACTIVE_KEY = 'weplex_active_space';
const SPACE_SESSIONS_KEY = 'weplex_space_sessions';

const DEFAULT_SPACE: Space = {
  id: 'default',
  name: 'Default',
  color: SPACE_COLORS[0],
  order: 0,
  type: 'personal',
  shared: false,
};

/** Migrate legacy spaces that lack type/shared fields. */
function migrateSpace(s: Space): Space {
  return {
    ...s,
    type: s.type ?? 'personal',
    shared: s.shared ?? false,
  };
}

function loadSpaces(): { spaces: Space[]; activeId: string } {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return { spaces: [DEFAULT_SPACE], activeId: 'default' };
    const saved: Space[] = JSON.parse(raw).map(migrateSpace);
    if (!saved.some((s) => s.id === 'default')) {
      saved.unshift(DEFAULT_SPACE);
    }
    const activeId = localStorage.getItem(ACTIVE_KEY) || 'default';
    return { spaces: saved, activeId };
  } catch {
    return { spaces: [DEFAULT_SPACE], activeId: 'default' };
  }
}

function loadSpaceSessions(): Record<string, number> {
  try {
    const raw = localStorage.getItem(SPACE_SESSIONS_KEY);
    return raw ? JSON.parse(raw) : {};
  } catch {
    return {};
  }
}

const initial = loadSpaces();
let spaces = $state<Space[]>(initial.spaces);
let activeSpaceId = $state(initial.activeId);
let transitionDirection = $state<'left' | 'right' | null>(null);
let editingSpaceIdVal = $state<string | null>(null);

// Per-space last active session id
let spaceSessions = $state<Record<string, number>>(loadSpaceSessions());

function persist() {
  try {
    durableSave(STORAGE_KEY, JSON.stringify(spaces));
    durableSave(ACTIVE_KEY, activeSpaceId);
    durableSave(SPACE_SESSIONS_KEY, JSON.stringify(spaceSessions));
  } catch {}
}

export const spaceStore = {
  get spaces() {
    return spaces;
  },
  get activeSpaceId() {
    return activeSpaceId;
  },
  get transitionDirection() {
    return transitionDirection;
  },
  clearTransition() {
    transitionDirection = null;
  },
  get editingSpaceId() {
    return editingSpaceIdVal;
  },
  set editingSpaceId(id: string | null) {
    editingSpaceIdVal = id;
  },
  get activeSpace(): Space {
    if (activeSpaceId === HYPERSPACE_ID) {
      return { id: HYPERSPACE_ID, name: 'All Spaces', color: '#9898A8', order: -1, type: 'personal', shared: false };
    }
    return spaces.find((s) => s.id === activeSpaceId) || spaces[0];
  },

  get isHyperspace(): boolean {
    return activeSpaceId === HYPERSPACE_ID;
  },

  activate(id: string) {
    if (id === HYPERSPACE_ID) {
      if (activeSpaceId !== HYPERSPACE_ID) {
        transitionDirection = 'right';
        activeSpaceId = HYPERSPACE_ID;
        persist();
      }
      return;
    }
    if (spaces.some((s) => s.id === id) && id !== activeSpaceId) {
      const oldIdx =
        activeSpaceId === HYPERSPACE_ID ? -1 : spaces.findIndex((s) => s.id === activeSpaceId);
      const newIdx = spaces.findIndex((s) => s.id === id);
      transitionDirection = newIdx > oldIdx ? 'left' : 'right';
      activeSpaceId = id;
      persist();
    }
  },

  create(
    name: string,
    color?: string,
    profileId?: string,
    bgColor?: string,
    directory?: string,
  ): Space {
    const id = `space-${Date.now()}`;
    const space: Space = {
      id,
      name,
      color: color || SPACE_COLORS[spaces.length % SPACE_COLORS.length],
      order: spaces.length,
      profileId,
      bgColor,
      directory,
      type: 'personal',
      shared: false,
    };
    spaces.push(space);
    persist();
    return space;
  },

  /** Toggle shared flag for a space. Syncs to server if it has a teamId. */
  async toggleShared(spaceId: string): Promise<void> {
    const idx = spaces.findIndex((s) => s.id === spaceId);
    if (idx === -1) return;

    const space = spaces[idx];
    const newShared = !space.shared;
    spaces[idx] = { ...space, shared: newShared };
    persist();

    // Sync to server if this space is linked to a team
    if (space.serverId) {
      try {
        await spaceService.updateSpace(space.serverId, { shared: newShared });
      } catch (e) {
        console.warn('[Weplex] Failed to sync shared toggle to server:', e);
      }
    }
  },

  /** Create a team space: creates on server first, then adds locally. */
  async createTeamSpace(
    name: string,
    color: string,
    teamId: string,
    createdBy?: string,
  ): Promise<Space> {
    const serverSpace = await spaceService.createSpace(teamId, name, color, 'team', true);

    const id = `space-${Date.now()}`;
    const space: Space = {
      id,
      name: serverSpace.name,
      color: serverSpace.color,
      order: spaces.length,
      type: 'team',
      shared: true,
      teamId,
      serverId: serverSpace.id,
      createdBy: createdBy || serverSpace.createdBy,
    };
    spaces.push(space);
    persist();
    return space;
  },

  /** Fetch shared/team spaces from server for a team and merge with local state. */
  async syncSharedSpaces(teamId: string): Promise<void> {
    try {
      const serverSpaces = await spaceService.listSpaces(teamId);

      for (const ss of serverSpaces) {
        // Check if already exists locally by serverId
        const existing = spaces.find((s) => s.serverId === ss.id);
        if (existing) {
          // Update name/color/shared from server
          const idx = spaces.indexOf(existing);
          spaces[idx] = {
            ...existing,
            name: ss.name,
            color: ss.color,
            shared: ss.shared,
          };
        } else {
          // Add new server space locally
          const id = `space-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
          spaces.push({
            id,
            name: ss.name,
            color: ss.color,
            order: spaces.length,
            type: ss.type,
            shared: ss.shared,
            teamId: ss.teamId,
            serverId: ss.id,
            createdBy: ss.createdBy,
          });
        }
      }

      persist();
    } catch (e) {
      console.warn('[Weplex] Failed to sync shared spaces:', e);
    }
  },

  update(id: string, patch: Partial<Omit<Space, 'id'>>) {
    const idx = spaces.findIndex((s) => s.id === id);
    if (idx !== -1) {
      spaces[idx] = { ...spaces[idx], ...patch };
      persist();
    }
  },

  switchToNext() {
    if (activeSpaceId === HYPERSPACE_ID) {
      if (spaces.length > 0) {
        transitionDirection = 'left';
        activeSpaceId = spaces[0].id;
        persist();
      }
      return;
    }
    const idx = spaces.findIndex((s) => s.id === activeSpaceId);
    if (idx < spaces.length - 1) {
      transitionDirection = 'left';
      activeSpaceId = spaces[idx + 1].id;
      persist();
    }
  },

  switchToPrevious() {
    if (activeSpaceId === HYPERSPACE_ID) return;
    const idx = spaces.findIndex((s) => s.id === activeSpaceId);
    if (idx === 0) {
      transitionDirection = 'right';
      activeSpaceId = HYPERSPACE_ID;
      persist();
    } else if (idx > 0) {
      transitionDirection = 'right';
      activeSpaceId = spaces[idx - 1].id;
      persist();
    }
  },

  // Per-space active session tracking
  setActiveSession(spaceId: string, sessionId: number) {
    spaceSessions[spaceId] = sessionId;
    persist();
  },

  getActiveSession(spaceId: string): number | null {
    return spaceSessions[spaceId] ?? null;
  },

  reorder(fromIndex: number, toIndex: number) {
    if (fromIndex === toIndex) return;
    if (fromIndex < 0 || fromIndex >= spaces.length) return;
    if (toIndex < 0 || toIndex >= spaces.length) return;
    const [moved] = spaces.splice(fromIndex, 1);
    spaces.splice(toIndex, 0, moved);
    // Update order fields
    for (let i = 0; i < spaces.length; i++) {
      spaces[i].order = i;
    }
    persist();
  },

  remove(id: string) {
    if (id === 'default') return;
    spaces = spaces.filter((s) => s.id !== id);
    if (activeSpaceId === id) {
      activeSpaceId = 'default';
    }
    delete spaceSessions[id];
    persist();
  },
};

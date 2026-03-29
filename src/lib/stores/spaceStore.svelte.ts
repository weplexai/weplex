import type { Space } from '../types';
import { SPACE_COLORS, HYPERSPACE_ID } from '../types';
import { durableSave } from '../utils/durablePersist';

const STORAGE_KEY = 'weplex_spaces';
const ACTIVE_KEY = 'weplex_active_space';
const SPACE_SESSIONS_KEY = 'weplex_space_sessions';

const DEFAULT_SPACE: Space = { id: 'default', name: 'Default', color: SPACE_COLORS[0], order: 0 };

function loadSpaces(): { spaces: Space[]; activeId: string } {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return { spaces: [DEFAULT_SPACE], activeId: 'default' };
    const saved: Space[] = JSON.parse(raw);
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
      return { id: HYPERSPACE_ID, name: 'All Spaces', color: '#9898A8', order: -1 };
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
    };
    spaces.push(space);
    persist();
    return space;
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

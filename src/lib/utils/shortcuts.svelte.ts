import { HYPERSPACE_ID } from '../types';
import { sessionStore } from '../stores/sessionStore';
import { spaceStore } from '../stores/spaceStore';
import { uiStore } from '../stores/uiStore';
import { splitStore } from '../stores/splitStore';
import { featureFlags } from '../stores/featureFlagsStore.svelte';

interface Shortcut {
  key: string;
  meta?: boolean;
  shift?: boolean;
  alt?: boolean;
  action: () => void;
  label: string;
  category: 'navigation' | 'sessions' | 'terminal';
}

const isMac = typeof navigator !== 'undefined' && navigator.platform.includes('Mac');

function mod(e: KeyboardEvent): boolean {
  return isMac ? e.metaKey : e.ctrlKey;
}

const shortcuts: Shortcut[] = [
  {
    key: 'k',
    meta: true,
    action: () => uiStore.toggleOverlay('command-palette'),
    label: 'Command palette',
    category: 'navigation',
  },
  {
    key: 'p',
    meta: true,
    action: () => uiStore.toggleOverlay('quick-switcher'),
    label: 'Quick switch',
    category: 'navigation',
  },
  {
    key: 'b',
    meta: true,
    action: () => uiStore.toggleSidebar(),
    label: 'Toggle sidebar',
    category: 'navigation',
  },
  {
    key: '.',
    meta: true,
    action: () => uiStore.toggleDetailPanel(),
    label: 'Toggle detail panel',
    category: 'navigation',
  },
  {
    key: 'a',
    meta: true,
    shift: true,
    action: () => {
      if (featureFlags.resources) uiStore.enterHubMode('resources');
    },
    label: 'Resources',
    category: 'navigation',
  },
  {
    key: 'h',
    meta: true,
    shift: true,
    action: () => uiStore.toggleHubMode(),
    label: 'Toggle Hub',
    category: 'navigation',
  },
  {
    key: ',',
    meta: true,
    action: () => uiStore.enterHubMode('settings'),
    label: 'Settings',
    category: 'navigation',
  },
  {
    key: 'n',
    meta: true,
    action: () => uiStore.openOverlay('new-session'),
    label: 'New session',
    category: 'sessions',
  },
  {
    key: 'w',
    meta: true,
    action: () => {
      // If there are splits, Cmd+W closes the focused pane (not the session)
      const spaceId = spaceStore.activeSpaceId;
      if (splitStore.hasSplits(spaceId)) {
        splitStore.closeFocusedPane(spaceId);
      } else {
        const active = sessionStore.activeSessionId;
        if (active !== null) sessionStore.kill(active);
      }
    },
    label: 'Close session',
    category: 'sessions',
  },
  {
    key: 'd',
    meta: true,
    action: () => {
      const spaceId = spaceStore.activeSpaceId;
      if (spaceId === HYPERSPACE_ID) return;
      splitStore.split(spaceId, 'horizontal');
    },
    label: 'Split horizontal',
    category: 'terminal',
  },
  {
    key: 'd',
    meta: true,
    shift: true,
    action: () => {
      const spaceId = spaceStore.activeSpaceId;
      if (spaceId === HYPERSPACE_ID) return;
      splitStore.split(spaceId, 'vertical');
    },
    label: 'Split vertical',
    category: 'terminal',
  },
  {
    key: ']',
    meta: true,
    action: () => {
      const spaceId = spaceStore.activeSpaceId;
      splitStore.focusNext(spaceId);
    },
    label: 'Focus next pane',
    category: 'terminal',
  },
  {
    key: '[',
    meta: true,
    action: () => {
      const spaceId = spaceStore.activeSpaceId;
      splitStore.focusPrev(spaceId);
    },
    label: 'Focus previous pane',
    category: 'terminal',
  },
  {
    key: 'ArrowDown',
    meta: true,
    action: () => {
      const sessions = sessionStore.sessions;
      const idx = sessions.findIndex((s) => s.id === sessionStore.activeSessionId);
      if (idx < sessions.length - 1) sessionStore.activate(sessions[idx + 1].id);
    },
    label: 'Next session',
    category: 'sessions',
  },
  {
    key: 'ArrowUp',
    meta: true,
    action: () => {
      const sessions = sessionStore.sessions;
      const idx = sessions.findIndex((s) => s.id === sessionStore.activeSessionId);
      if (idx > 0) sessionStore.activate(sessions[idx - 1].id);
    },
    label: 'Previous session',
    category: 'sessions',
  },
];

export function handleGlobalKeydown(e: KeyboardEvent) {
  if (!mod(e)) return;

  // Don't intercept when typing in real inputs (but allow xterm textarea)
  const target = e.target as HTMLElement;
  const tag = target?.tagName;
  const isXterm = target?.closest('.xterm');
  if ((tag === 'INPUT' || tag === 'TEXTAREA') && !isXterm) {
    if (e.key === 'Escape') {
      uiStore.closeOverlay();
      e.preventDefault();
    }
    return;
  }

  if (e.key === 'Escape') {
    if (uiStore.hubMode) {
      uiStore.exitHubMode();
      e.preventDefault();
      return;
    }
    uiStore.closeOverlay();
    e.preventDefault();
    return;
  }

  // Cmd+0 — Hyperspace
  if (e.key === '0' && !e.shiftKey) {
    e.preventDefault();
    if (uiStore.hubMode) uiStore.exitHubMode();
    spaceStore.activate(HYPERSPACE_ID);
    sessionStore.activateForSpace(HYPERSPACE_ID);
    return;
  }

  // Cmd+1..9 — Switch to space by index
  const digit = parseInt(e.key);
  if (digit >= 1 && digit <= 9 && !e.shiftKey) {
    if (uiStore.hubMode) uiStore.exitHubMode();
    const idx = digit - 1;
    if (idx < spaceStore.spaces.length) {
      e.preventDefault();
      const space = spaceStore.spaces[idx];
      spaceStore.activate(space.id);
      sessionStore.activateForSpace(space.id);
      return;
    }
  }

  for (const s of shortcuts) {
    if (s.key === e.key && (s.shift ? e.shiftKey : !e.shiftKey) && (!s.alt || e.altKey)) {
      e.preventDefault();
      s.action();
      return;
    }
  }
}

export function getShortcutHint(key: string, shift = false): string {
  const modKey = isMac ? '⌘' : 'Ctrl';
  const parts = [modKey];
  if (shift) parts.push('⇧');
  parts.push(key.toUpperCase());
  return parts.join('');
}

export { shortcuts };

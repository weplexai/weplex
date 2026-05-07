import type { SplitNode, SplitDirection } from '../types';
import { durableSave } from '../utils/durablePersist';
import {
  createLeaf,
  splitPane,
  closePane,
  updateRatio,
  getAllLeafIds,
  getAllSessionIds,
  findLeafBySessionId,
  findNode,
  replaceSessionInLeaf,
} from '../utils/splitTree';
import { sessionStore } from './sessionStore';
import { spaceStore } from './spaceStore';

const STORAGE_KEY = 'weplex_splits';
const MAX_PANES = 16;

// Validate that a deserialized node has correct shape (pure — no mutation)
function isValidNode(node: unknown, depth = 0): node is SplitNode {
  if (depth > 20) return false;
  if (!node || typeof node !== 'object') return false;
  const n = node as Record<string, unknown>;
  if (n.type === 'leaf') {
    return (
      typeof n.id === 'string' &&
      typeof n.sessionId === 'number' &&
      Number.isInteger(n.sessionId) &&
      (n.sessionId as number) > 0
    );
  }
  if (n.type === 'branch') {
    if (typeof n.id !== 'string') return false;
    if (typeof n.ratio !== 'number' || !isFinite(n.ratio)) return false;
    if (n.direction !== 'horizontal' && n.direction !== 'vertical') return false;
    if (!Array.isArray(n.children) || n.children.length !== 2) return false;
    return isValidNode(n.children[0], depth + 1) && isValidNode(n.children[1], depth + 1);
  }
  return false;
}

// Clamp ratios after validation (separate from validation — no mutation in validator)
function sanitizeNode(node: SplitNode): SplitNode {
  if (node.type === 'leaf') return node;
  const ratio = Math.max(0.15, Math.min(0.85, node.ratio));
  return {
    ...node,
    ratio,
    children: [sanitizeNode(node.children[0]), sanitizeNode(node.children[1])] as [
      SplitNode,
      SplitNode,
    ],
  };
}

function loadLayouts(): Record<string, SplitNode> {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw);
    const result: Record<string, SplitNode> = {};
    for (const [key, value] of Object.entries(parsed)) {
      if (isValidNode(value)) {
        result[key] = sanitizeNode(value as SplitNode);
      }
    }
    return result;
  } catch {
    return {};
  }
}

function loadFocusedPaneIds(): Record<string, string> {
  try {
    const raw = localStorage.getItem(STORAGE_KEY + '_focus');
    if (!raw) return {};
    return JSON.parse(raw);
  } catch {
    return {};
  }
}

function persistLayouts(layouts: Record<string, SplitNode>) {
  try {
    durableSave(STORAGE_KEY, JSON.stringify(layouts));
  } catch {}
}

function persistFocusedPaneIds(ids: Record<string, string>) {
  try {
    durableSave(STORAGE_KEY + '_focus', JSON.stringify(ids));
  } catch {}
}

let layouts = $state<Record<string, SplitNode>>(loadLayouts());
let focusedPaneIds = $state<Record<string, string>>(loadFocusedPaneIds());

function persist() {
  persistLayouts(layouts);
  persistFocusedPaneIds(focusedPaneIds);
}

// Get the focused pane for the currently active space (validated)
function getCurrentFocusedPaneId(): string | null {
  const spaceId = spaceStore.activeSpaceId;
  return getValidFocusedPaneId(spaceId);
}

// Validate that focusedPaneId points to an existing leaf
// IMPORTANT: read-only — does NOT mutate focusedPaneIds (mutations during reads
// cause infinite effect loops in Svelte 5's fine-grained reactivity)
function getValidFocusedPaneId(spaceId: string): string | null {
  const paneId = focusedPaneIds[spaceId] ?? null;
  const layout = layouts[spaceId];
  if (!layout) return null;
  if (paneId) {
    const node = findNode(layout, paneId);
    if (node && node.type === 'leaf') return paneId;
  }
  // Focused pane is stale — return first leaf as fallback (no mutation)
  const leafIds = getAllLeafIds(layout);
  return leafIds[0] ?? null;
}

function setFocusedPane(spaceId: string, paneId: string | null) {
  if (paneId) {
    focusedPaneIds[spaceId] = paneId;
  } else {
    delete focusedPaneIds[spaceId];
  }
}

export const splitStore = {
  get layouts() {
    return layouts;
  },

  // Focused pane for the currently active space (backward-compatible getter)
  get focusedPaneId(): string | null {
    return getCurrentFocusedPaneId();
  },

  // Focused pane for a specific space (validated)
  focusedPaneForSpace(spaceId: string): string | null {
    return getValidFocusedPaneId(spaceId);
  },

  // Read-only: get the split layout for a space (safe for $derived)
  getLayout(spaceId: string): SplitNode | null {
    return layouts[spaceId] ?? null;
  },

  // Create a default single-pane layout if none exists (call from $effect, not $derived)
  ensureLayout(spaceId: string) {
    if (layouts[spaceId]) return;
    // Use a session that belongs to this space — not the global activeSessionId
    // (which may belong to a different space, causing a loop with the reconciliation effect)
    const spaceSessions = sessionStore.getBySpace(spaceId);
    if (spaceSessions.length === 0) return;
    const lastActiveId = spaceStore.getActiveSession(spaceId);
    const target = lastActiveId ? spaceSessions.find((s) => s.id === lastActiveId) : null;
    const sessionId = target ? target.id : spaceSessions[spaceSessions.length - 1].id;
    const leaf = createLeaf(sessionId);
    layouts[spaceId] = leaf;
    setFocusedPane(spaceId, leaf.id);
    persist();
  },

  // Ensure a session is visible in the layout (called when sidebar click activates a session)
  ensureSession(spaceId: string, sessionId: number) {
    const layout = layouts[spaceId];

    if (!layout) {
      // No layout yet — create a single-leaf layout
      const leaf = createLeaf(sessionId);
      layouts[spaceId] = leaf;
      setFocusedPane(spaceId, leaf.id);
      persist();
      return;
    }

    // If session is already in the tree, just focus its pane
    const existing = findLeafBySessionId(layout, sessionId);
    if (existing) {
      setFocusedPane(spaceId, existing.id);
      return;
    }

    // Single pane — just swap its sessionId (immutable)
    if (layout.type === 'leaf') {
      layouts[spaceId] = { ...layout, sessionId };
      setFocusedPane(spaceId, layout.id);
      persist();
      return;
    }

    // Multi-pane: update the focused pane to show this session (immutable)
    const currentFocus = focusedPaneIds[spaceId];
    if (currentFocus) {
      const node = findNode(layout, currentFocus);
      if (node && node.type === 'leaf') {
        layouts[spaceId] = replaceSessionInLeaf(layout, currentFocus, sessionId);
        persist();
      }
    }
  },

  // Split the focused pane in a direction, creating a new session for the new pane
  split(spaceId: string, direction: SplitDirection) {
    const targetPaneId = getValidFocusedPaneId(spaceId);
    if (!targetPaneId) return;

    // Ensure layout exists
    if (!layouts[spaceId]) {
      this.ensureLayout(spaceId);
    }
    if (!layouts[spaceId]) return;

    // Check pane limit
    const currentLeafs = getAllLeafIds(layouts[spaceId]);
    if (currentLeafs.length >= MAX_PANES) return;

    // Verify target pane actually exists in the tree before creating a session
    const targetNode = findNode(layouts[spaceId], targetPaneId);
    if (!targetNode || targetNode.type !== 'leaf') return;

    // Inherit cwd from the source pane's session
    const sourceSession = sessionStore.sessions.find((s) => s.id === targetNode.sessionId);

    // Create a new terminal session for the new pane
    const newSession = sessionStore.create({
      name: 'terminal',
      spaceId,
      cwd: sourceSession?.cwd,
    });

    const { tree, newLeafId } = splitPane(layouts[spaceId], targetPaneId, direction, newSession.id);
    layouts[spaceId] = tree;

    // Focus the new pane
    setFocusedPane(spaceId, newLeafId);
    persist();
  },

  // Close the focused pane (and kill its session)
  closeFocusedPane(spaceId: string) {
    const currentFocus = getValidFocusedPaneId(spaceId);
    if (!currentFocus || !layouts[spaceId]) return;
    const allIds = getAllLeafIds(layouts[spaceId]);
    if (allIds.length <= 1) return; // Don't close the last pane

    // Find the session in the pane being closed
    const node = findNode(layouts[spaceId], currentFocus);
    if (node && node.type === 'leaf') {
      sessionStore.kill(node.sessionId);
    }

    const result = closePane(layouts[spaceId], currentFocus);
    if (result) {
      layouts[spaceId] = result;
      // Focus next available pane
      const remaining = getAllLeafIds(result);
      const newFocus = remaining[0] || null;
      setFocusedPane(spaceId, newFocus);

      // Activate the session in the newly focused pane
      if (newFocus) {
        const focusedNode = findNode(result, newFocus);
        if (focusedNode && focusedNode.type === 'leaf') {
          sessionStore.activate(focusedNode.sessionId);
        }
      }
    }
    persist();
  },

  // Set focus to a specific pane (infers space from active space)
  focusPane(paneId: string) {
    const spaceId = spaceStore.activeSpaceId;
    setFocusedPane(spaceId, paneId);
  },

  // Cycle focus to the next pane
  focusNext(spaceId: string) {
    if (!layouts[spaceId]) return;
    const ids = getAllLeafIds(layouts[spaceId]);
    if (ids.length <= 1) return;
    const currentFocus = getValidFocusedPaneId(spaceId) ?? '';
    const idx = ids.indexOf(currentFocus);
    const nextIdx = (idx + 1) % ids.length;
    setFocusedPane(spaceId, ids[nextIdx]);

    // Activate the session in the focused pane
    const node = findNode(layouts[spaceId], ids[nextIdx]);
    if (node && node.type === 'leaf') {
      sessionStore.activate(node.sessionId);
    }
  },

  // Cycle focus to the previous pane
  focusPrev(spaceId: string) {
    if (!layouts[spaceId]) return;
    const ids = getAllLeafIds(layouts[spaceId]);
    if (ids.length <= 1) return;
    const currentFocus = getValidFocusedPaneId(spaceId) ?? '';
    const idx = ids.indexOf(currentFocus);
    const prevIdx = (idx - 1 + ids.length) % ids.length;
    setFocusedPane(spaceId, ids[prevIdx]);

    const node = findNode(layouts[spaceId], ids[prevIdx]);
    if (node && node.type === 'leaf') {
      sessionStore.activate(node.sessionId);
    }
  },

  // Update the split ratio of a branch (state only — no persist, use persistLayout for that)
  setRatio(spaceId: string, branchId: string, ratio: number) {
    if (!layouts[spaceId]) return;
    layouts[spaceId] = updateRatio(layouts[spaceId], branchId, ratio);
  },

  // Persist the current layout (call on resize end, not every pointermove)
  persistLayout(spaceId: string) {
    persist();
  },

  // Remove a specific session from the layout (e.g. when killed from sidebar)
  removeSession(spaceId: string, sessionId: number) {
    if (!layouts[spaceId]) return;
    const leaf = findLeafBySessionId(layouts[spaceId], sessionId);
    if (!leaf) return;

    const allIds = getAllLeafIds(layouts[spaceId]);
    if (allIds.length <= 1) {
      // Last pane — remove the layout entirely
      delete layouts[spaceId];
      setFocusedPane(spaceId, null);
      persist();
      return;
    }

    const result = closePane(layouts[spaceId], leaf.id);
    if (result) {
      layouts[spaceId] = result;
      if ((focusedPaneIds[spaceId] ?? null) === leaf.id || !getValidFocusedPaneId(spaceId)) {
        const remaining = getAllLeafIds(result);
        setFocusedPane(spaceId, remaining[0] || null);
      }
    }
    persist();
  },

  // Check if a space has multiple panes
  hasSplits(spaceId: string): boolean {
    const layout = layouts[spaceId];
    if (!layout) return false;
    return layout.type === 'branch';
  },

  // Get all visible session IDs for a space
  getVisibleSessionIds(spaceId: string): number[] {
    const layout = layouts[spaceId];
    if (!layout) return [];
    return getAllSessionIds(layout);
  },

  // Split a specific pane with an existing session (for drag-and-drop from sidebar)
  splitWithExistingSession(
    spaceId: string,
    targetPaneId: string,
    direction: SplitDirection,
    sessionId: number,
    position: 'before' | 'after' = 'after',
  ) {
    if (!layouts[spaceId]) return;

    const currentLeafs = getAllLeafIds(layouts[spaceId]);
    if (currentLeafs.length >= MAX_PANES) return;

    const targetNode = findNode(layouts[spaceId], targetPaneId);
    if (!targetNode || targetNode.type !== 'leaf') return;

    // If session is already visible in the tree, skip
    const existingLeaf = findLeafBySessionId(layouts[spaceId], sessionId);
    if (existingLeaf) return;

    const { tree, newLeafId } = splitPane(
      layouts[spaceId],
      targetPaneId,
      direction,
      sessionId,
      position,
    );
    layouts[spaceId] = tree;
    setFocusedPane(spaceId, newLeafId);
    // Dropping a session into a layout makes it visible — wake it now,
    // otherwise its TerminalView mounts but stays gated on `isHibernated`.
    sessionStore.wakeUp(sessionId);
    persist();
  },

  // Replace the session displayed in a specific pane (for center drop)
  replaceSessionInPane(spaceId: string, paneId: string, sessionId: number) {
    if (!layouts[spaceId]) return;

    // If session is already visible, skip
    const existingLeaf = findLeafBySessionId(layouts[spaceId], sessionId);
    if (existingLeaf) return;

    layouts[spaceId] = replaceSessionInLeaf(layouts[spaceId], paneId, sessionId);
    setFocusedPane(spaceId, paneId);
    sessionStore.wakeUp(sessionId);
    persist();
  },

  // Reset layout for a space (single pane with current active session)
  reset(spaceId: string) {
    const activeId = sessionStore.activeSessionId;
    if (activeId !== null) {
      // Immediately recreate as a single leaf (don't wait for $effect)
      const leaf = createLeaf(activeId);
      layouts[spaceId] = leaf;
      setFocusedPane(spaceId, leaf.id);
    } else {
      delete layouts[spaceId];
      setFocusedPane(spaceId, null);
    }
    persist();
  },
};

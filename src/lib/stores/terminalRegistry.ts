// Portal registry: maps sessionId → terminal wrapper DOM element.
// TerminalView registers on mount; SplitContainer moves elements into pane slots.
// This avoids destroying/recreating xterm.js instances on split changes.

const registry = new Map<number, HTMLElement>();
// Refresh callbacks let SplitContainer ask a terminal to redraw after reparent.
// xterm Canvas does not auto-redraw on detach/reattach when slot size is unchanged.
const refreshCallbacks = new Map<number, () => void>();

export const terminalRegistry = {
  register(sessionId: number, el: HTMLElement) {
    registry.set(sessionId, el);
  },

  unregister(sessionId: number) {
    registry.delete(sessionId);
    refreshCallbacks.delete(sessionId);
  },

  get(sessionId: number): HTMLElement | undefined {
    return registry.get(sessionId);
  },

  setRefresh(sessionId: number, fn: () => void) {
    refreshCallbacks.set(sessionId, fn);
  },

  refresh(sessionId: number) {
    refreshCallbacks.get(sessionId)?.();
  },
};

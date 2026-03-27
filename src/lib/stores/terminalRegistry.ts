// Portal registry: maps sessionId → terminal wrapper DOM element.
// TerminalView registers on mount; SplitContainer moves elements into pane slots.
// This avoids destroying/recreating xterm.js instances on split changes.

const registry = new Map<number, HTMLElement>();

export const terminalRegistry = {
  register(sessionId: number, el: HTMLElement) {
    registry.set(sessionId, el);
  },

  unregister(sessionId: number) {
    registry.delete(sessionId);
  },

  get(sessionId: number): HTMLElement | undefined {
    return registry.get(sessionId);
  },
};

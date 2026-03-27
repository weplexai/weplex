// Pipeline injection store — passes instructions from AgentsPipelines to TerminalView
// without using window globals.

interface PipelineInject {
  sessionId: number;
  instructions: string;
}

let pending = $state<PipelineInject | null>(null);

export const pipelineInjectStore = {
  get pending() {
    return pending;
  },

  set(sessionId: number, instructions: string) {
    pending = { sessionId, instructions };
  },

  consume(sessionId: number): string | null {
    if (pending && pending.sessionId === sessionId) {
      const instructions = pending.instructions;
      pending = null;
      return instructions;
    }
    return null;
  },

  clear() {
    pending = null;
  },
};

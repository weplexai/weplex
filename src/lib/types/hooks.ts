export type HookEventType = 'pre_tool_use' | 'post_tool_use' | 'stop' | 'subagent_start' | 'subagent_stop' | 'session_start';

export interface HookEventPayload {
  event_type: HookEventType;
  session_id: number;
  tool_name?: string;
  file_path?: string;
  cwd?: string;
  tool_input?: string;
  tool_output?: string;
  agent_type?: string;
  agent_id?: string;
  claude_session_id?: string;
  timestamp: number;
}

export interface SessionActivity {
  toolUses: ToolUseEntry[];
  filesTouched: string[];
  totalToolCalls: number;
}

export interface ToolUseEntry {
  toolName: string;
  filePath?: string;
  timestamp: number;
  type: 'pre' | 'post';
}

export interface SubAgent {
  agentId: string;
  agentType: string;
  sessionId: number;
  startedAt: number;
  finishedAt?: number;
  status: 'running' | 'completed' | 'unknown';
}

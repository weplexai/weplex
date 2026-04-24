export type SessionType = 'terminal' | 'agent' | 'ssh' | 'dashboard' | 'spectator' | 'plugin';
export type AgentType = 'claude' | 'opencode' | 'aider' | 'gemini' | 'codex';
export type SessionStatus = 'active' | 'thinking' | 'waiting' | 'idle' | 'error' | 'new' | 'disconnected';

export interface Session {
  id: number;
  name: string;
  type: SessionType;
  status: SessionStatus;
  agentType?: AgentType;
  spaceId: string;
  profileId?: string;
  folderId?: string;
  pinned: boolean;
  order: number;
  createdAt: number;
  lastActivity: number;
  icon?: string;
  command?: string;
  cwd?: string;
  hasOutput?: boolean;
  previousStatus?: SessionStatus;
  lastError?: string;
  claudeSessionId?: string;
  model?: string;
  authType?: string;
  tokensIn?: number;
  tokensOut?: number;
  cacheReadTokens?: number;
  cacheWriteTokens?: number;
  turns?: number;
  cost?: number;
  branch?: string;
  gitFiles?: GitFileChange[];
  host?: string;
  sshUser?: string;
  pid?: number;
  exitCode?: number;
  toolCalls?: number;
  sshPort?: number;
  sshKeyPath?: string;
  tags?: string[];
  extraEnvVars?: Record<string, string>;
  parentId?: number;
  childCollapsed?: boolean;
  dashboardType?: 'orchestration' | 'project' | 'space';
  orchestratorId?: number;
  spectatorCount?: number;
  spectateSpaceId?: string;
  spectateSessionName?: string;
  spectateOwnerName?: string;
  pluginId?: string;
}

export interface GitFileChange {
  path: string;
  status: 'M' | 'A' | 'D' | 'R';
}

export interface Note {
  id: string;
  content: string;
  key: string;
  keyType: 'cwd' | 'ssh';
  createdAt: number;
  updatedAt: number;
}

export interface NoteEntry {
  text: string;
  filesChanged?: string[];
  decisions?: string[];
  at: number;
}

export interface WeplexAgent {
  name: string;
  description: string;
  binary: string;
  model: string;
  prompt: string;
  one_shot: string;
  env: Record<string, string>;
  file_path: string;
}

export const HYPERSPACE_ID = '__hyperspace__';
export type HyperspaceGroupBy = 'space' | 'status' | 'project';

export const AGENT_ICONS: Record<AgentType, string> = {
  claude: '⚡',
  opencode: '⟨⟩',
  aider: '✎',
  gemini: '✦',
  codex: '◎',
};

export const SESSION_TYPE_ICONS: Record<SessionType, string> = {
  agent: '⚡',
  ssh: '↗',
  terminal: '>_',
  dashboard: '▦',
  spectator: '👁',
  plugin: '🧩',
};

export const STATUS_COLORS: Record<SessionStatus, string> = {
  active: 'var(--weplex-active)',
  thinking: 'var(--weplex-accent)',
  idle: 'var(--weplex-active)',
  waiting: 'var(--weplex-warning)',
  error: 'var(--weplex-error)',
  new: 'var(--weplex-info)',
  disconnected: 'var(--weplex-text-muted)',
};

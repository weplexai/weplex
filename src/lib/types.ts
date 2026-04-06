export type SessionType = 'terminal' | 'agent' | 'ssh';
export type AgentType = 'claude' | 'opencode' | 'aider' | 'gemini' | 'codex';
export type SessionStatus = 'active' | 'waiting' | 'idle' | 'error' | 'new' | 'disconnected';
export type SidebarState = 'expanded' | 'collapsed' | 'overlay';
export type OverlayType =
  | 'none'
  | 'command-palette'
  | 'quick-switcher'
  | 'new-session'
  | 'settings'
  | 'space-modal'
  | 'agents'
  | 'auth'
  | 'uikit';
export type SplitDirection = 'horizontal' | 'vertical';

export interface Session {
  id: number;
  name: string;
  type: SessionType;
  status: SessionStatus;
  agentType?: AgentType;
  spaceId: string;
  profileId?: string; // overrides space profile, undefined = inherit from space
  folderId?: string;
  pinned: boolean;
  order: number;
  createdAt: number;
  lastActivity: number;
  icon?: string;
  command?: string;
  cwd?: string;

  // Tracks if session ever had terminal output (for smart restore)
  hasOutput?: boolean;

  // Status before app restart (used to decide if resume is appropriate)
  previousStatus?: SessionStatus;

  // Agent metadata
  claudeSessionId?: string;
  model?: string;
  authType?: string;
  tokensIn?: number;
  tokensOut?: number;
  cacheReadTokens?: number;
  cacheWriteTokens?: number;
  turns?: number;
  cost?: number;

  // Git
  branch?: string;
  gitFiles?: GitFileChange[];

  // SSH
  host?: string;
  sshUser?: string;

  // Process
  pid?: number;
  exitCode?: number;

  // Agent metadata (extended)
  toolCalls?: number;

  // SSH (extended)
  sshPort?: number;
  sshKeyPath?: string;

  // User annotations
  tags?: string[];

  // Extra environment variables (e.g. MCP socket path for pipeline stages)
  extraEnvVars?: Record<string, string>;
}

export interface Note {
  id: string;
  content: string;
  key: string; // cwd path or "user@host"
  keyType: 'cwd' | 'ssh';
  createdAt: number;
  updatedAt: number;
}

export interface NoteEntry {
  text: string;
  filesChanged?: string[];
  decisions?: string[];
  at: number; // unix timestamp
}

export interface GitFileChange {
  path: string;
  status: 'M' | 'A' | 'D' | 'R';
}

export interface Folder {
  id: string;
  name: string;
  spaceId: string;
  order: number;
  collapsed: boolean;
}

export type SpaceType = 'personal' | 'team';

export interface Space {
  id: string;
  name: string;
  color: string;
  order: number;
  profileId?: string; // references Profile.id, undefined = default profile
  bgColor?: string; // background tint color for the space chrome
  directory?: string; // default working directory for new sessions in this space
  type: SpaceType; // default: 'personal'
  shared: boolean; // default: false — visible to team members
  teamId?: string; // set for shared/team spaces
  serverId?: string; // server UUID (null for private local spaces)
  createdBy?: string; // userId who created
}

export interface Profile {
  id: string;
  name: string;
  isDefault: boolean;
  configDir: string | null; // null = system default (~/.claude/)
  envVars: Record<string, string>;
  linkedAccount?: {
    email?: string;
    plan?: string; // "Pro", "Max", "API Key"
  };
}

export interface DiscoveredProfile {
  path: string;
  name: string;
  source: 'filesystem' | 'shell_config';
}

export interface AppSettings {
  defaultShell: string;
  defaultDirectory: string;
  fontFamily: string;
  fontSize: number;
  theme: 'dark' | 'light';
  sidebarDefault: SidebarState;
  idleTimeout: number;
  persistSessions: boolean;
  chatSoundEnabled: boolean;
  chatNotificationsEnabled: boolean;
}

export interface SplitLeaf {
  type: 'leaf';
  id: string;
  sessionId: number;
}

export interface SplitBranch {
  type: 'branch';
  id: string;
  direction: SplitDirection;
  ratio: number;
  children: [SplitNode, SplitNode];
}

export type SplitNode = SplitLeaf | SplitBranch;

export const SPACE_COLORS = [
  '#8B5CF6',
  '#3B82F6',
  '#10B981',
  '#F59E0B',
  '#EF4444',
  '#EC4899',
  '#06B6D4',
  '#F97316',
  '#84CC16',
  '#A855F7',
];

// Vibrant source colors for space backgrounds (displayed in picker as-is)
// Applied to chrome via color-mix to produce dark tints
export const SPACE_BG_COLORS = [
  // Saturated
  '#7C3AED', // purple
  '#2563EB', // blue
  '#0D9488', // teal
  '#16A34A', // green
  '#D97706', // amber
  '#DC2626', // red
  '#DB2777', // pink
  '#9333EA', // violet
  '#0891B2', // cyan
  '#EA580C', // orange
  '#65A30D', // lime
  '#4F46E5', // indigo
  // Pastel / light
  '#A78BFA', // lavender
  '#60A5FA', // sky
  '#5EEAD4', // mint
  '#86EFAC', // light green
  '#FCD34D', // gold
  '#FCA5A5', // salmon
  '#F9A8D4', // rose
  '#C4B5FD', // soft violet
  '#67E8F9', // aqua
  '#FDBA74', // peach
  '#BEF264', // chartreuse
  '#A5B4FC', // periwinkle
];

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
};

// ── Pipeline Run types ──────────────────────────────────────────────────────

export type PipelineRunStatus = 'pending' | 'running' | 'completed' | 'failed' | 'cancelled';
export type StageStatus = 'pending' | 'running' | 'completed' | 'failed' | 'skipped';

export interface StageRunInfo {
  name: string;
  agent: string;
  state: {
    status: StageStatus;
    artifact?: string;
    output?: string;
    exit_code?: number;
    duration_ms?: number;
  };
  parallel_group: StageRunInfo[] | null;
}

export interface PipelineRunInfo {
  id: string;
  pipeline_name: string;
  pipeline_file: string;
  task: string;
  cwd: string;
  /** Profile used for this run — all stages share the same profile. */
  profile_name: string;
  status: PipelineRunStatus;
  stages: StageRunInfo[];
  started_at: number | null;
  finished_at: number | null;
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

export const STATUS_COLORS: Record<SessionStatus, string> = {
  active: 'var(--weplex-active)', // green pulsing  — executing a task
  idle: 'var(--weplex-active)', // green solid    — done, ready for next prompt
  waiting: 'var(--weplex-warning)', // yellow         — needs user action (question/menu/permission)
  error: 'var(--weplex-error)', // red            — PTY failed
  new: 'var(--weplex-info)', // blue           — just created
  disconnected: 'var(--weplex-text-muted)', // dark gray
};

// ── Auth types ─────────────────────────────────────────────────────────────

export interface AuthUser {
  id: string;
  email: string;
  displayName: string | null;
  plan: string;
  oauthProvider: string | null;
  emailVerified: boolean;
}

export interface AuthTokens {
  accessToken: string;
  refreshToken: string;
}

export interface AuthResponse {
  accessToken: string;
  refreshToken: string;
  user: AuthUser;
}

export type SyncStatus = 'idle' | 'pulling' | 'pushing' | 'error';

// ── Collaborative Pipeline Types ──────────────────────────────────────────

export type CollaborativeRunStatus = 'pending' | 'running' | 'completed' | 'failed' | 'cancelled';
export type CollaborativeStageStatus =
  | 'pending'
  | 'waiting'
  | 'running'
  | 'completed'
  | 'failed'
  | 'skipped';

export interface CollaborativeRun {
  id: string;
  teamId: string;
  initiatorId: string;
  pipelineName: string;
  task: string;
  status: CollaborativeRunStatus;
  stages: CollaborativeStageInfo[];
  artifacts: Record<string, string>;
  createdAt: string;
  updatedAt: string;
  finishedAt: string | null;
}

export interface CollaborativeStageInfo {
  name: string;
  agent: string;
  role: string;
  receives: string[];
  optional: boolean;
  ownerId: string | null;
  ownerEmail: string | null;
  status: CollaborativeStageStatus;
  startedAt: string | null;
  finishedAt: string | null;
}

export interface TeamInfo {
  id: string;
  name: string;
  inviteCode: string;
  inviteCodeExpiresAt?: string;
  ownerId: string;
  members: TeamMember[];
}

export interface TeamMember {
  id: string;           // membership ID
  userId: string;       // actual user ID
  email: string;
  displayName: string | null;
  role: 'owner' | 'member';
}

export interface CreateRunPayload {
  teamId: string;
  pipelineName: string;
  task: string;
  stages: StageDefinitionPayload[];
}

export interface StageDefinitionPayload {
  name: string;
  agent: string;
  role: string;
  receives: string[];
  optional?: boolean;
  ownerEmail?: string;
}

export interface CreateRunResponse {
  run: CollaborativeRun;
  warnings: string[];
}

// ── Space Sharing / Presence types ────────────────────────────────────────

export interface SessionMeta {
  id: string;
  name: string;
  status: 'active' | 'idle' | 'closed';
  agentType?: string;
  cwd?: string;
  gitBranch?: string;
  shared: boolean;
  createdAt: string;
  updatedAt: string;
  summary?: string;
  filesChanged?: string[];
  decisions?: string[];
  notes?: NoteEntry[];
}

export interface MemberPresence {
  userId: string;
  displayName: string;
  sessions: SessionMeta[];
}

export interface ServerSpace {
  id: string;
  teamId: string;
  name: string;
  color: string;
  type: SpaceType;
  shared: boolean;
  createdBy: string;
  createdAt: string;
  updatedAt: string;
}

export interface PipelineNotification {
  type: 'stage-ready' | 'run-completed' | 'run-failed';
  title: string;
  body: string;
  runId: string;
  stageName?: string;
}

// ── Chat ─────────────────────────────────────────────────────────────────

export interface ChatMessage {
  id: string;
  spaceId: string;
  userId: string;
  userEmail: string;
  displayName: string;
  text: string;
  createdAt: string;
  replyToId?: string;
  replyTo?: { text: string; displayName: string };
  editedAt?: string;
}

// ── Session History (server-persisted records for shared spaces) ──────────

export interface SessionRecord {
  id: string;
  spaceId: string;
  userId: string;
  sessionLocalId: string;
  name: string;
  agentType?: string;
  cwd?: string;
  gitBranch?: string;
  status: 'active' | 'idle' | 'closed';
  startedAt: string;
  lastSeenAt: string;
  endedAt?: string;
  summary?: string | null;
  filesChanged?: string[] | null;
  decisions?: string[] | null;
  user?: {
    email: string;
    displayName: string | null;
  };
}

// ── Hook Events ──────────────────────────────────────────────────────────────

export type HookEventType = 'pre_tool_use' | 'post_tool_use' | 'stop' | 'subagent_start' | 'subagent_stop';

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
  timestamp: number;
}

/** Accumulated tool activity for a session. */
export interface SessionActivity {
  /** Recent tool uses (last 50). */
  toolUses: ToolUseEntry[];
  /** Files touched by this session (Set-like, deduped). */
  filesTouched: string[];
  /** Total tool call count. */
  totalToolCalls: number;
}

export interface ToolUseEntry {
  toolName: string;
  filePath?: string;
  timestamp: number;
  type: 'pre' | 'post';
}

/** A sub-agent spawned by Claude during a session. */
export interface SubAgent {
  agentId: string;
  agentType: string;
  sessionId: number;
  startedAt: number;
  finishedAt?: number;
  status: 'running' | 'completed' | 'unknown';
}

import type { SpaceType } from './space';
import type { NoteEntry } from './session';

export interface TeamInfo {
  id: string;
  name: string;
  inviteCode: string;
  inviteCodeExpiresAt?: string;
  ownerId: string;
  members: TeamMember[];
}

export interface TeamMember {
  id: string;
  userId: string;
  email: string;
  displayName: string | null;
  role: 'owner' | 'member';
}

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

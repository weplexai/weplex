import type { SpaceType } from './space';

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

/** Identity-only metadata about a session, safe to share between teammates.
 *  Activity content (notes/summaries) is NEVER pushed — it's private to the
 *  machine that wrote it, and surfaced only via explicit share actions. */
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
}

// ── Compile-time guard: re-introducing activity content into SessionMeta
//    reopens the surveillance path we deliberately closed. If you find
//    yourself adding `summary` / `filesChanged` / `decisions` / `notes`
//    here to make a feature "easier", don't — route explicit share through
//    SpaceChat.shareSessionContext instead. This line will fail the build
//    if any of those keys reappear.
type _SessionMetaForbiddenKeys = 'summary' | 'filesChanged' | 'decisions' | 'notes';
type _SessionMetaNoSurveillanceFields = Extract<
  keyof SessionMeta,
  _SessionMetaForbiddenKeys
> extends never
  ? true
  : false;
const _sessionMetaGuard: _SessionMetaNoSurveillanceFields = true;
void _sessionMetaGuard;

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

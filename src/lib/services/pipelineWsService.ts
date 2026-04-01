// WebSocket service for real-time pipeline collaboration updates

import { io, type Socket } from 'socket.io-client';
import { getBaseUrl } from './apiClient';
import type { CollaborativeRun, PipelineNotification, SessionMeta, MemberPresence, TeamMember, ServerSpace, ChatMessage } from '../types';

let socket: Socket | null = null;

// Track joined rooms for re-joining on reconnect
const joinedRuns: Set<string> = new Set();
const joinedSpaces: Map<string, string | undefined> = new Map();
const joinedTeams: Set<string> = new Set();

export const pipelineWsService = {
  /** Connect to the /pipeline namespace with bearer auth. */
  connect(token: string): void {
    if (socket?.connected) return;

    // Derive WS URL from API base (same host, /pipeline namespace)
    const baseUrl = getBaseUrl();
    socket = io(`${baseUrl}/pipeline`, {
      transports: ['websocket'],
      auth: { token },
      reconnection: true,
      reconnectionAttempts: 10,
      reconnectionDelay: 1000,
      reconnectionDelayMax: 10000,
    });

    socket.on('connect', () => {
      console.log('[Weplex] Pipeline WS connected');
      // Re-join all rooms on reconnect
      for (const runId of joinedRuns) {
        socket?.emit('join-run', { runId });
      }
      for (const [spaceId, displayName] of joinedSpaces) {
        socket?.emit('join-space', { spaceId, displayName });
      }
      for (const teamId of joinedTeams) {
        socket?.emit('join-team-room', { teamId });
      }
    });

    socket.on('disconnect', (reason) => {
      console.log(`[Weplex] Pipeline WS disconnected: ${reason}`);
    });

    socket.on('connect_error', (err) => {
      console.warn('[Weplex] Pipeline WS connection error:', err.message);
    });
  },

  /** Disconnect and clean up. */
  disconnect(): void {
    if (!socket) return;
    socket.removeAllListeners();
    socket.disconnect();
    socket = null;
    joinedRuns.clear();
    joinedSpaces.clear();
    joinedTeams.clear();
  },

  /** Join a run room to receive updates. */
  joinRun(runId: string): void {
    joinedRuns.add(runId);
    socket?.emit('join-run', { runId });
  },

  /** Leave a run room. */
  leaveRun(runId: string): void {
    joinedRuns.delete(runId);
    socket?.emit('leave-run', { runId });
  },

  /** Subscribe to run updates. Returns an unsubscribe function. */
  onRunUpdated(cb: (run: CollaborativeRun) => void): () => void {
    if (!socket) return () => {};
    socket.on('run-updated', cb);
    return () => {
      socket?.off('run-updated', cb);
    };
  },

  /** Subscribe to stage-ready notifications. Returns an unsubscribe function. */
  onStageReady(
    cb: (data: { runId: string; stageName: string; ownerEmail: string }) => void,
  ): () => void {
    if (!socket) return () => {};
    socket.on('stage-ready', cb);
    return () => {
      socket?.off('stage-ready', cb);
    };
  },

  /** Subscribe to pipeline notifications. Returns an unsubscribe function. */
  onNotification(cb: (data: PipelineNotification) => void): () => void {
    if (!socket) return () => {};
    socket.on('notification', cb);
    return () => {
      socket?.off('notification', cb);
    };
  },

  // ── Space Presence ───────────────────────────────────────────────────────

  /** Join a space room to receive presence updates. */
  joinSpace(spaceId: string, displayName?: string): void {
    joinedSpaces.set(spaceId, displayName);
    socket?.emit('join-space', { spaceId, displayName });
  },

  /** Leave a space room. */
  leaveSpace(spaceId: string): void {
    joinedSpaces.delete(spaceId);
    socket?.emit('leave-space', { spaceId });
  },

  /** Sync local sessions to the space for presence broadcasting. */
  syncSessions(spaceId: string, sessions: SessionMeta[]): void {
    socket?.emit('session-sync', { spaceId, sessions });
  },

  /** Subscribe to space presence updates. Returns an unsubscribe function. */
  onSpaceSessions(
    cb: (data: { spaceId: string; members: MemberPresence[] }) => void,
  ): () => void {
    if (!socket) return () => {};
    socket.on('space-sessions', cb);
    return () => {
      socket?.off('space-sessions', cb);
    };
  },

  /** Subscribe to member-offline events. Returns an unsubscribe function. */
  onMemberOffline(
    cb: (data: { spaceId: string; userId: string }) => void,
  ): () => void {
    if (!socket) return () => {};
    socket.on('member-offline', cb);
    return () => {
      socket?.off('member-offline', cb);
    };
  },

  // ── Chat Events ───────────────────────────────────────────────────────

  /** Send a chat message to a space. */
  sendChatMessage(spaceId: string, text: string): void {
    socket?.emit('chat-message', { spaceId, text });
  },

  /** Subscribe to incoming chat messages. Returns an unsubscribe function. */
  onChatMessage(cb: (msg: ChatMessage) => void): () => void {
    if (!socket) return () => {};
    socket.on('chat-message', cb);
    return () => {
      socket?.off('chat-message', cb);
    };
  },

  /** Subscribe to chat history (sent on join-space). Returns an unsubscribe function. */
  onChatHistory(
    cb: (data: { spaceId: string; messages: ChatMessage[] }) => void,
  ): () => void {
    if (!socket) return () => {};
    socket.on('chat-history', cb);
    return () => {
      socket?.off('chat-history', cb);
    };
  },

  // ── Team Events ────────────────────────────────────────────────────────

  /** Join a team room to receive team/space events. */
  joinTeamRoom(teamId: string): void {
    joinedTeams.add(teamId);
    socket?.emit('join-team-room', { teamId });
  },

  /** Leave a team room. */
  leaveTeamRoom(teamId: string): void {
    joinedTeams.delete(teamId);
    socket?.emit('leave-team-room', { teamId });
  },

  /** Subscribe to team member joined events. Returns an unsubscribe function. */
  onTeamMemberJoined(
    cb: (data: { teamId: string; member: TeamMember }) => void,
  ): () => void {
    if (!socket) return () => {};
    socket.on('team-member-joined', cb);
    return () => {
      socket?.off('team-member-joined', cb);
    };
  },

  /** Subscribe to team member left events. Returns an unsubscribe function. */
  onTeamMemberLeft(
    cb: (data: { teamId: string; userId: string }) => void,
  ): () => void {
    if (!socket) return () => {};
    socket.on('team-member-left', cb);
    return () => {
      socket?.off('team-member-left', cb);
    };
  },

  /** Subscribe to team updated events. Returns an unsubscribe function. */
  onTeamUpdated(
    cb: (data: { teamId: string; name?: string; inviteCode?: string; inviteCodeExpiresAt?: string; ownerId?: string }) => void,
  ): () => void {
    if (!socket) return () => {};
    socket.on('team-updated', cb);
    return () => {
      socket?.off('team-updated', cb);
    };
  },

  /** Subscribe to team deleted events. Returns an unsubscribe function. */
  onTeamDeleted(
    cb: (data: { teamId: string }) => void,
  ): () => void {
    if (!socket) return () => {};
    socket.on('team-deleted', cb);
    return () => {
      socket?.off('team-deleted', cb);
    };
  },

  // ── Space Events ──────────────────────────────────────────────────────

  /** Subscribe to space created events. Returns an unsubscribe function. */
  onSpaceCreated(
    cb: (data: { teamId: string; space: ServerSpace }) => void,
  ): () => void {
    if (!socket) return () => {};
    socket.on('space-created', cb);
    return () => {
      socket?.off('space-created', cb);
    };
  },

  /** Subscribe to space updated events. Returns an unsubscribe function. */
  onSpaceUpdated(
    cb: (data: { teamId: string; space: ServerSpace }) => void,
  ): () => void {
    if (!socket) return () => {};
    socket.on('space-updated', cb);
    return () => {
      socket?.off('space-updated', cb);
    };
  },

  /** Subscribe to space deleted events. Returns an unsubscribe function. */
  onSpaceDeleted(
    cb: (data: { teamId: string; spaceId: string }) => void,
  ): () => void {
    if (!socket) return () => {};
    socket.on('space-deleted', cb);
    return () => {
      socket?.off('space-deleted', cb);
    };
  },

  /** Check current connection state. */
  isConnected(): boolean {
    return socket?.connected ?? false;
  },
};

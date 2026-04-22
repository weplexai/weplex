// WebSocket service for real-time collaboration (teams, chat, presence, spectating)

import { io, type Socket } from 'socket.io-client';
import { getBaseUrl } from './apiClient';
import { logger } from '../utils/logger';
import type { SessionMeta, MemberPresence, TeamMember, ServerSpace, ChatMessage } from '../types';

let socket: Socket | null = null;

// Track joined rooms for re-joining on reconnect
const joinedSpaces: Map<string, string | undefined> = new Map();
const joinedTeams: Set<string> = new Set();

export const wsService = {
  /** Connect to the /relay namespace with bearer auth. */
  connect(token: string): void {
    if (socket?.connected) return;

    // Derive WS URL from API base (same host, /relay namespace)
    const baseUrl = getBaseUrl();
    socket = io(`${baseUrl}/relay`, {
      transports: ['websocket'],
      auth: { token },
      reconnection: true,
      reconnectionAttempts: 10,
      reconnectionDelay: 1000,
      reconnectionDelayMax: 10000,
    });

    socket.on('connect', () => {
      logger.info('WS connected');
      // Re-join all rooms on reconnect
      for (const [spaceId, displayName] of joinedSpaces) {
        socket?.emit('join-space', { spaceId, displayName });
      }
      for (const teamId of joinedTeams) {
        socket?.emit('join-team-room', { teamId });
      }
    });

    socket.on('disconnect', (reason) => {
      logger.info(`WS disconnected: ${reason}`);
    });

    socket.on('connect_error', (err) => {
      console.warn('[Weplex] WS connection error:', err.message);
    });
  },

  /** Disconnect and clean up. */
  disconnect(): void {
    if (!socket) return;
    socket.removeAllListeners();
    socket.disconnect();
    socket = null;
    joinedSpaces.clear();
    joinedTeams.clear();
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

  /** Send a chat message to a space, optionally replying to another message. */
  sendChatMessage(spaceId: string, text: string, replyToId?: string): void {
    socket?.emit('chat-message', { spaceId, text, replyToId });
  },

  /** Edit a chat message. */
  editChatMessage(messageId: string, text: string): void {
    socket?.emit('chat-edit-message', { messageId, text });
  },

  /** Delete a chat message. */
  deleteChatMessage(messageId: string): void {
    socket?.emit('chat-delete-message', { messageId });
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

  /** Subscribe to chat message edited events. Returns an unsubscribe function. */
  onChatMessageEdited(
    cb: (data: { spaceId: string; messageId: string; text: string; editedAt: string }) => void,
  ): () => void {
    if (!socket) return () => {};
    socket.on('chat-message-edited', cb);
    return () => {
      socket?.off('chat-message-edited', cb);
    };
  },

  /** Subscribe to chat message deleted events. Returns an unsubscribe function. */
  onChatMessageDeleted(
    cb: (data: { spaceId: string; messageId: string }) => void,
  ): () => void {
    if (!socket) return () => {};
    socket.on('chat-message-deleted', cb);
    return () => {
      socket?.off('chat-message-deleted', cb);
    };
  },

  /** Emit a typing indicator to a space. */
  emitTyping(spaceId: string): void {
    socket?.emit('chat-typing', { spaceId });
  },

  /** Subscribe to chat typing indicators. Returns an unsubscribe function. */
  onChatTyping(
    cb: (data: { spaceId: string; userId: string; displayName: string }) => void,
  ): () => void {
    if (!socket) return () => {};
    socket.on('chat-typing', cb);
    return () => {
      socket?.off('chat-typing', cb);
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

  // ── Session Spectating ─────────────────────────────────────────────────

  /** Offer a session for spectating (owner). */
  spectateOffer(spaceId: string, sessionName: string): void {
    socket?.emit('spectate-offer', { spaceId, sessionName });
  },

  /** Stream PTY output chunk to spectators (owner). */
  sendPtyStream(spaceId: string, sessionName: string, chunk: string): void {
    socket?.emit('pty-stream', { spaceId, sessionName, chunk });
  },

  /** Stop offering a session for spectating (owner). */
  spectateStop(spaceId: string, sessionName: string): void {
    socket?.emit('spectate-stop', { spaceId, sessionName });
  },

  /** Join as spectator to watch a session. */
  spectateJoin(spaceId: string, sessionName: string): void {
    socket?.emit('spectate-join', { spaceId, sessionName });
  },

  /** Leave spectating a session. */
  spectateLeave(spaceId: string, sessionName: string): void {
    socket?.emit('spectate-leave', { spaceId, sessionName });
  },

  /** Subscribe to PTY stream chunks (spectator). */
  onPtyStream(cb: (data: { spaceId: string; sessionName: string; chunk: string }) => void): () => void {
    if (!socket) return () => {};
    socket.on('pty-stream', cb);
    return () => { socket?.off('pty-stream', cb); };
  },

  /** Subscribe to scrollback on spectate join. */
  onSpectateScrollback(cb: (data: { spaceId: string; sessionName: string; lines: string[] }) => void): () => void {
    if (!socket) return () => {};
    socket.on('spectate-scrollback', cb);
    return () => { socket?.off('spectate-scrollback', cb); };
  },

  /** Subscribe to spectator count updates (owner). */
  onSpectateCount(cb: (data: { spaceId: string; sessionName: string; count: number }) => void): () => void {
    if (!socket) return () => {};
    socket.on('spectate-count', cb);
    return () => { socket?.off('spectate-count', cb); };
  },

  /** Subscribe to spectate session list (available sessions). */
  onSpectateList(cb: (data: { spaceId: string; sessions: { sessionName: string; ownerName: string }[] }) => void): () => void {
    if (!socket) return () => {};
    socket.on('spectate-list', cb);
    return () => { socket?.off('spectate-list', cb); };
  },

  /** Subscribe to spectate ended (session owner stopped sharing). */
  onSpectateEnded(cb: (data: { spaceId: string; sessionName: string }) => void): () => void {
    if (!socket) return () => {};
    socket.on('spectate-ended', cb);
    return () => { socket?.off('spectate-ended', cb); };
  },

  /** Check current connection state. */
  isConnected(): boolean {
    return socket?.connected ?? false;
  },
};

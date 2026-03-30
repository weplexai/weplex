// WebSocket service for real-time pipeline collaboration updates

import { io, type Socket } from 'socket.io-client';
import { getBaseUrl } from './apiClient';
import type { CollaborativeRun, PipelineNotification, SessionMeta, MemberPresence } from '../types';

let socket: Socket | null = null;

// Track joined rooms for re-joining on reconnect
const joinedRuns: Set<string> = new Set();
const joinedSpaces: Map<string, string | undefined> = new Map();

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

  /** Check current connection state. */
  isConnected(): boolean {
    return socket?.connected ?? false;
  },
};

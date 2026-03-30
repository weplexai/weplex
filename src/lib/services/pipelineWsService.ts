// WebSocket service for real-time pipeline collaboration updates

import { io, type Socket } from 'socket.io-client';
import { getBaseUrl } from './apiClient';
import type { CollaborativeRun, PipelineNotification } from '../types';

let socket: Socket | null = null;

// Track joined rooms for re-joining on reconnect
const joinedRuns: Set<string> = new Set();

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

  /** Check current connection state. */
  isConnected(): boolean {
    return socket?.connected ?? false;
  },
};

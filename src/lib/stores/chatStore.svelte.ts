// Chat store — real-time chat messages per space

import type { ChatMessage } from '../types';
import { pipelineWsService } from '../services/pipelineWsService';
import { spaceService } from '../services/spaceService';
import { authStore } from './authStore.svelte';

// ── State ──────────────────────────────────────────────────────────────────

/** Messages per space (keyed by serverId) */
let messages = $state<Record<string, ChatMessage[]>>({});

/** Unread count per space (keyed by serverId) */
let unreadCounts = $state<Record<string, number>>({});

/** Currently active chat space (serverId) — used to suppress unread when viewing */
let activeSpaceId = $state<string | null>(null);

/** Loading state for older messages */
let loadingOlder = $state<Record<string, boolean>>({});

/** Whether there are more older messages to load */
let hasMore = $state<Record<string, boolean>>({});

let unsubMessage: (() => void) | null = null;
let unsubHistory: (() => void) | null = null;

// ── Store ──────────────────────────────────────────────────────────────────

export const chatStore = {
  get messages() {
    return messages;
  },

  get unreadCounts() {
    return unreadCounts;
  },

  /** Get messages for a specific space. */
  getMessages(serverId: string): ChatMessage[] {
    return messages[serverId] ?? [];
  },

  /** Get unread count for a specific space. */
  getUnread(serverId: string): number {
    return unreadCounts[serverId] ?? 0;
  },

  /** Check if older messages are loading for a space. */
  isLoadingOlder(serverId: string): boolean {
    return loadingOlder[serverId] ?? false;
  },

  /** Check if there are more older messages to load. */
  canLoadMore(serverId: string): boolean {
    return hasMore[serverId] !== false; // default true
  },

  /** Set the active chat space — clears unread for it. */
  setActive(serverId: string | null): void {
    activeSpaceId = serverId;
    if (serverId) {
      unreadCounts = { ...unreadCounts, [serverId]: 0 };
    }
  },

  /** Load older messages (pagination via REST). */
  async loadOlder(serverId: string): Promise<void> {
    if (loadingOlder[serverId]) return;
    const current = messages[serverId] ?? [];
    if (current.length === 0) return;

    const oldestId = current[0].id;
    loadingOlder = { ...loadingOlder, [serverId]: true };
    try {
      const older = await spaceService.getChatMessages(serverId, oldestId);
      if (older.length === 0) {
        hasMore = { ...hasMore, [serverId]: false };
      } else {
        // Re-read current messages after await to include any WS messages that arrived
        const fresh = messages[serverId] ?? [];
        // Deduplicate by id in case WS delivered a message also in REST response
        const existingIds = new Set(fresh.map((m) => m.id));
        const uniqueOlder = older.filter((m) => !existingIds.has(m.id));
        messages = {
          ...messages,
          [serverId]: [...uniqueOlder, ...fresh],
        };
        if (older.length < 50) {
          hasMore = { ...hasMore, [serverId]: false };
        }
      }
    } catch (err) {
      console.warn('[Weplex] Failed to load older chat messages:', err);
    } finally {
      loadingOlder = { ...loadingOlder, [serverId]: false };
    }
  },

  /** Send a chat message. */
  send(serverId: string, text: string): void {
    pipelineWsService.sendChatMessage(serverId, text);
  },

  /** Subscribe to WebSocket chat events. Call once after WS connects. */
  init(): void {
    this.reset();

    unsubHistory = pipelineWsService.onChatHistory((data) => {
      messages = { ...messages, [data.spaceId]: data.messages };
      // Initial history = no unread
      hasMore = { ...hasMore, [data.spaceId]: data.messages.length >= 50 };
    });

    unsubMessage = pipelineWsService.onChatMessage((msg) => {
      const current = messages[msg.spaceId] ?? [];
      messages = {
        ...messages,
        [msg.spaceId]: [...current, msg],
      };

      // Increment unread if not viewing this space's chat (skip own messages)
      if (activeSpaceId !== msg.spaceId && msg.userId !== authStore.user?.id) {
        unreadCounts = {
          ...unreadCounts,
          [msg.spaceId]: (unreadCounts[msg.spaceId] ?? 0) + 1,
        };
      }
    });
  },

  /** Clean up all state and subscriptions. */
  reset(): void {
    if (unsubMessage) {
      unsubMessage();
      unsubMessage = null;
    }
    if (unsubHistory) {
      unsubHistory();
      unsubHistory = null;
    }
    messages = {};
    unreadCounts = {};
    activeSpaceId = null;
    loadingOlder = {};
    hasMore = {};
  },
};

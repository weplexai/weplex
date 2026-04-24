// Chat store — real-time chat messages per space

import type { ChatMessage } from '../types';
import { wsService } from '../services/wsService';
import { spaceService } from '../services/spaceService';
import { authStore } from './authStore.svelte';
import { settingsStore } from './settingsStore.svelte';
import { showNativeNotification } from '../utils/notifications';
import { capture } from '../services/analytics';

// Track which spaces we've already reported a first-message for,
// so chat analytics is aggregate (once per space per app session)
// rather than one event per message.
const firstMessageSent = new Set<string>();

// ── Notification helpers ──────────────────────────────────────────────────

/** Shared AudioContext — browsers limit to ~6 instances per document. */
let sharedAudioCtx: AudioContext | null = null;

/** Play a short subtle pop sound using Web Audio API. */
function playNotificationSound(): void {
  try {
    if (!sharedAudioCtx) sharedAudioCtx = new AudioContext();
    const ctx = sharedAudioCtx;
    if (ctx.state === 'suspended') ctx.resume();
    const osc = ctx.createOscillator();
    const gain = ctx.createGain();
    osc.connect(gain);
    gain.connect(ctx.destination);
    osc.frequency.value = 800;
    osc.type = 'sine';
    gain.gain.value = 0.1;
    gain.gain.exponentialRampToValueAtTime(0.001, ctx.currentTime + 0.15);
    osc.start();
    osc.stop(ctx.currentTime + 0.15);
  } catch {
    // AudioContext may not be available or may require user interaction
  }
}

/** Notify user about an incoming chat message (sound + OS notification). */
function notifyIncomingMessage(msg: ChatMessage): void {
  if (settingsStore.settings.chatSoundEnabled) {
    playNotificationSound();
  }

  if (settingsStore.settings.chatNotificationsEnabled && !document.hasFocus()) {
    const title = msg.displayName ?? 'New message';
    const body = msg.text.length > 100 ? msg.text.slice(0, 100) + '…' : msg.text;
    showNativeNotification(title, body);
  }
}

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

/** Who is currently typing, per space. Auto-cleared after 3s. */
let typingUsers = $state<Record<string, { userId: string; displayName: string; timeout: ReturnType<typeof setTimeout> }[]>>({});
let typingDebounce: ReturnType<typeof setTimeout> | null = null;

/** Reply/Edit state */
let replyingTo = $state<ChatMessage | null>(null);
let editingMessage = $state<ChatMessage | null>(null);

let unsubMessage: (() => void) | null = null;
let unsubHistory: (() => void) | null = null;
let unsubTyping: (() => void) | null = null;
let unsubEdited: (() => void) | null = null;
let unsubDeleted: (() => void) | null = null;

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
    if (serverId && (unreadCounts[serverId] ?? 0) > 0) {
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

  /** Get the message being replied to. */
  get replyTo(): ChatMessage | null {
    return replyingTo;
  },

  /** Set the message to reply to (or null to cancel). */
  setReplyTo(msg: ChatMessage | null): void {
    replyingTo = msg;
  },

  /** Get the message being edited. */
  get editing(): ChatMessage | null {
    return editingMessage;
  },

  /** Set a message to edit (or null to cancel). */
  setEditing(msg: ChatMessage | null): void {
    editingMessage = msg;
  },

  /** Edit a message's text. */
  edit(messageId: string, text: string): void {
    wsService.editChatMessage(messageId, text);
    editingMessage = null;
  },

  /** Delete a message. */
  deleteMsg(messageId: string): void {
    wsService.deleteChatMessage(messageId);
  },

  /** Send a chat message (with optional reply). */
  send(serverId: string, text: string): void {
    wsService.sendChatMessage(serverId, text, replyingTo?.id);
    replyingTo = null;
    if (!firstMessageSent.has(serverId)) {
      firstMessageSent.add(serverId);
      capture('chat_first_message', { has_reply: !!replyingTo });
    }
  },

  /** Subscribe to WebSocket chat events. Call once after WS connects. */
  init(): void {
    this.reset();

    unsubHistory = wsService.onChatHistory((data) => {
      messages = { ...messages, [data.spaceId]: data.messages };
      // Initial history = no unread
      hasMore = { ...hasMore, [data.spaceId]: data.messages.length >= 50 };
    });

    unsubTyping = wsService.onChatTyping((data) => {
      const current = typingUsers[data.spaceId] ?? [];
      // Remove existing entry for this user (reset timer)
      const filtered = current.filter((t) => {
        if (t.userId === data.userId) {
          clearTimeout(t.timeout);
          return false;
        }
        return true;
      });
      // Auto-remove after 3s
      const timeout = setTimeout(() => {
        typingUsers = {
          ...typingUsers,
          [data.spaceId]: (typingUsers[data.spaceId] ?? []).filter((t) => t.userId !== data.userId),
        };
      }, 3000);
      typingUsers = {
        ...typingUsers,
        [data.spaceId]: [...filtered, { userId: data.userId, displayName: data.displayName, timeout }],
      };
    });

    unsubMessage = wsService.onChatMessage((msg) => {
      const current = messages[msg.spaceId] ?? [];
      messages = {
        ...messages,
        [msg.spaceId]: [...current, msg],
      };

      // Increment unread and notify if not viewing this space's chat (skip own messages)
      if (activeSpaceId !== msg.spaceId && msg.userId !== authStore.user?.id) {
        unreadCounts = {
          ...unreadCounts,
          [msg.spaceId]: (unreadCounts[msg.spaceId] ?? 0) + 1,
        };
        notifyIncomingMessage(msg);
      }
    });

    unsubEdited = wsService.onChatMessageEdited((data) => {
      const msgs = messages[data.spaceId];
      if (msgs) {
        messages = {
          ...messages,
          [data.spaceId]: msgs.map((m) =>
            m.id === data.messageId ? { ...m, text: data.text, editedAt: data.editedAt } : m,
          ),
        };
      }
    });

    unsubDeleted = wsService.onChatMessageDeleted((data) => {
      const msgs = messages[data.spaceId];
      if (msgs) {
        messages = {
          ...messages,
          [data.spaceId]: msgs.filter((m) => m.id !== data.messageId),
        };
      }
    });
  },

  /** Get display names of users currently typing in a space. */
  getTypingUsers(serverId: string): string[] {
    return (typingUsers[serverId] ?? []).map((t) => t.displayName);
  },

  /** Emit a typing indicator (debounced: max once per 2s). */
  emitTyping(serverId: string): void {
    if (typingDebounce) return;
    wsService.emitTyping(serverId);
    typingDebounce = setTimeout(() => {
      typingDebounce = null;
    }, 2000);
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
    if (unsubTyping) {
      unsubTyping();
      unsubTyping = null;
    }
    if (unsubEdited) {
      unsubEdited();
      unsubEdited = null;
    }
    if (unsubDeleted) {
      unsubDeleted();
      unsubDeleted = null;
    }
    replyingTo = null;
    editingMessage = null;
    // Clear all typing timeouts
    for (const entries of Object.values(typingUsers)) {
      for (const entry of entries) {
        clearTimeout(entry.timeout);
      }
    }
    typingUsers = {};
    if (typingDebounce) {
      clearTimeout(typingDebounce);
      typingDebounce = null;
    }
    messages = {};
    unreadCounts = {};
    activeSpaceId = null;
    loadingOlder = {};
    hasMore = {};
  },
};

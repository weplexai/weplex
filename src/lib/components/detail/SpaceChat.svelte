<script lang="ts">
  import type { ChatMessage } from '../../types';
  import { chatStore } from '../../stores/chatStore.svelte';
  import { authStore } from '../../stores/authStore.svelte';

  let { serverId }: { serverId: string } = $props();

  let inputText = $state('');
  let messagesEl: HTMLDivElement | undefined = $state();
  let wasAtBottom = $state(true);

  let msgs = $derived(chatStore.getMessages(serverId));
  let canLoadMore = $derived(chatStore.canLoadMore(serverId));
  let isLoadingOlder = $derived(chatStore.isLoadingOlder(serverId));

  // Mark as active when viewing, clear unread
  $effect(() => {
    const _id = serverId;
    chatStore.setActive(_id);
    return () => {
      chatStore.setActive(null);
    };
  });

  // Auto-scroll to bottom when new messages arrive (if already at bottom)
  $effect(() => {
    const _len = msgs.length;
    if (wasAtBottom && messagesEl) {
      requestAnimationFrame(() => {
        if (messagesEl) {
          messagesEl.scrollTop = messagesEl.scrollHeight;
        }
      });
    }
  });

  function handleScroll() {
    if (!messagesEl) return;
    const { scrollTop, scrollHeight, clientHeight } = messagesEl;
    wasAtBottom = scrollHeight - scrollTop - clientHeight < 40;

    // Load older messages when scrolled to top
    if (scrollTop === 0 && canLoadMore && !isLoadingOlder) {
      const prevHeight = messagesEl.scrollHeight;
      chatStore.loadOlder(serverId).then(() => {
        // Preserve scroll position after prepending
        requestAnimationFrame(() => {
          if (messagesEl) {
            messagesEl.scrollTop = messagesEl.scrollHeight - prevHeight;
          }
        });
      });
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      sendMessage();
    }
  }

  function sendMessage() {
    const text = inputText.trim();
    if (!text) return;
    chatStore.send(serverId, text);
    inputText = '';
    wasAtBottom = true;
  }

  /** Group consecutive messages by the same user within 5 minutes. */
  function shouldShowHeader(msg: ChatMessage, idx: number): boolean {
    if (idx === 0) return true;
    const prev = msgs[idx - 1];
    if (prev.userId !== msg.userId) return true;
    const gap = new Date(msg.createdAt).getTime() - new Date(prev.createdAt).getTime();
    return gap > 5 * 60 * 1000;
  }

  function isOwnMessage(msg: ChatMessage): boolean {
    return msg.userId === authStore.user?.id;
  }

  function formatTime(iso: string): string {
    const d = new Date(iso);
    return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  }
</script>

<div class="space-chat">
  <div class="messages" bind:this={messagesEl} onscroll={handleScroll}>
    {#if isLoadingOlder}
      <div class="loading-older">Loading...</div>
    {/if}

    {#if msgs.length === 0}
      <div class="empty-chat">No messages yet</div>
    {:else}
      {#each msgs as msg, idx}
        {#if shouldShowHeader(msg, idx)}
          <div class="msg-header">
            <span class="msg-author" class:own={isOwnMessage(msg)}>
              {isOwnMessage(msg) ? 'You' : (msg.displayName || msg.userEmail)}
            </span>
            <span class="msg-time">{formatTime(msg.createdAt)}</span>
          </div>
        {/if}
        <div class="msg-text">{msg.text}</div>
      {/each}
    {/if}
  </div>

  <div class="chat-input-area">
    <textarea
      class="chat-input"
      placeholder="Type a message..."
      bind:value={inputText}
      onkeydown={handleKeydown}
      maxlength={2000}
      rows="1"
    ></textarea>
    <button
      class="send-btn"
      onclick={sendMessage}
      disabled={!inputText.trim()}
      aria-label="Send message"
    >
      Send
    </button>
  </div>
</div>

<style>
  .space-chat {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 200px;
  }

  .messages {
    flex: 1;
    overflow-y: auto;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .loading-older {
    text-align: center;
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
    padding: 8px;
  }

  .empty-chat {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: var(--weplex-text-sm);
    color: var(--weplex-text-muted);
  }

  .msg-header {
    display: flex;
    align-items: baseline;
    gap: 6px;
    margin-top: 10px;
  }

  .msg-author {
    font-size: var(--weplex-text-sm);
    font-weight: 600;
    color: var(--weplex-text);
  }

  .msg-author.own {
    color: var(--weplex-accent);
  }

  .msg-time {
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
  }

  .msg-text {
    font-size: var(--weplex-text-sm);
    color: var(--weplex-text-secondary, var(--weplex-text));
    line-height: 1.4;
    white-space: pre-wrap;
    word-break: break-word;
    padding-left: 0;
  }

  .chat-input-area {
    display: flex;
    gap: 6px;
    margin-top: 8px;
    align-items: flex-end;
  }

  .chat-input {
    flex: 1;
    padding: 6px 8px;
    background: var(--weplex-bg);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-sm);
    color: var(--weplex-text);
    font-size: var(--weplex-text-xs);
    font-family: var(--weplex-font-mono);
    resize: none;
    outline: none;
    line-height: 1.5;
    min-height: 32px;
    max-height: 80px;
    box-sizing: border-box;
  }

  .chat-input:focus {
    border-color: var(--weplex-accent);
  }

  .chat-input::placeholder {
    color: var(--weplex-text-muted);
  }

  .send-btn {
    padding: 6px 12px;
    background: var(--weplex-accent);
    color: var(--weplex-bg);
    border: none;
    border-radius: var(--weplex-radius-sm);
    font-size: var(--weplex-text-xs);
    font-weight: 600;
    cursor: pointer;
    white-space: nowrap;
    height: 32px;
  }

  .send-btn:hover:not(:disabled) {
    opacity: 0.9;
  }

  .send-btn:disabled {
    opacity: 0.4;
    cursor: default;
  }
</style>

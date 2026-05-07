<script lang="ts">
  import type { ChatMessage } from '../../types';
  import { chatStore } from '../../stores/chatStore.svelte';
  import { authStore } from '../../stores/authStore.svelte';
  import { presenceStore } from '../../stores/presenceStore.svelte';
  import { sessionStore } from '../../stores/sessionStore';
  import { resolveProfileEnvId } from '../../utils/profile';

  let { serverId, sessionId }: { serverId: string; sessionId?: number } = $props();

  let inputText = $state('');
  let messagesEl: HTMLDivElement | undefined = $state();
  let textareaEl: HTMLTextAreaElement | undefined = $state();
  let wasAtBottom = $state(true);

  let msgs = $derived(chatStore.getMessages(serverId));
  let canLoadMore = $derived(chatStore.canLoadMore(serverId));
  let isLoadingOlder = $derived(chatStore.isLoadingOlder(serverId));
  let typingNames = $derived(chatStore.getTypingUsers(serverId));
  let replyTo = $derived(chatStore.replyTo);
  let editing = $derived(chatStore.editing);

  // @Mention state
  let showMentions = $state(false);
  let mentionFilter = $state('');
  let mentionIndex = $state(0);

  let mentionMembers = $derived(() => {
    const members = presenceStore.getMembers(serverId);
    const filter = mentionFilter.toLowerCase();
    return members
      .filter((m) => m.displayName?.toLowerCase().includes(filter) || (!filter))
      .slice(0, 8);
  });

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
    // Mention dropdown navigation
    if (showMentions) {
      const members = mentionMembers();
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        mentionIndex = (mentionIndex + 1) % Math.max(members.length, 1);
        return;
      }
      if (e.key === 'ArrowUp') {
        e.preventDefault();
        mentionIndex = (mentionIndex - 1 + Math.max(members.length, 1)) % Math.max(members.length, 1);
        return;
      }
      if (e.key === 'Enter' || e.key === 'Tab') {
        e.preventDefault();
        if (members.length > 0) {
          insertMention(members[mentionIndex]?.displayName ?? '');
        }
        return;
      }
      if (e.key === 'Escape') {
        e.preventDefault();
        showMentions = false;
        return;
      }
    }

    if (e.key === 'Escape') {
      if (editing) {
        e.preventDefault();
        chatStore.setEditing(null);
        inputText = '';
        return;
      }
      if (replyTo) {
        e.preventDefault();
        chatStore.setReplyTo(null);
        return;
      }
    }

    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      sendMessage();
    }
  }

  function sendMessage() {
    const text = inputText.trim();
    if (!text) return;

    if (editing) {
      chatStore.edit(editing.id, text);
      inputText = '';
      wasAtBottom = true;
      return;
    }

    chatStore.send(serverId, text);
    inputText = '';
    wasAtBottom = true;
  }

  function handleInput() {
    chatStore.emitTyping(serverId);
    detectMention();
  }

  /** Detect @mention trigger in textarea. */
  function detectMention() {
    if (!textareaEl) return;
    const val = textareaEl.value;
    const pos = textareaEl.selectionStart;
    // Look backwards from cursor for @
    const before = val.slice(0, pos);
    const atIdx = before.lastIndexOf('@');
    if (atIdx === -1 || (atIdx > 0 && /\S/.test(before[atIdx - 1]))) {
      showMentions = false;
      return;
    }
    const filter = before.slice(atIdx + 1);
    // Close if there's a space after some chars (completed mention)
    if (filter.includes('\n')) {
      showMentions = false;
      return;
    }
    mentionFilter = filter;
    mentionIndex = 0;
    showMentions = true;
  }

  /** Insert a mention at the @ position in the textarea. */
  function insertMention(name: string) {
    if (!textareaEl) return;
    const val = textareaEl.value;
    const pos = textareaEl.selectionStart;
    const before = val.slice(0, pos);
    const atIdx = before.lastIndexOf('@');
    if (atIdx === -1) return;
    const after = val.slice(pos);
    const inserted = `@${name} `;
    inputText = val.slice(0, atIdx) + inserted + after;
    showMentions = false;
    // Set cursor after mention
    requestAnimationFrame(() => {
      if (textareaEl) {
        const newPos = atIdx + inserted.length;
        textareaEl.selectionStart = newPos;
        textareaEl.selectionEnd = newPos;
        textareaEl.focus();
      }
    });
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

  /** Parse markdown + URLs in message text. */
  const urlRe = /https?:\/\/[^\s<>"')\]]+/g;

  interface Segment {
    type: 'text' | 'link' | 'code-inline' | 'code-block' | 'bold' | 'italic' | 'mention';
    value: string;
  }

  /** Linkify URLs within a plain text string. */
  function linkify(text: string): Segment[] {
    const segments: Segment[] = [];
    let lastIndex = 0;
    for (const match of text.matchAll(urlRe)) {
      if (match.index! > lastIndex) {
        segments.push({ type: 'text', value: text.slice(lastIndex, match.index!) });
      }
      segments.push({ type: 'link', value: match[0] });
      lastIndex = match.index! + match[0].length;
    }
    if (lastIndex < text.length) {
      segments.push({ type: 'text', value: text.slice(lastIndex) });
    }
    return segments.length > 0 ? segments : [{ type: 'text', value: text }];
  }

  /**
   * Parse message text into segments: code blocks, inline code, bold, italic, then URLs.
   * Parse order: code blocks (greedy) → inline code → bold → italic → URLs on text segments.
   */
  function parseMessage(text: string): Segment[] {
    // Step 1: Split on fenced code blocks (```...```)
    const codeBlockRe = /```(?:\w*)\n?([\s\S]*?)```/g;
    let parts = splitByRegex(text, codeBlockRe, 'code-block');

    // Step 2: Split text parts on inline code (`...`)
    const inlineCodeRe = /`([^`]+)`/g;
    parts = expandParts(parts, inlineCodeRe, 'code-inline');

    // Step 3: Split text parts on bold (**...**)
    const boldRe = /\*\*(.+?)\*\*/g;
    parts = expandParts(parts, boldRe, 'bold');

    // Step 4: Split text parts on italic (*...*)
    // Avoid matching ** (already consumed) — only single * not preceded/followed by *
    const italicRe = /(?<!\*)\*(?!\*)(.+?)(?<!\*)\*(?!\*)/g;
    parts = expandParts(parts, italicRe, 'italic');

    // Step 4b: Mentions (@Name)
    const mentionRe = /@(\w[\w\s]{0,30}\w)/g;
    parts = expandParts(parts, mentionRe, 'mention');

    // Step 5: Linkify remaining text segments
    const result: Segment[] = [];
    for (const seg of parts) {
      if (seg.type === 'text') {
        result.push(...linkify(seg.value));
      } else {
        result.push(seg);
      }
    }

    return result.length > 0 ? result : [{ type: 'text', value: text }];
  }

  /** Split text by regex into typed segments, keeping unmatched parts as 'text'. */
  function splitByRegex(
    text: string,
    re: RegExp,
    segType: Segment['type'],
  ): Segment[] {
    const segments: Segment[] = [];
    let lastIndex = 0;
    for (const match of text.matchAll(re)) {
      if (match.index! > lastIndex) {
        segments.push({ type: 'text', value: text.slice(lastIndex, match.index!) });
      }
      // Use first capture group as value (the content inside markers)
      segments.push({ type: segType, value: match[1] ?? match[0] });
      lastIndex = match.index! + match[0].length;
    }
    if (lastIndex < text.length) {
      segments.push({ type: 'text', value: text.slice(lastIndex) });
    }
    return segments.length > 0 ? segments : [{ type: 'text', value: text }];
  }

  /** For each 'text' segment in parts, further split by regex into segType segments. */
  function expandParts(
    parts: Segment[],
    re: RegExp,
    segType: Segment['type'],
  ): Segment[] {
    const result: Segment[] = [];
    for (const seg of parts) {
      if (seg.type === 'text') {
        result.push(...splitByRegex(seg.value, re, segType));
      } else {
        result.push(seg);
      }
    }
    return result;
  }

  async function shareSessionContext() {
    if (!sessionId) return;
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const data = await invoke<{
        summary: string;
        filesChanged: string[];
        decisions: string[];
        notes: { text: string; at: number }[];
      } | null>('get_session_summary', {
        sessionId: String(sessionId),
        profileId: (() => {
          const sess = sessionStore.sessions.find((s) => s.id === sessionId);
          return sess ? resolveProfileEnvId(sess) : 'default';
        })(),
      });

      if (!data) {
        chatStore.send(serverId, '📋 No activity recorded yet for this session.');
        return;
      }

      // Format as a readable message
      let msg = '📋 **Current session context**\n';

      // Use latest note if available, otherwise summary
      if (data.notes && data.notes.length > 0) {
        const latest = data.notes[data.notes.length - 1];
        msg += latest.text;
      } else if (data.summary) {
        msg += data.summary;
      }

      if (data.filesChanged && data.filesChanged.length > 0) {
        msg +=
          '\n\nFiles: ' +
          data.filesChanged.map((f) => '`' + f.split('/').pop() + '`').join(', ');
      }

      if (data.decisions && data.decisions.length > 0) {
        msg += '\n\nDecisions: ' + data.decisions.join(', ');
      }

      chatStore.send(serverId, msg);
    } catch (err) {
      console.warn('[Weplex] Failed to read session summary:', err);
    }
  }

  async function openLink(url: string) {
    try {
      const { open } = await import('@tauri-apps/plugin-shell');
      await open(url);
    } catch {
      window.open(url, '_blank');
    }
  }

  // Context menu state
  let contextMenu: { x: number; y: number; msg: ChatMessage; linkHref: string | null } | null = $state(null);

  function handleContextMenu(e: MouseEvent, msg: ChatMessage) {
    e.preventDefault();

    // Check if right-clicked on a link
    let linkHref: string | null = null;
    const target = e.target as HTMLElement;
    const anchor = target.closest('a.msg-link') as HTMLAnchorElement | null;
    if (anchor) {
      linkHref = anchor.href;
    }

    // Clamp to viewport edges
    const menuWidth = 180;
    const menuHeight = 80;
    const x = Math.min(e.clientX, window.innerWidth - menuWidth);
    const y = Math.min(e.clientY, window.innerHeight - menuHeight);

    contextMenu = { x, y, msg, linkHref };
  }

  function closeContextMenu() {
    contextMenu = null;
  }

  async function copyMessageText() {
    if (!contextMenu) return;
    try {
      await navigator.clipboard.writeText(contextMenu.msg.text);
    } catch { /* clipboard may fail in some environments */ }
    closeContextMenu();
  }

  async function copyLinkHref() {
    if (!contextMenu?.linkHref) return;
    try {
      await navigator.clipboard.writeText(contextMenu.linkHref);
    } catch { /* clipboard may fail in some environments */ }
    closeContextMenu();
  }

  function replyToMessage() {
    if (!contextMenu) return;
    chatStore.setReplyTo(contextMenu.msg);
    closeContextMenu();
    textareaEl?.focus();
  }

  function editMessage() {
    if (!contextMenu) return;
    chatStore.setEditing(contextMenu.msg);
    inputText = contextMenu.msg.text;
    closeContextMenu();
    requestAnimationFrame(() => textareaEl?.focus());
  }

  function deleteMessage() {
    if (!contextMenu) return;
    chatStore.deleteMsg(contextMenu.msg.id);
    closeContextMenu();
  }

  function handleGlobalKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape' && contextMenu) {
      closeContextMenu();
    }
  }
</script>

<svelte:window onkeydown={handleGlobalKeydown} onclick={closeContextMenu} />

<div class="space-chat">
  <div class="messages" bind:this={messagesEl} onscroll={() => { handleScroll(); closeContextMenu(); }}>
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
            {#if msg.editedAt}<span class="msg-edited">(edited)</span>{/if}
          </div>
        {/if}
        {#if msg.replyTo}
          <div class="msg-reply-quote">
            <span class="reply-quote-author">{msg.replyTo.displayName}:</span>
            {msg.replyTo.text.slice(0, 100)}
          </div>
        {/if}
        <div class="msg-text" oncontextmenu={(e) => handleContextMenu(e, msg)}>{#each parseMessage(msg.text) as seg}{#if seg.type === 'link'}<a
          href={seg.value}
          class="msg-link"
          onclick={(e) => { e.preventDefault(); openLink(seg.value); }}
        >{seg.value}</a>{:else if seg.type === 'code-block'}<pre class="msg-code-block"><code>{seg.value}</code></pre>{:else if seg.type === 'code-inline'}<code class="msg-code-inline">{seg.value}</code>{:else if seg.type === 'bold'}<strong>{seg.value}</strong>{:else if seg.type === 'italic'}<em>{seg.value}</em>{:else if seg.type === 'mention'}<span class="msg-mention">@{seg.value}</span>{:else}{seg.value}{/if}{/each}{#if msg.editedAt && !shouldShowHeader(msg, msgs.indexOf(msg))}<span class="msg-edited">(edited)</span>{/if}</div>
      {/each}
    {/if}
  </div>

  {#if typingNames.length > 0}
    <div class="typing-indicator">
      <span class="typing-dots"><span></span><span></span><span></span></span>
      <span class="typing-text">{typingNames.join(', ')} {typingNames.length === 1 ? 'is' : 'are'} typing</span>
    </div>
  {/if}

  {#if replyTo}
    <div class="reply-preview">
      <span class="reply-label">&#8617; {replyTo.displayName}</span>
      <span class="reply-text">{replyTo.text.slice(0, 80)}{replyTo.text.length > 80 ? '...' : ''}</span>
      <button class="reply-close" onclick={() => chatStore.setReplyTo(null)}>&#10005;</button>
    </div>
  {/if}

  {#if editing}
    <div class="reply-preview">
      <span class="reply-label">Editing message</span>
      <button class="reply-close" onclick={() => { chatStore.setEditing(null); inputText = ''; }}>&#10005;</button>
    </div>
  {/if}

  <div class="chat-input-area">
    {#if sessionId}
      <button
        class="context-btn"
        onclick={shareSessionContext}
        title="Share what you're working on"
        aria-label="Share session context"
      >
        📋
      </button>
    {/if}
    <div style="position: relative; flex: 1;">
      {#if showMentions}
        <div class="mention-dropdown">
          {#each mentionMembers() as member, i}
            <button
              class="mention-item"
              class:selected={i === mentionIndex}
              onmousedown={(e) => { e.preventDefault(); insertMention(member.displayName ?? ''); }}
            >
              {member.displayName ?? member.userId}
            </button>
          {:else}
            <div class="mention-item" style="color: var(--weplex-text-muted);">No members</div>
          {/each}
        </div>
      {/if}
      <textarea
        class="chat-input"
        placeholder={editing ? 'Edit message...' : 'Type a message...'}
        bind:value={inputText}
        bind:this={textareaEl}
        onkeydown={handleKeydown}
        oninput={handleInput}
        maxlength={2000}
        rows="1"
      ></textarea>
    </div>
    <button
      class="send-btn"
      onclick={sendMessage}
      disabled={!inputText.trim()}
      aria-label={editing ? 'Save edit' : 'Send message'}
    >
      {editing ? 'Save' : 'Send'}
    </button>
  </div>

  {#if contextMenu}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="context-menu"
      style="left: {contextMenu.x}px; top: {contextMenu.y}px;"
      onclick={(e) => e.stopPropagation()}
    >
      <button class="context-menu-item" onclick={replyToMessage}>Reply</button>
      <button class="context-menu-item" onclick={copyMessageText}>Copy message</button>
      {#if contextMenu.linkHref}
        <button class="context-menu-item" onclick={copyLinkHref}>Copy link</button>
      {/if}
      {#if isOwnMessage(contextMenu.msg)}
        <button class="context-menu-item" onclick={editMessage}>Edit</button>
        <button class="context-menu-item context-menu-danger" onclick={deleteMessage}>Delete</button>
      {/if}
    </div>
  {/if}
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
    user-select: text;
    -webkit-user-select: text;
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
    user-select: text;
    -webkit-user-select: text;
    cursor: text;
  }

  .msg-code-inline {
    background: var(--weplex-surface-hover);
    padding: 1px 4px;
    border-radius: 3px;
    font-family: var(--weplex-font-mono);
    font-size: var(--weplex-text-xs);
  }

  .msg-code-block {
    background: var(--weplex-surface-hover);
    padding: 8px;
    border-radius: var(--weplex-radius-sm);
    overflow-x: auto;
    font-family: var(--weplex-font-mono);
    font-size: var(--weplex-text-xs);
    white-space: pre;
    width: 100%;
    margin: 4px 0;
    user-select: all;
    -webkit-user-select: all;
    box-sizing: border-box;
  }

  .msg-code-block code {
    background: none;
    padding: 0;
    font: inherit;
  }

  .msg-link {
    color: var(--weplex-accent);
    text-decoration: underline;
    text-decoration-color: color-mix(in srgb, var(--weplex-accent) 40%, transparent);
    cursor: pointer;
  }

  .msg-link:hover {
    text-decoration-color: var(--weplex-accent);
  }

  .context-btn {
    padding: 4px 6px;
    background: none;
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-sm);
    cursor: pointer;
    font-size: 14px;
    line-height: 1;
    height: 32px;
    display: flex;
    align-items: center;
    opacity: 0.6;
  }

  .context-btn:hover {
    opacity: 1;
    border-color: var(--weplex-accent);
  }

  .typing-indicator {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 0;
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
    min-height: 20px;
  }

  .typing-dots {
    display: inline-flex;
    gap: 2px;
  }

  .typing-dots span {
    width: 4px;
    height: 4px;
    border-radius: 50%;
    background: var(--weplex-text-muted);
    animation: typing-bounce 1.4s infinite;
  }

  .typing-dots span:nth-child(2) {
    animation-delay: 0.2s;
  }

  .typing-dots span:nth-child(3) {
    animation-delay: 0.4s;
  }

  @keyframes typing-bounce {
    0%, 60%, 100% {
      transform: translateY(0);
      opacity: 0.4;
    }
    30% {
      transform: translateY(-4px);
      opacity: 1;
    }
  }

  .typing-text {
    font-style: italic;
  }

  .chat-input-area {
    display: flex;
    gap: 6px;
    margin-top: 8px;
    align-items: flex-end;
  }

  .chat-input {
    width: 100%;
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

  .context-menu {
    position: fixed;
    background: var(--weplex-surface, #1a1a2e);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-sm);
    padding: 4px 0;
    z-index: 1000;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.4);
    min-width: 160px;
  }

  .context-menu-item {
    display: block;
    width: 100%;
    padding: 6px 12px;
    font-size: var(--weplex-text-sm);
    color: var(--weplex-text);
    cursor: pointer;
    background: none;
    border: none;
    text-align: left;
    font-family: inherit;
  }

  .context-menu-item:hover {
    background: var(--weplex-surface-hover);
  }

  .context-menu-danger {
    color: var(--weplex-error, #EF4444);
  }

  .reply-preview {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 8px;
    background: var(--weplex-surface-hover);
    border-left: 2px solid var(--weplex-accent);
    border-radius: var(--weplex-radius-sm);
    font-size: var(--weplex-text-xs);
  }

  .reply-label {
    color: var(--weplex-accent);
    font-weight: 600;
    white-space: nowrap;
  }

  .reply-text {
    color: var(--weplex-text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
  }

  .reply-close {
    background: none;
    border: none;
    color: var(--weplex-text-muted);
    cursor: pointer;
    padding: 0 2px;
    font-size: 12px;
  }

  .reply-close:hover {
    color: var(--weplex-text);
  }

  .msg-reply-quote {
    padding: 4px 8px;
    margin-bottom: 2px;
    border-left: 2px solid var(--weplex-border);
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .reply-quote-author {
    font-weight: 600;
    margin-right: 4px;
  }

  .msg-edited {
    font-size: 9px;
    color: var(--weplex-text-muted);
    font-style: italic;
    margin-left: 4px;
  }

  .msg-mention {
    color: var(--weplex-accent);
    font-weight: 500;
  }

  .mention-dropdown {
    position: absolute;
    bottom: 100%;
    left: 0;
    background: var(--weplex-surface, #1a1a2e);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-sm);
    max-height: 120px;
    overflow-y: auto;
    z-index: 100;
    min-width: 140px;
    box-shadow: 0 -4px 12px rgba(0, 0, 0, 0.3);
  }

  .mention-item {
    display: block;
    width: 100%;
    padding: 6px 10px;
    font-size: var(--weplex-text-sm);
    color: var(--weplex-text);
    cursor: pointer;
    background: none;
    border: none;
    text-align: left;
    font-family: inherit;
  }

  .mention-item:hover,
  .mention-item.selected {
    background: var(--weplex-surface-hover);
  }
</style>

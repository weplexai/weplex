<script lang="ts">
  import { presenceStore } from '../../stores/presenceStore.svelte';
  import { sessionStore } from '../../stores/sessionStore';
  import { spaceStore } from '../../stores/spaceStore';
  import { Users } from 'lucide-svelte';
  import type { MemberPresence, SessionRecord } from '../../types';

  function spectateSession(member: MemberPresence, sessionLabel: string) {
    const activeSpace = spaceStore.activeSpace;
    if (!activeSpace?.serverId) return;
    sessionStore.createSpectatorSession(
      activeSpace.id,
      activeSpace.serverId,
      sessionLabel,
      member.displayName,
    );
  }

  interface Props {
    serverId: string;
  }

  let { serverId }: Props = $props();

  let members: MemberPresence[] = $derived(presenceStore.getMembers(serverId));
  let history: SessionRecord[] = $derived(presenceStore.getHistory(serverId));
  let historyLoading: boolean = $derived(presenceStore.isHistoryLoading(serverId));

  // Online user IDs for filtering history
  let onlineUserIds: Set<string> = $derived(new Set(members.map((m) => m.userId)));

  // Group offline history by userId, show most recent sessions per user
  interface OfflineMember {
    userId: string;
    displayName: string;
    lastSeenAt: string;
    sessions: SessionRecord[];
  }

  let offlineMembers: OfflineMember[] = $derived.by(() => {
    // Filter out sessions from currently online members
    const offlineRecords = history.filter((r) => !onlineUserIds.has(r.userId));
    if (offlineRecords.length === 0) return [];

    // Group by userId
    const grouped = new Map<string, SessionRecord[]>();
    for (const record of offlineRecords) {
      const existing = grouped.get(record.userId) ?? [];
      existing.push(record);
      grouped.set(record.userId, existing);
    }

    // Build offline member entries
    const result: OfflineMember[] = [];
    for (const [userId, sessions] of grouped) {
      // Sort sessions by lastSeenAt descending
      sessions.sort((a, b) => new Date(b.lastSeenAt).getTime() - new Date(a.lastSeenAt).getTime());

      const latest = sessions[0];
      const displayName =
        latest.user?.displayName || latest.user?.email || userId.slice(0, 8);

      result.push({
        userId,
        displayName,
        lastSeenAt: latest.lastSeenAt,
        sessions: sessions.slice(0, 3), // Show up to 3 most recent sessions
      });
    }

    // Sort by most recently active first
    result.sort(
      (a, b) => new Date(b.lastSeenAt).getTime() - new Date(a.lastSeenAt).getTime(),
    );

    return result;
  });

  /** Format a timestamp as relative time (e.g. "2h ago", "yesterday"). */
  function relativeTime(isoDate: string): string {
    const now = Date.now();
    const then = new Date(isoDate).getTime();
    const diffMs = now - then;

    if (diffMs < 0) return 'just now';

    const seconds = Math.floor(diffMs / 1000);
    if (seconds < 60) return 'just now';

    const minutes = Math.floor(seconds / 60);
    if (minutes < 60) return `${minutes}m ago`;

    const hours = Math.floor(minutes / 60);
    if (hours < 24) return `${hours}h ago`;

    const days = Math.floor(hours / 24);
    if (days === 1) return 'yesterday';
    if (days < 7) return `${days}d ago`;

    const weeks = Math.floor(days / 7);
    if (weeks < 4) return `${weeks}w ago`;

    return new Date(isoDate).toLocaleDateString();
  }
</script>

{#if members.length > 0 || offlineMembers.length > 0}
  <div class="team-presence">
    {#if members.length > 0}
      <div class="team-presence-header">
        <Users size={10} />
        <span>Online</span>
      </div>
      {#each members as member (member.userId)}
        {@const isActive = member.sessions.some((s) => s.status === 'active')}
        <div class="member-row">
          <div class="member-info">
            <span class="member-dot" class:active={isActive}></span>
            <span class="member-name">{member.displayName}</span>
            <span class="member-status">{isActive ? 'active' : 'idle'}</span>
          </div>
          {#each member.sessions.filter((s) => s.status !== 'closed') as session (session.id)}
            <button
              class="member-session spectatable"
              onclick={() => spectateSession(member, session.label || session.agentType || 'session')}
              title="Spectate this session"
            >
              <span class="session-agent">{session.agentType || 'terminal'}</span>
              {#if session.cwd}
                <span class="session-cwd">{session.cwd.split('/').pop()}</span>
              {/if}
              {#if session.gitBranch}
                <span class="session-branch">{session.gitBranch}</span>
              {/if}
              <span class="spectate-icon">👁</span>
            </button>
            {#if session.summary}
              <div class="session-summary">"{session.summary}"</div>
            {/if}
            {#if session.filesChanged && session.filesChanged.length > 0}
              <div class="session-files">files: {session.filesChanged.map(f => f.split('/').pop()).join(', ')}</div>
            {/if}
            {#if session.decisions && session.decisions.length > 0}
              <div class="session-decisions">Decisions: {session.decisions.join(', ')}</div>
            {/if}
          {/each}
        </div>
      {/each}
    {/if}

    {#if offlineMembers.length > 0}
      <div class="team-presence-header offline-header">
        <span>Recent</span>
      </div>
      {#each offlineMembers as offlineMember (offlineMember.userId)}
        <div class="member-row offline">
          <div class="member-info">
            <span class="member-dot offline"></span>
            <span class="member-name">{offlineMember.displayName}</span>
            <span class="member-status">{relativeTime(offlineMember.lastSeenAt)}</span>
          </div>
          {#each offlineMember.sessions as session (session.id)}
            <div class="member-session">
              <span class="session-agent">{session.agentType || 'terminal'}</span>
              {#if session.cwd}
                <span class="session-cwd">{session.cwd.split('/').pop()}</span>
              {/if}
              {#if session.gitBranch}
                <span class="session-branch">{session.gitBranch}</span>
              {/if}
            </div>
            {#if session.summary}
              <div class="session-summary">"{session.summary}"</div>
            {/if}
            {#if session.filesChanged && session.filesChanged.length > 0}
              <div class="session-files">files: {session.filesChanged.map(f => f.split('/').pop()).join(', ')}</div>
            {/if}
            {#if session.decisions && session.decisions.length > 0}
              <div class="session-decisions">Decisions: {session.decisions.join(', ')}</div>
            {/if}
          {/each}
        </div>
      {/each}
    {:else if historyLoading}
      <div class="team-presence-header offline-header">
        <span>Recent</span>
        <span class="loading-hint">loading...</span>
      </div>
    {/if}
  </div>
{/if}

<style>
  .team-presence {
    padding: 4px 0;
    margin-top: 4px;
  }

  .team-presence-header {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 12px;
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .offline-header {
    margin-top: 6px;
    opacity: 0.7;
  }

  .loading-hint {
    font-size: 9px;
    text-transform: none;
    letter-spacing: normal;
    opacity: 0.5;
  }

  .member-row {
    padding: 2px 12px;
  }

  .member-row.offline {
    opacity: 0.6;
  }

  .member-info {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 0;
  }

  .member-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--weplex-text-muted);
    flex-shrink: 0;
  }

  .member-dot.active {
    background: var(--weplex-active);
  }

  .member-dot.offline {
    background: var(--weplex-text-muted);
    opacity: 0.4;
  }

  .member-name {
    font-size: var(--weplex-text-sm);
    color: var(--weplex-text);
    font-weight: 500;
  }

  .member-status {
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
    margin-left: auto;
  }

  .member-session {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 1px 0 1px 12px;
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-secondary);
  }

  .member-session.spectatable {
    background: none;
    border: none;
    cursor: pointer;
    width: 100%;
    text-align: left;
    border-radius: var(--weplex-radius-sm);
  }

  .member-session.spectatable:hover {
    background: var(--weplex-surface-hover);
  }

  .spectate-icon {
    margin-left: auto;
    opacity: 0;
    font-size: 10px;
    transition: opacity 0.15s;
  }

  .member-session.spectatable:hover .spectate-icon {
    opacity: 0.7;
  }

  .session-agent {
    color: var(--weplex-text-muted);
  }

  .session-agent::after {
    content: ':';
  }

  .session-cwd {
    color: var(--weplex-text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .session-branch {
    color: var(--weplex-accent);
    font-family: var(--weplex-font-mono);
    font-size: 10px;
  }

  .session-summary {
    padding: 1px 0 1px 12px;
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
    font-style: italic;
    overflow: hidden;
    text-overflow: ellipsis;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    line-height: 1.3;
  }

  .session-files {
    padding: 0 0 0 12px;
    font-size: 10px;
    color: var(--weplex-text-muted);
    font-family: var(--weplex-font-mono);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    opacity: 0.7;
  }

  .session-decisions {
    padding: 0 0 1px 12px;
    font-size: 10px;
    color: var(--weplex-text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    opacity: 0.7;
  }
</style>

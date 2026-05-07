<script lang="ts">
  import { presenceStore } from '../../stores/presenceStore.svelte';
  import { sessionStore } from '../../stores/sessionStore';
  import { spaceStore } from '../../stores/spaceStore';
  import { Users } from 'lucide-svelte';
  import type { MemberPresence } from '../../types';

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
</script>

{#if members.length > 0}
  <div class="team-presence">
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
            onclick={() => spectateSession(member, session.name || session.agentType || 'session')}
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
        {/each}
      </div>
    {/each}
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

  .member-row {
    padding: 2px 12px;
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
</style>

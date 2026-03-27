<script lang="ts">
  import type { Session } from '../../types';
  import { AGENT_ICONS, SESSION_TYPE_ICONS } from '../../types';
  import { uiStore } from '../../stores/uiStore';
  import { formatCost, formatTokens, formatDuration } from '../../utils/time';

  let { session }: { session: Session | undefined } = $props();

  function getIcon(s: Session): string {
    if (s.type === 'agent' && s.agentType) return AGENT_ICONS[s.agentType];
    return SESSION_TYPE_ICONS[s.type];
  }
</script>

<header class="header">
  {#if session}
    <div class="header-left">
      <span class="header-icon">{getIcon(session)}</span>
      <span class="header-name">{session.name}</span>
      {#if session.type === 'agent'}
        <span class="header-tag">{session.agentType || 'agent'}</span>
        {#if session.model}
          <span class="header-tag model">{session.model}</span>
        {/if}
      {:else if session.type === 'ssh'}
        <span class="header-tag ssh"
          >{session.sshUser ? `${session.sshUser}@` : ''}{session.host}</span
        >
      {:else}
        <span class="header-tag">{session.cwd || '~'}</span>
      {/if}
    </div>

    <div class="header-right">
      {#if session.branch}
        <span class="header-meta branch">{session.branch}</span>
      {/if}
      {#if session.gitFiles?.length}
        <span class="header-meta changes">
          +{session.gitFiles.filter((f) => f.status === 'A').length}
          M{session.gitFiles.filter((f) => f.status === 'M').length}
        </span>
      {/if}
      {#if session.tokensIn || session.tokensOut}
        <span class="header-meta tokens">
          {formatTokens((session.tokensIn || 0) + (session.tokensOut || 0))} tokens
        </span>
      {/if}
      {#if session.cost}
        <span class="header-meta cost">{formatCost(session.cost)}</span>
      {/if}
      <span class="header-meta uptime">{formatDuration(Date.now() - session.createdAt)}</span>

      <button
        class="detail-toggle"
        class:active={uiStore.detailPanelOpen}
        onclick={() => uiStore.toggleDetailPanel()}
        title="Toggle detail panel (⌘.)"
      >
        ⓘ
      </button>
    </div>
  {:else}
    <div class="header-left">
      <span class="header-empty">No session selected</span>
    </div>
  {/if}
</header>

<style>
  .header {
    height: var(--weplex-header-height);
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 12px;
    background: transparent;
    border-bottom: 1px solid var(--weplex-border);
    -webkit-app-region: drag;
    flex-shrink: 0;
    gap: 8px;
  }

  .header-left {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    -webkit-app-region: no-drag;
  }

  .header-right {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-shrink: 0;
    -webkit-app-region: no-drag;
  }

  .header-icon {
    font-size: 14px;
    flex-shrink: 0;
  }

  .header-name {
    font-weight: 600;
    font-size: var(--weplex-text-base);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .header-tag {
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
    padding: 1px 6px;
    border-radius: var(--weplex-radius-sm);
    background: var(--weplex-surface-hover);
    font-family: var(--weplex-font-mono);
    white-space: nowrap;
  }

  .header-tag.model {
    color: var(--weplex-accent);
    background: color-mix(in srgb, var(--weplex-accent) 10%, transparent);
  }

  .header-tag.ssh {
    color: var(--weplex-info);
    background: color-mix(in srgb, var(--weplex-info) 10%, transparent);
  }

  .header-meta {
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
    font-family: var(--weplex-font-mono);
    white-space: nowrap;
  }

  .header-meta.branch {
    color: var(--weplex-active);
  }

  .header-meta.cost {
    color: var(--weplex-warning);
  }

  .header-empty {
    font-size: var(--weplex-text-sm);
    color: var(--weplex-text-muted);
  }

  .detail-toggle {
    width: 26px;
    height: 26px;
    border: none;
    border-radius: var(--weplex-radius-sm);
    background: transparent;
    color: var(--weplex-text-muted);
    font-size: 15px;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all var(--weplex-duration-fast) var(--weplex-easing);
  }

  .detail-toggle:hover {
    background: var(--weplex-surface-hover);
    color: var(--weplex-text);
  }

  .detail-toggle.active {
    color: var(--weplex-accent);
  }
</style>

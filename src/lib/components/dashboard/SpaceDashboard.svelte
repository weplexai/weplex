<script lang="ts">
  import type { Session } from '../../types';
  import { STATUS_COLORS } from '../../types';
  import { sessionStore } from '../../stores/sessionStore';
  import { spaceStore } from '../../stores/spaceStore';
  import { formatCost } from '../../utils/time';
  import { shortPath } from '../../utils/path';

  let {
    sessionId,
  }: {
    sessionId: number;
  } = $props();

  let dashSession = $derived(sessionStore.sessions.find((s) => s.id === sessionId));
  let spaceId = $derived(dashSession?.spaceId || '');
  let space = $derived(spaceStore.spaces.find((s) => s.id === spaceId));

  // All non-dashboard sessions in this space
  let spaceSessions = $derived(
    sessionStore.sessions.filter(
      (s) => s.spaceId === spaceId && s.type !== 'dashboard',
    ),
  );

  // Group sessions by cwd (project)
  let projectGroups = $derived.by(() => {
    const groups = new Map<string, Session[]>();
    for (const s of spaceSessions) {
      const key = s.cwd || '~';
      const arr = groups.get(key) || [];
      arr.push(s);
      groups.set(key, arr);
    }
    return [...groups.entries()]
      .map(([cwd, sessions]) => ({ cwd, sessions }))
      .sort((a, b) => b.sessions.length - a.sessions.length);
  });

  // Stats
  let activeCount = $derived(spaceSessions.filter((s) => s.status === 'active').length);
  let waitingCount = $derived(spaceSessions.filter((s) => s.status === 'waiting').length);
  let idleCount = $derived(spaceSessions.filter((s) => s.status === 'idle').length);
  let totalCost = $derived(spaceSessions.reduce((sum, s) => sum + (s.cost || 0), 0));

  let isVisible = $derived(sessionId === sessionStore.activeSessionId);

  function statusLabel(status: string): string {
    if (status === 'active') return 'active';
    if (status === 'waiting') return 'waiting';
    return 'idle';
  }
</script>

<div class="dashboard" class:visible={isVisible}>
  <!-- Header -->
  <div class="dash-header">
    <h2 class="dash-title" style="color: {space?.color || 'var(--weplex-text)'}">
      {space?.name || 'Space'}
    </h2>
    <div class="dash-meta">
      {#if activeCount > 0}
        <span class="meta-item active-text">{activeCount} active</span>
      {/if}
      {#if waitingCount > 0}
        <span class="meta-sep">|</span>
        <span class="meta-item warning-text">{waitingCount} waiting</span>
      {/if}
      {#if idleCount > 0}
        <span class="meta-sep">|</span>
        <span class="meta-item">{idleCount} idle</span>
      {/if}
      <span class="meta-sep">|</span>
      <span class="meta-item">{spaceSessions.length} session{spaceSessions.length !== 1 ? 's' : ''}</span>
      {#if totalCost > 0}
        <span class="meta-sep">|</span>
        <span class="meta-item">{formatCost(totalCost)}</span>
      {/if}
    </div>
  </div>

  <!-- Project groups -->
  {#if projectGroups.length > 0}
    <div class="project-grid">
      {#each projectGroups as group}
        <div class="project-card">
          <div class="project-header">
            <span class="project-path">{shortPath(group.cwd)}</span>
            <span class="project-count">{group.sessions.length}</span>
          </div>
          <div class="project-sessions">
            {#each group.sessions as s (s.id)}
              <button
                class="session-chip"
                class:chip-active={s.id === sessionStore.activeSessionId}
                onclick={() => sessionStore.activate(s.id)}
              >
                <span
                  class="status-dot"
                  class:pulse={s.status === 'active'}
                  style="background: {STATUS_COLORS[s.status]}"
                ></span>
                <span class="chip-name">{s.name}</span>
              </button>
            {/each}
          </div>
        </div>
      {/each}
    </div>
  {:else}
    <div class="empty">No sessions in this space</div>
  {/if}

  <!-- Footer stats -->
  <div class="dash-footer">
    <span
      class="status-dot"
      style="background: var(--weplex-active)"
    ></span>
    <span>{activeCount} active</span>
    {#if waitingCount > 0}
      <span
        class="status-dot"
        style="background: var(--weplex-warning)"
      ></span>
      <span>{waitingCount} waiting</span>
    {/if}
    <span
      class="status-dot"
      style="background: var(--weplex-text-muted)"
    ></span>
    <span>{idleCount} idle</span>
    <span class="footer-total">{spaceSessions.length} sessions</span>
  </div>
</div>

<style>
  .dashboard {
    position: absolute;
    top: 0; left: 0; right: 0; bottom: 0;
    overflow-y: auto;
    padding: 20px 24px;
    background: var(--weplex-bg);
    display: none;
  }
  .dashboard.visible { display: block; }

  .dash-header {
    margin-bottom: 20px;
    padding-bottom: 12px;
    border-bottom: 1px solid var(--weplex-border);
  }
  .dash-title {
    font-size: 18px;
    font-weight: 600;
    margin: 0 0 6px 0;
  }
  .dash-meta {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: var(--weplex-text-sm);
    color: var(--weplex-text-muted);
  }
  .meta-sep { color: var(--weplex-border); }
  .active-text { color: var(--weplex-active); }
  .warning-text { color: var(--weplex-warning); }

  .project-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: 12px;
  }

  .project-card {
    background: var(--weplex-surface);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-md);
    padding: 12px;
  }

  .project-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 8px;
    padding-bottom: 6px;
    border-bottom: 1px solid var(--weplex-border);
  }

  .project-path {
    font-family: var(--weplex-font-mono);
    font-size: var(--weplex-text-sm);
    color: var(--weplex-text);
    font-weight: 500;
  }

  .project-count {
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
  }

  .project-sessions {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .session-chip {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 6px;
    border-radius: var(--weplex-radius-sm);
    background: none;
    border: none;
    color: var(--weplex-text);
    font-size: var(--weplex-text-xs);
    cursor: pointer;
    text-align: left;
    width: 100%;
  }
  .session-chip:hover { background: var(--weplex-surface-hover); }
  .session-chip.chip-active { background: var(--weplex-surface-active); }
  .chip-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .status-dot {
    width: 6px; height: 6px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .status-dot.pulse {
    animation: dot-pulse 1.4s ease-in-out infinite;
  }
  @keyframes dot-pulse {
    0%, 100% { opacity: 1; transform: scale(1); }
    50% { opacity: 0.45; transform: scale(0.75); }
  }

  .dash-footer {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 20px;
    padding-top: 12px;
    border-top: 1px solid var(--weplex-border);
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
  }

  .footer-total {
    margin-left: auto;
  }

  .empty {
    padding: 40px;
    text-align: center;
    color: var(--weplex-text-muted);
    font-size: var(--weplex-text-sm);
  }
</style>

<script lang="ts">
  import type { HyperspaceGroupBy, Session } from '../../types';
  import { STATUS_COLORS } from '../../types';
  import { sessionStore } from '../../stores/sessionStore';
  import { spaceStore } from '../../stores/spaceStore';
  import { uiStore } from '../../stores/uiStore';
  import { dragStore } from '../../stores/dragStore';
  import SessionItem from './SessionItem.svelte';
  import SpaceBadge from './SpaceBadge.svelte';

  const STORAGE_KEY = 'weplex_hyperspace_groupby';

  let groupBy = $state<HyperspaceGroupBy>(
    (localStorage.getItem(STORAGE_KEY) as HyperspaceGroupBy) || 'space',
  );

  let collapsedGroups = $state<Set<string>>(new Set());

  let groups = $derived(sessionStore.getAllGrouped(groupBy));
  let showBadge = $derived(groupBy !== 'space');

  function setGroupBy(mode: HyperspaceGroupBy) {
    groupBy = mode;
    localStorage.setItem(STORAGE_KEY, mode);
    collapsedGroups = new Set();
  }

  function toggleGroup(key: string) {
    const next = new Set(collapsedGroups);
    if (next.has(key)) {
      next.delete(key);
    } else {
      next.add(key);
    }
    collapsedGroups = next;
  }

  function getSpaceForSession(session: Session) {
    return spaceStore.spaces.find((s) => s.id === session.spaceId);
  }

  // Aggregate stats across all sessions
  let stats = $derived(sessionStore.stats);
</script>

<!-- GroupBy selector -->
<div class="group-selector">
  <span class="group-label">Group by:</span>
  <button class="group-btn" class:active={groupBy === 'space'} onclick={() => setGroupBy('space')}
    >Space</button
  >
  <span class="group-sep">&middot;</span>
  <button class="group-btn" class:active={groupBy === 'status'} onclick={() => setGroupBy('status')}
    >Status</button
  >
  <span class="group-sep">&middot;</span>
  <button
    class="group-btn"
    class:active={groupBy === 'project'}
    onclick={() => setGroupBy('project')}>Project</button
  >
</div>

<!-- Grouped list -->
<div class="groups">
  {#each groups as group (group.key)}
    {@const isSpaceGroup = groupBy === 'space'}
    {@const isDropTarget =
      isSpaceGroup &&
      dragStore.isDragging &&
      dragStore.dropTarget?.type === 'space-group' &&
      dragStore.dropTarget.id === group.key}
    <div
      class="group"
      class:hyperspace-group={isSpaceGroup}
      class:group-drop-target={isDropTarget}
      data-space-id={isSpaceGroup ? group.key : undefined}
    >
      <button
        class="group-header"
        onclick={() => toggleGroup(group.key)}
        style={group.color ? `border-left: 3px solid ${group.color}` : ''}
      >
        <span class="group-chevron" class:collapsed={collapsedGroups.has(group.key)}>&#9662;</span>
        {#if groupBy === 'status'}
          <span
            class="group-status-dot"
            style="background: {STATUS_COLORS[group.key] || 'var(--weplex-text-muted)'}"
          ></span>
        {/if}
        <span class="group-name">{group.label}</span>
        <span class="group-count">({group.sessions.length})</span>
      </button>

      {#if !collapsedGroups.has(group.key)}
        <div class="group-sessions">
          {#each group.sessions as session (session.id)}
            {@const space = showBadge ? getSpaceForSession(session) : null}
            <SessionItem
              {session}
              active={session.id === sessionStore.activeSessionId}
              onclick={() => sessionStore.activate(session.id)}
              badgeLetter={space ? space.name[0].toUpperCase() : undefined}
              badgeColor={space?.color}
            />
          {/each}
        </div>
      {/if}
    </div>
  {:else}
    <div class="empty">
      <p class="empty-text">No sessions yet</p>
      <p class="empty-hint">Press Cmd+N to create one</p>
    </div>
  {/each}
</div>

<!-- Sticky footer with aggregate stats -->
<div class="hyperspace-footer">
  <div class="footer-stats">
    {#if stats.active > 0}
      <span class="stat"><span class="stat-dot active"></span>{stats.active} active</span>
    {/if}
    {#if stats.waiting > 0}
      <span class="stat"><span class="stat-dot waiting"></span>{stats.waiting} waiting</span>
    {/if}
    {#if stats.idle > 0}
      <span class="stat"><span class="stat-dot idle"></span>{stats.idle} idle</span>
    {/if}
    {#if stats.totalCost > 0}
      <span class="stat">${stats.totalCost.toFixed(2)}</span>
    {/if}
    {#if stats.total === 0}
      <span class="stat muted">No sessions</span>
    {/if}
  </div>
  <button class="new-session-btn" onclick={() => uiStore.openOverlay('new-session')}>
    + New Session
  </button>
</div>

<style>
  .group-selector {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 8px 4px 6px;
    flex-shrink: 0;
  }

  .group-label {
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
    margin-right: 2px;
  }

  .group-btn {
    border: none;
    background: none;
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-secondary);
    cursor: pointer;
    padding: 2px 4px;
    border-radius: var(--weplex-radius-sm);
    transition: color var(--weplex-duration-fast) var(--weplex-easing);
  }

  .group-btn:hover {
    color: var(--weplex-text);
  }

  .group-btn.active {
    color: var(--weplex-text);
    text-decoration: underline;
    text-underline-offset: 3px;
  }

  .group-sep {
    color: var(--weplex-text-muted);
    font-size: var(--weplex-text-xs);
  }

  .groups {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
  }

  .group {
    display: flex;
    flex-direction: column;
    border-radius: var(--weplex-radius-md);
    transition: background var(--weplex-duration-fast) var(--weplex-easing);
  }

  .group.group-drop-target {
    background: color-mix(in srgb, var(--weplex-accent) 10%, transparent);
  }

  .group-header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 8px;
    border: none;
    border-left: 3px solid transparent;
    border-radius: 0 var(--weplex-radius-sm) var(--weplex-radius-sm) 0;
    background: transparent;
    color: var(--weplex-text-muted);
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.06em;
    cursor: pointer;
    text-align: left;
    transition: background var(--weplex-duration-fast) var(--weplex-easing);
  }

  .group-header:hover {
    background: var(--weplex-surface);
  }

  .group-chevron {
    font-size: 10px;
    transition: transform var(--weplex-duration-fast) var(--weplex-easing);
    line-height: 1;
  }

  .group-chevron.collapsed {
    transform: rotate(-90deg);
  }

  .group-status-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .group-name {
    flex: 1;
  }

  .group-count {
    color: var(--weplex-text-muted);
    font-weight: 400;
  }

  .group-sessions {
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding-left: 4px;
  }

  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 4px;
    text-align: center;
    padding: 32px 0;
  }

  .empty-text {
    font-size: var(--weplex-text-sm);
    color: var(--weplex-text-secondary);
  }

  .empty-hint {
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
    margin-top: 4px;
  }

  .hyperspace-footer {
    margin-top: auto;
    position: sticky;
    bottom: 0;
    padding: 8px;
    border-top: 1px solid var(--weplex-border);
    background: var(--weplex-sidebar-bg);
    flex-shrink: 0;
  }

  .footer-stats {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    padding-bottom: 6px;
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-secondary);
  }

  .stat {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }

  .stat.muted {
    color: var(--weplex-text-muted);
  }

  .stat-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .stat-dot.active {
    background: var(--weplex-active);
  }
  .stat-dot.waiting {
    background: var(--weplex-warning);
  }
  .stat-dot.idle {
    background: var(--weplex-text-muted);
  }

  .new-session-btn {
    width: 100%;
    padding: 6px;
    border: 1px dashed var(--weplex-border);
    border-radius: var(--weplex-radius-md);
    background: transparent;
    color: var(--weplex-text-muted);
    font-size: var(--weplex-text-xs);
    cursor: pointer;
    transition: all var(--weplex-duration-fast) var(--weplex-easing);
  }

  .new-session-btn:hover {
    border-color: var(--weplex-accent);
    color: var(--weplex-accent);
    background: color-mix(in srgb, var(--weplex-accent) 5%, transparent);
  }
</style>

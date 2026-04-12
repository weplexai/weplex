<script lang="ts">
  import type { Session, SessionStatus } from '../../types';
  import { STATUS_COLORS } from '../../types';
  import { sessionStore } from '../../stores/sessionStore';
  import { hookStore } from '../../stores/hookStore.svelte';
  import { formatCost, formatDuration, formatAbsoluteTime } from '../../utils/time';
  let {
    sessionId,
    orchestratorId,
  }: {
    sessionId: number;
    orchestratorId?: number;
  } = $props();

  // The orchestrator session (parent of the agent tree)
  let orchestrator = $derived(
    sessionStore.sessions.find((s) => s.id === (orchestratorId || sessionId)),
  );

  // Child sessions of the orchestrator
  let children = $derived(
    orchestratorId ? sessionStore.getChildren(orchestratorId) : [],
  );

  // Aggregated stats
  let totalCost = $derived(
    [orchestrator, ...children]
      .filter(Boolean)
      .reduce((sum, s) => sum + ((s as Session).cost || 0), 0),
  );
  let activeCount = $derived(children.filter((c) => c.status === 'active').length);

  // Ticking clock for live duration display
  let now = $state(Date.now());
  $effect(() => {
    const id = setInterval(() => { now = Date.now(); }, 1000);
    return () => clearInterval(id);
  });
  let elapsedMs = $derived(orchestrator ? now - orchestrator.createdAt : 0);

  // Activity feed: recent tool uses from all child sessions
  let activityFeed = $derived.by(() => {
    const allActivities: {
      sessionName: string;
      toolName: string;
      filePath?: string;
      timestamp: number;
    }[] = [];

    for (const child of children) {
      const activity = hookStore.getActivity(child.id);
      if (!activity) continue;
      for (const tool of activity.toolUses) {
        if (tool.type !== 'post') continue;
        allActivities.push({
          sessionName: child.name.split(': ').pop() || child.name,
          toolName: tool.toolName,
          filePath: tool.filePath,
          timestamp: tool.timestamp,
        });
      }
    }

    // Sort newest first, take last 30
    return allActivities.sort((a, b) => b.timestamp - a.timestamp).slice(0, 30);
  });

  // Changed files across all children
  let changedFiles = $derived.by(() => {
    const fileMap = new Map<string, { status: string; agent: string }>();
    for (const child of children) {
      if (child.gitFiles) {
        for (const f of child.gitFiles) {
          fileMap.set(f.path, {
            status: f.status,
            agent: child.name.split(': ').pop() || child.name,
          });
        }
      }
    }
    return [...fileMap.entries()].map(([path, info]) => ({
      path,
      ...info,
    }));
  });

  // Conflicts from hookStore
  let allConflicts = $derived.by(() => {
    const result: { filePath: string; sessions: string[] }[] = [];
    for (const [filePath, sessionIds] of hookStore.conflicts) {
      const relevantIds = sessionIds.filter(
        (id) => id === orchestratorId || children.some((c) => c.id === id),
      );
      if (relevantIds.length > 1) {
        const names = relevantIds.map(
          (id) => sessionStore.sessions.find((s) => s.id === id)?.name.split(': ').pop() || `#${id}`,
        );
        result.push({ filePath, sessions: names });
      }
    }
    return result;
  });

  // Visibility: same absolute-positioning pattern as TerminalView
  let isVisible = $derived(sessionId === sessionStore.activeSessionId);

  function statusLabel(status: SessionStatus): string {
    if (status === 'active') return 'active';
    if (status === 'waiting') return 'waiting';
    return 'idle';
  }

  function toolIcon(name: string): string {
    const icons: Record<string, string> = {
      Write: '~', Edit: '~', Read: '#', Bash: '$',
      Glob: '@', Grep: '/', Agent: '*',
    };
    return icons[name] || '.';
  }

  function shortName(path?: string): string {
    if (!path) return '';
    return path.split('/').pop() || path;
  }

  function formatTime(ts: number): string {
    const d = new Date(ts);
    return `${d.getHours().toString().padStart(2, '0')}:${d.getMinutes().toString().padStart(2, '0')}`;
  }

  // Timeline: bar width as percentage of elapsed time
  function timelineWidth(child: Session): number {
    if (!orchestrator) return 0;
    const elapsed = now - orchestrator.createdAt;
    if (elapsed <= 0) return 0;
    const childElapsed = now - child.createdAt;
    return Math.min(100, Math.max(5, (childElapsed / elapsed) * 100));
  }
</script>

<div
  class="dashboard"
  class:visible={isVisible}
>
  {#if orchestrator}
    <!-- Header -->
    <div class="dash-header">
      <h2 class="dash-title">{orchestrator.name}</h2>
      <div class="dash-meta">
        <span
          class="status-dot"
          class:pulse={orchestrator.status === 'active'}
          style="background: {STATUS_COLORS[orchestrator.status]}"
        ></span>
        <span class="meta-item">{statusLabel(orchestrator.status)}</span>
        <span class="meta-sep">|</span>
        <span class="meta-item">{formatDuration(elapsedMs)}</span>
        {#if totalCost > 0}
          <span class="meta-sep">|</span>
          <span class="meta-item">{formatCost(totalCost)}</span>
        {/if}
        {#if activeCount > 0}
          <span class="meta-sep">|</span>
          <span class="meta-item accent">{activeCount} active</span>
        {/if}
      </div>
    </div>

    <!-- Agents -->
    {#if children.length > 0}
      <section class="dash-section">
        <h3 class="section-title">AGENTS</h3>
        <div class="agents-list">
          {#each children as child (child.id)}
            <button
              class="agent-row"
              class:active={child.id === sessionStore.activeSessionId}
              onclick={() => sessionStore.activate(child.id)}
            >
              <span
                class="status-dot small"
                class:pulse={child.status === 'active'}
                style="background: {STATUS_COLORS[child.status]}"
              ></span>
              <span class="agent-name">{child.name.split(': ').pop() || child.name}</span>
              {#if child.branch}
                <span class="agent-branch">{child.branch}</span>
              {/if}
              <span class="agent-status">{statusLabel(child.status)}</span>
              {#if child.cost}
                <span class="agent-cost">{formatCost(child.cost)}</span>
              {/if}
            </button>
          {/each}
        </div>
      </section>
    {/if}

    <!-- Timeline -->
    {#if children.length > 0}
      <section class="dash-section">
        <h3 class="section-title">TIMELINE</h3>
        <div class="timeline">
          {#each children as child (child.id)}
            <div class="timeline-row">
              <span class="timeline-label">{child.name.split(': ').pop() || child.name}</span>
              <div class="timeline-bar-container">
                <div
                  class="timeline-bar"
                  class:active={child.status === 'active'}
                  style="width: {timelineWidth(child)}%"
                ></div>
              </div>
              <span
                class="status-dot small"
                style="background: {STATUS_COLORS[child.status]}"
              ></span>
            </div>
          {/each}
        </div>
      </section>
    {/if}

    <!-- Activity Feed -->
    {#if activityFeed.length > 0}
      <section class="dash-section">
        <h3 class="section-title">ACTIVITY</h3>
        <div class="activity-feed">
          {#each activityFeed as entry}
            <div class="activity-row">
              <span class="activity-time">{formatTime(entry.timestamp)}</span>
              <span class="activity-agent">{entry.sessionName}</span>
              <span class="activity-tool">{toolIcon(entry.toolName)} {entry.toolName}</span>
              {#if entry.filePath}
                <span class="activity-file">{shortName(entry.filePath)}</span>
              {/if}
            </div>
          {/each}
        </div>
      </section>
    {/if}

    <!-- Changed Files -->
    {#if changedFiles.length > 0}
      <section class="dash-section">
        <h3 class="section-title">CHANGED FILES</h3>
        <div class="files-list">
          {#each changedFiles as file}
            <div class="file-row">
              <span
                class="file-status"
                class:modified={file.status === 'M'}
                class:added={file.status === 'A'}
                class:deleted={file.status === 'D'}
              >{file.status}</span>
              <span class="file-path">{file.path}</span>
              <span class="file-agent">{file.agent}</span>
            </div>
          {/each}
        </div>
      </section>
    {/if}

    <!-- Conflicts -->
    {#if allConflicts.length > 0}
      <section class="dash-section">
        <h3 class="section-title warning">CONFLICTS</h3>
        {#each allConflicts as conflict}
          <div class="conflict-row">
            <span class="conflict-icon">!</span>
            <span class="conflict-file">{conflict.filePath}</span>
            <span class="conflict-agents">{conflict.sessions.join(' & ')}</span>
          </div>
        {/each}
      </section>
    {/if}
  {:else}
    <div class="empty">No orchestrator session found</div>
  {/if}
</div>

<style>
  .dashboard {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    overflow-y: auto;
    padding: 20px 24px;
    background: var(--weplex-bg);
    display: none;
  }

  .dashboard.visible {
    display: block;
  }

  .dash-header {
    margin-bottom: 20px;
    padding-bottom: 12px;
    border-bottom: 1px solid var(--weplex-border);
  }

  .dash-title {
    font-size: 16px;
    font-weight: 600;
    color: var(--weplex-text);
    margin: 0 0 6px 0;
  }

  .dash-meta {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: var(--weplex-text-sm);
    color: var(--weplex-text-muted);
  }

  .meta-sep {
    color: var(--weplex-border);
  }

  .meta-item.accent {
    color: var(--weplex-active);
  }

  .dash-section {
    margin-bottom: 16px;
  }

  .section-title {
    font-size: 10px;
    font-weight: 600;
    color: var(--weplex-text-muted);
    letter-spacing: 0.06em;
    margin-bottom: 8px;
  }

  .section-title.warning {
    color: var(--weplex-warning);
  }

  /* Status dot */
  .status-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .status-dot.small {
    width: 6px;
    height: 6px;
  }

  .status-dot.pulse {
    animation: dot-pulse 1.4s ease-in-out infinite;
  }

  @keyframes dot-pulse {
    0%, 100% { opacity: 1; transform: scale(1); }
    50% { opacity: 0.45; transform: scale(0.75); }
  }

  /* Agents */
  .agents-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .agent-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    border-radius: var(--weplex-radius-sm);
    background: none;
    border: none;
    color: var(--weplex-text);
    font-size: var(--weplex-text-sm);
    cursor: pointer;
    text-align: left;
    width: 100%;
  }

  .agent-row:hover {
    background: var(--weplex-surface-hover);
  }

  .agent-row.active {
    background: var(--weplex-surface-active);
  }

  .agent-name {
    font-weight: 500;
    flex-shrink: 0;
  }

  .agent-branch {
    color: var(--weplex-active);
    font-family: var(--weplex-font-mono);
    font-size: var(--weplex-text-xs);
  }

  .agent-status {
    color: var(--weplex-text-muted);
    margin-left: auto;
    font-size: var(--weplex-text-xs);
  }

  .agent-cost {
    color: var(--weplex-text-muted);
    font-family: var(--weplex-font-mono);
    font-size: var(--weplex-text-xs);
    flex-shrink: 0;
  }

  /* Timeline */
  .timeline {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .timeline-row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: var(--weplex-text-xs);
  }

  .timeline-label {
    width: 100px;
    color: var(--weplex-text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex-shrink: 0;
  }

  .timeline-bar-container {
    flex: 1;
    height: 6px;
    background: var(--weplex-surface-hover);
    border-radius: 3px;
    overflow: hidden;
  }

  .timeline-bar {
    height: 100%;
    background: var(--weplex-text-muted);
    border-radius: 3px;
    transition: width 1s ease;
  }

  .timeline-bar.active {
    background: var(--weplex-accent);
  }

  /* Activity Feed */
  .activity-feed {
    display: flex;
    flex-direction: column;
    gap: 1px;
    max-height: 200px;
    overflow-y: auto;
  }

  .activity-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 3px 0;
    font-size: var(--weplex-text-xs);
    font-family: var(--weplex-font-mono);
  }

  .activity-time {
    color: var(--weplex-text-muted);
    flex-shrink: 0;
    width: 40px;
  }

  .activity-agent {
    color: var(--weplex-accent);
    flex-shrink: 0;
    width: 90px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .activity-tool {
    color: var(--weplex-text);
    flex-shrink: 0;
    width: 70px;
  }

  .activity-file {
    color: var(--weplex-text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Changed Files */
  .files-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .file-row {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: var(--weplex-text-xs);
    font-family: var(--weplex-font-mono);
    padding: 2px 0;
  }

  .file-status {
    width: 14px;
    text-align: center;
    font-weight: 600;
    flex-shrink: 0;
  }

  .file-status.modified { color: var(--weplex-warning); }
  .file-status.added { color: var(--weplex-active); }
  .file-status.deleted { color: var(--weplex-error); }

  .file-path {
    color: var(--weplex-text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .file-agent {
    color: var(--weplex-text-muted);
    margin-left: auto;
    flex-shrink: 0;
  }

  /* Conflicts */
  .conflict-row {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: var(--weplex-text-xs);
    padding: 3px 0;
  }

  .conflict-icon {
    color: var(--weplex-warning);
    font-weight: 700;
    width: 14px;
    text-align: center;
  }

  .conflict-file {
    color: var(--weplex-text);
    font-family: var(--weplex-font-mono);
  }

  .conflict-agents {
    color: var(--weplex-text-muted);
    margin-left: auto;
  }

  .empty {
    padding: 40px;
    text-align: center;
    color: var(--weplex-text-muted);
    font-size: var(--weplex-text-sm);
  }
</style>

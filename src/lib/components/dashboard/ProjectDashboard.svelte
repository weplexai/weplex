<script lang="ts">
  import type { Session } from '../../types';
  import { STATUS_COLORS } from '../../types';
  import { sessionStore } from '../../stores/sessionStore';
  import { hookStore } from '../../stores/hookStore.svelte';
  import { formatCost } from '../../utils/time';
  import { shortPath } from '../../utils/path';

  let {
    sessionId,
  }: {
    sessionId: number;
  } = $props();

  let dashSession = $derived(sessionStore.sessions.find((s) => s.id === sessionId));
  let cwd = $derived(dashSession?.cwd || '');

  // All sessions working in the same cwd (across all spaces)
  let projectSessions = $derived(
    sessionStore.sessions.filter(
      (s) => s.cwd === cwd && s.type !== 'dashboard' && s.id !== sessionId,
    ),
  );

  // Stats
  let activeCount = $derived(projectSessions.filter((s) => s.status === 'active').length);
  let waitingCount = $derived(projectSessions.filter((s) => s.status === 'waiting').length);
  let idleCount = $derived(projectSessions.filter((s) => s.status === 'idle').length);
  let totalCost = $derived(projectSessions.reduce((sum, s) => sum + (s.cost || 0), 0));

  // Git info from the first session that has it
  let gitBranch = $derived(projectSessions.find((s) => s.branch)?.branch);
  let allGitFiles = $derived.by(() => {
    const fileMap = new Map<string, string>();
    for (const s of projectSessions) {
      if (s.gitFiles) {
        for (const f of s.gitFiles) {
          fileMap.set(f.path, f.status);
        }
      }
    }
    return [...fileMap.entries()].map(([path, status]) => ({ path, status }));
  });

  // Conflicts
  let conflicts = $derived.by(() => {
    const result: { filePath: string; sessions: string[] }[] = [];
    for (const [filePath, sessionIds] of hookStore.conflicts) {
      const relevant = sessionIds.filter((id) =>
        projectSessions.some((s) => s.id === id),
      );
      if (relevant.length > 1) {
        const names = relevant.map(
          (id) => sessionStore.sessions.find((s) => s.id === id)?.name || `#${id}`,
        );
        result.push({ filePath, sessions: names });
      }
    }
    return result;
  });

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
    <h2 class="dash-title">{shortPath(cwd)}</h2>
    <div class="dash-meta">
      <span class="meta-item">{projectSessions.length} session{projectSessions.length !== 1 ? 's' : ''}</span>
      {#if activeCount > 0}
        <span class="meta-sep">|</span>
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
      {#if totalCost > 0}
        <span class="meta-sep">|</span>
        <span class="meta-item">{formatCost(totalCost)}</span>
      {/if}
    </div>
  </div>

  <!-- Sessions -->
  {#if projectSessions.length > 0}
    <section class="dash-section">
      <h3 class="section-title">SESSIONS</h3>
      <div class="sessions-list">
        {#each projectSessions as s (s.id)}
          <button
            class="session-row"
            class:row-active={s.id === sessionStore.activeSessionId}
            onclick={() => sessionStore.activate(s.id)}
          >
            <span
              class="status-dot"
              class:pulse={s.status === 'active'}
              style="background: {STATUS_COLORS[s.status]}"
            ></span>
            <span class="session-name">{s.name}</span>
            {#if s.branch}
              <span class="session-branch">{s.branch}</span>
            {/if}
            <span class="session-status">{statusLabel(s.status)}</span>
            {#if s.cost}
              <span class="session-cost">{formatCost(s.cost)}</span>
            {/if}
          </button>
        {/each}
      </div>
    </section>
  {/if}

  <!-- Git -->
  {#if gitBranch || allGitFiles.length > 0}
    <section class="dash-section">
      <h3 class="section-title">GIT</h3>
      {#if gitBranch}
        <div class="git-branch">{gitBranch}</div>
      {/if}
      {#if allGitFiles.length > 0}
        <div class="files-list">
          {#each allGitFiles as file}
            <div class="file-row">
              <span
                class="file-status"
                class:modified={file.status === 'M'}
                class:added={file.status === 'A'}
                class:deleted={file.status === 'D'}
              >{file.status}</span>
              <span class="file-path">{file.path}</span>
            </div>
          {/each}
        </div>
      {/if}
    </section>
  {/if}

  <!-- Conflicts -->
  {#if conflicts.length > 0}
    <section class="dash-section">
      <h3 class="section-title warning">CONFLICTS</h3>
      {#each conflicts as conflict}
        <div class="conflict-row">
          <span class="conflict-icon">!</span>
          <span class="conflict-file">{conflict.filePath}</span>
          <span class="conflict-agents">{conflict.sessions.join(' & ')}</span>
        </div>
      {/each}
    </section>
  {/if}

  {#if projectSessions.length === 0}
    <div class="empty">No sessions working in this directory</div>
  {/if}
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
    font-size: 16px;
    font-weight: 600;
    color: var(--weplex-text);
    margin: 0 0 6px 0;
    font-family: var(--weplex-font-mono);
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

  .dash-section { margin-bottom: 16px; }
  .section-title {
    font-size: 10px;
    font-weight: 600;
    color: var(--weplex-text-muted);
    letter-spacing: 0.06em;
    margin-bottom: 8px;
  }
  .section-title.warning { color: var(--weplex-warning); }

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

  .sessions-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .session-row {
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
  .session-row:hover { background: var(--weplex-surface-hover); }
  .session-row.row-active { background: var(--weplex-surface-active); }
  .session-name { font-weight: 500; flex-shrink: 0; }
  .session-branch {
    color: var(--weplex-active);
    font-family: var(--weplex-font-mono);
    font-size: var(--weplex-text-xs);
  }
  .session-status {
    color: var(--weplex-text-muted);
    margin-left: auto;
    font-size: var(--weplex-text-xs);
  }
  .session-cost {
    color: var(--weplex-text-muted);
    font-family: var(--weplex-font-mono);
    font-size: var(--weplex-text-xs);
    flex-shrink: 0;
  }

  .git-branch {
    color: var(--weplex-active);
    font-family: var(--weplex-font-mono);
    font-size: var(--weplex-text-sm);
    margin-bottom: 8px;
  }

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

  .conflict-row {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: var(--weplex-text-xs);
    padding: 3px 0;
  }
  .conflict-icon { color: var(--weplex-warning); font-weight: 700; width: 14px; text-align: center; }
  .conflict-file { color: var(--weplex-text); font-family: var(--weplex-font-mono); }
  .conflict-agents { color: var(--weplex-text-muted); margin-left: auto; }

  .empty {
    padding: 40px;
    text-align: center;
    color: var(--weplex-text-muted);
    font-size: var(--weplex-text-sm);
  }
</style>

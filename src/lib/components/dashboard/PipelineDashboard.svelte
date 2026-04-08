<script lang="ts">
  import type { Session } from '../../types';
  import { STATUS_COLORS } from '../../types';
  import { sessionStore } from '../../stores/sessionStore';
  import { pipelineRunStore } from '../../stores/pipelineRunStore.svelte';
  import { formatCost, formatDuration, formatAbsoluteTime } from '../../utils/time';

  let { sessionId }: { sessionId: number } = $props();

  // All pipeline runs, newest first
  let allRuns = $derived(
    [...pipelineRunStore.runs].sort((a, b) => b.startedAt - a.startedAt),
  );

  // Selected run (default: latest)
  let selectedRunId = $state<string | null>(null);
  let activeRun = $derived(
    selectedRunId
      ? allRuns.find((r) => r.id === selectedRunId) || allRuns[0]
      : allRuns[0],
  );

  // Child sessions mapped to stages
  let stageSessionMap = $derived.by(() => {
    const map = new Map<string, Session>();
    if (!activeRun) return map;
    for (const stage of activeRun.stages) {
      if (stage.sessionId) {
        const session = sessionStore.sessions.find((s) => s.id === stage.sessionId);
        if (session) map.set(stage.name, session);
      }
    }
    return map;
  });

  // Cost per stage (from mapped sessions)
  let stageCosts = $derived.by(() => {
    const costs = new Map<string, number>();
    for (const [name, session] of stageSessionMap) {
      costs.set(name, session.cost || 0);
    }
    return costs;
  });

  let totalCost = $derived(
    [...stageCosts.values()].reduce((sum, c) => sum + c, 0),
  );

  // Total duration
  let now = $state(Date.now());
  $effect(() => {
    const id = setInterval(() => { now = Date.now(); }, 1000);
    return () => clearInterval(id);
  });

  let totalDuration = $derived(
    activeRun
      ? (activeRun.finishedAt || now) - activeRun.startedAt
      : 0,
  );

  // Expand stage for artifact/output detail
  let expandedStage = $state<string | null>(null);

  function stageStatusColor(status: string): string {
    switch (status) {
      case 'running': return 'var(--weplex-accent)';
      case 'completed': return 'var(--weplex-active)';
      case 'failed': return 'var(--weplex-error)';
      case 'skipped': return 'var(--weplex-text-muted)';
      default: return 'var(--weplex-border)';
    }
  }

  function stageIcon(status: string): string {
    switch (status) {
      case 'running': return '●';
      case 'completed': return '✓';
      case 'failed': return '✗';
      case 'skipped': return '—';
      default: return '○';
    }
  }

  // Timeline: compute bar widths relative to total duration
  function stageBarWidth(stage: typeof activeRun.stages[0]): number {
    if (!activeRun || totalDuration <= 0) return 0;
    const duration = stage.status === 'completed' || stage.status === 'failed'
      ? (stage.mcpArtifact ? totalDuration * 0.15 : stageDuration(stage))
      : stage.status === 'running' ? now - activeRun.startedAt : 0;
    return Math.max(2, (duration / totalDuration) * 100);
  }

  function stageDuration(stage: typeof activeRun.stages[0]): number {
    const session = stageSessionMap.get(stage.name);
    if (!session) return 0;
    return (session.lastActivity || now) - session.createdAt;
  }
</script>

<div class="pipeline-dash" class:visible={sessionId === sessionStore.activeSessionId}>
  {#if !activeRun}
    <div class="empty">
      <span class="empty-icon">⚙</span>
      <p>No pipeline runs yet.</p>
      <p class="empty-hint">Start a pipeline from the Agents & Pipelines panel.</p>
    </div>
  {:else}
    <!-- Header -->
    <div class="header">
      <div class="header-left">
        <h2 class="title">{activeRun.pipelineName}</h2>
        <span class="run-status" style="color: {stageStatusColor(activeRun.status)}">{activeRun.status}</span>
      </div>
      <div class="header-stats">
        <div class="stat">
          <span class="stat-label">Duration</span>
          <span class="stat-value">{formatDuration(totalDuration)}</span>
        </div>
        <div class="stat">
          <span class="stat-label">Cost</span>
          <span class="stat-value">{formatCost(totalCost)}</span>
        </div>
        <div class="stat">
          <span class="stat-label">Stages</span>
          <span class="stat-value">{activeRun.stages.length}</span>
        </div>
        <div class="stat">
          <span class="stat-label">Started</span>
          <span class="stat-value">{formatAbsoluteTime(activeRun.startedAt)}</span>
        </div>
      </div>
    </div>

    <!-- Task -->
    <div class="task-bar">
      <span class="task-label">Task</span>
      <span class="task-text">{activeRun.task}</span>
    </div>

    <!-- Flow Visualization -->
    <div class="section">
      <h3 class="section-title">Flow</h3>
      <div class="flow">
        {#each activeRun.stages as stage, i}
          {#if stage.parallel && stage.parallel.length > 0}
            <!-- Parallel group -->
            <div class="flow-parallel">
              {#each stage.parallel as pStage}
                <button
                  class="flow-node"
                  class:expanded={expandedStage === pStage.name}
                  onclick={() => expandedStage = expandedStage === pStage.name ? null : pStage.name}
                >
                  <span class="fn-status" style="color: {stageStatusColor(pStage.status)}">{stageIcon(pStage.status)}</span>
                  <span class="fn-name">{pStage.name}</span>
                  <span class="fn-agent">{pStage.agent}</span>
                  {#if stageCosts.get(pStage.name)}
                    <span class="fn-cost">{formatCost(stageCosts.get(pStage.name) || 0)}</span>
                  {/if}
                </button>
              {/each}
            </div>
          {:else}
            <button
              class="flow-node"
              class:expanded={expandedStage === stage.name}
              onclick={() => expandedStage = expandedStage === stage.name ? null : stage.name}
            >
              <span class="fn-status" class:pulse={stage.status === 'running'} style="color: {stageStatusColor(stage.status)}">{stageIcon(stage.status)}</span>
              <span class="fn-name">{stage.name}</span>
              <span class="fn-agent">{stage.agent}</span>
              {#if stageCosts.get(stage.name)}
                <span class="fn-cost">{formatCost(stageCosts.get(stage.name) || 0)}</span>
              {/if}
            </button>
          {/if}
          {#if i < activeRun.stages.length - 1}
            <span class="flow-arrow">→</span>
          {/if}
        {/each}
      </div>
    </div>

    <!-- Expanded Stage Detail -->
    {#if expandedStage}
      {@const stage = activeRun.stages.find((s) => s.name === expandedStage)
        || activeRun.stages.flatMap((s) => s.parallel || []).find((s) => s.name === expandedStage)}
      {#if stage}
        <div class="stage-detail">
          <div class="sd-header">
            <span class="sd-name">{stage.name}</span>
            <span class="sd-agent">{stage.agent}</span>
            <span class="sd-status" style="color: {stageStatusColor(stage.status)}">{stage.status}</span>
            {#if stageDuration(stage) > 0}
              <span class="sd-dur">{formatDuration(stageDuration(stage))}</span>
            {/if}
            {#if stageCosts.get(stage.name)}
              <span class="sd-cost">{formatCost(stageCosts.get(stage.name) || 0)}</span>
            {/if}
          </div>
          {#if stage.mcpArtifact}
            <div class="sd-artifact">
              <span class="sd-label">Artifact</span>
              <pre class="sd-pre">{stage.mcpArtifact}</pre>
            </div>
          {/if}
          {#if stage.role}
            <div class="sd-role">
              <span class="sd-label">Role</span>
              <span>{stage.role}</span>
            </div>
          {/if}
        </div>
      {/if}
    {/if}

    <!-- Timeline (Gantt) -->
    <div class="section">
      <h3 class="section-title">Timeline</h3>
      <div class="timeline">
        {#each activeRun.stages as stage}
          {#if stage.parallel && stage.parallel.length > 0}
            {#each stage.parallel as pStage}
              <div class="tl-row">
                <span class="tl-label">{pStage.name}</span>
                <div class="tl-track">
                  <div
                    class="tl-bar"
                    style="width: {stageBarWidth(pStage)}%; background: {stageStatusColor(pStage.status)}"
                  ></div>
                </div>
                <span class="tl-dur">{stageDuration(pStage) > 0 ? formatDuration(stageDuration(pStage)) : '—'}</span>
              </div>
            {/each}
          {:else}
            <div class="tl-row">
              <span class="tl-label">{stage.name}</span>
              <div class="tl-track">
                <div
                  class="tl-bar"
                  class:pulse-bar={stage.status === 'running'}
                  style="width: {stageBarWidth(stage)}%; background: {stageStatusColor(stage.status)}"
                ></div>
              </div>
              <span class="tl-dur">{stageDuration(stage) > 0 ? formatDuration(stageDuration(stage)) : '—'}</span>
            </div>
          {/if}
        {/each}
      </div>
    </div>

    <!-- Cost Breakdown -->
    {#if totalCost > 0}
      <div class="section">
        <h3 class="section-title">Cost breakdown</h3>
        <div class="cost-grid">
          {#each activeRun.stages as stage}
            {#if stageCosts.get(stage.name)}
              <div class="cost-row">
                <span class="cost-name">{stage.name}</span>
                <span class="cost-agent">{stage.agent}</span>
                <div class="cost-bar-wrap">
                  <div class="cost-bar" style="width: {totalCost > 0 ? ((stageCosts.get(stage.name) || 0) / totalCost) * 100 : 0}%"></div>
                </div>
                <span class="cost-val">{formatCost(stageCosts.get(stage.name) || 0)}</span>
              </div>
            {/if}
          {/each}
          <div class="cost-row cost-total">
            <span class="cost-name">Total</span>
            <span class="cost-agent"></span>
            <div class="cost-bar-wrap"></div>
            <span class="cost-val">{formatCost(totalCost)}</span>
          </div>
        </div>
      </div>
    {/if}

    <!-- Run History -->
    {#if allRuns.length > 1}
      <div class="section">
        <h3 class="section-title">History</h3>
        <div class="history">
          {#each allRuns.slice(0, 10) as run}
            <button
              class="hist-row"
              class:hist-active={run.id === activeRun.id}
              onclick={() => selectedRunId = run.id}
            >
              <span class="hist-status" style="color: {stageStatusColor(run.status)}">{stageIcon(run.status)}</span>
              <span class="hist-name">{run.pipelineName}</span>
              <span class="hist-task">{run.task.slice(0, 40)}</span>
              <span class="hist-time">{formatAbsoluteTime(run.startedAt)}</span>
            </button>
          {/each}
        </div>
      </div>
    {/if}
  {/if}
</div>

<style>
  .pipeline-dash {
    position: absolute;
    inset: 0;
    display: none;
    overflow-y: auto;
    padding: 20px 24px;
    font-family: var(--weplex-font);
    color: var(--weplex-text);
    background: var(--weplex-bg);
  }
  .pipeline-dash.visible { display: block; }

  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    gap: 8px;
    color: var(--weplex-text-muted);
  }
  .empty-icon { font-size: 32px; opacity: 0.3; }
  .empty-hint { font-size: 12px; }

  /* Header */
  .header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    margin-bottom: 16px;
  }
  .title {
    font-size: 18px;
    font-weight: 600;
    margin: 0;
  }
  .run-status {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .header-stats {
    display: flex;
    gap: 20px;
  }
  .stat { display: flex; flex-direction: column; align-items: flex-end; }
  .stat-label { font-size: 10px; color: var(--weplex-text-muted); text-transform: uppercase; letter-spacing: 0.05em; }
  .stat-value { font-size: 14px; font-weight: 600; font-variant-numeric: tabular-nums; }

  /* Task */
  .task-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    background: var(--weplex-surface);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-md);
    margin-bottom: 20px;
    font-size: 12px;
  }
  .task-label {
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--weplex-text-muted);
    flex-shrink: 0;
  }
  .task-text { color: var(--weplex-text); }

  /* Sections */
  .section { margin-bottom: 20px; }
  .section-title {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--weplex-text-muted);
    margin-bottom: 10px;
  }

  /* Flow */
  .flow {
    display: flex;
    align-items: center;
    gap: 8px;
    overflow-x: auto;
    padding: 8px 0;
  }
  .flow-node {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 10px 14px;
    background: var(--weplex-surface);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-md);
    cursor: pointer;
    flex-shrink: 0;
    min-width: 100px;
    transition: border-color 0.15s, background 0.15s;
    color: var(--weplex-text);
    font-family: inherit;
    font-size: inherit;
    text-align: left;
  }
  .flow-node:hover { border-color: var(--weplex-accent); }
  .flow-node.expanded { border-color: var(--weplex-accent); background: var(--weplex-surface-hover); }
  .fn-status { font-size: 14px; }
  .fn-name { font-size: 12px; font-weight: 600; }
  .fn-agent { font-size: 10px; color: var(--weplex-text-muted); }
  .fn-cost { font-size: 10px; color: var(--weplex-accent); font-variant-numeric: tabular-nums; }
  .flow-arrow { color: var(--weplex-text-muted); font-size: 14px; flex-shrink: 0; }
  .flow-parallel {
    display: flex;
    flex-direction: column;
    gap: 4px;
    border: 1px dashed var(--weplex-border);
    border-radius: var(--weplex-radius-md);
    padding: 6px;
  }

  .pulse { animation: dot-pulse 1.4s ease-in-out infinite; }
  @keyframes dot-pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.4; }
  }

  /* Stage detail */
  .stage-detail {
    background: var(--weplex-surface);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-md);
    padding: 12px 16px;
    margin-bottom: 20px;
  }
  .sd-header {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 8px;
  }
  .sd-name { font-weight: 600; font-size: 13px; }
  .sd-agent { font-size: 11px; color: var(--weplex-text-muted); }
  .sd-status { font-size: 11px; font-weight: 600; text-transform: uppercase; }
  .sd-dur, .sd-cost { font-size: 11px; color: var(--weplex-text-muted); font-variant-numeric: tabular-nums; }
  .sd-label {
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--weplex-text-muted);
    margin-bottom: 4px;
    display: block;
  }
  .sd-pre {
    font-family: var(--weplex-font-mono);
    font-size: 11px;
    color: var(--weplex-text);
    background: var(--weplex-bg);
    padding: 8px 10px;
    border-radius: var(--weplex-radius-sm);
    overflow-x: auto;
    white-space: pre-wrap;
    word-break: break-word;
    max-height: 200px;
  }
  .sd-artifact, .sd-role { margin-top: 8px; }
  .sd-role span { font-size: 12px; }

  /* Timeline */
  .timeline { display: flex; flex-direction: column; gap: 6px; }
  .tl-row {
    display: grid;
    grid-template-columns: 100px 1fr 50px;
    align-items: center;
    gap: 8px;
  }
  .tl-label {
    font-size: 11px;
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tl-track {
    height: 16px;
    background: var(--weplex-surface);
    border-radius: var(--weplex-radius-sm);
    overflow: hidden;
  }
  .tl-bar {
    height: 100%;
    border-radius: var(--weplex-radius-sm);
    transition: width 0.3s ease;
    min-width: 2px;
  }
  .tl-bar.pulse-bar { animation: bar-pulse 1.4s ease-in-out infinite; }
  @keyframes bar-pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.5; }
  }
  .tl-dur {
    font-size: 10px;
    color: var(--weplex-text-muted);
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  /* Cost breakdown */
  .cost-grid { display: flex; flex-direction: column; gap: 6px; }
  .cost-row {
    display: grid;
    grid-template-columns: 100px 80px 1fr 60px;
    align-items: center;
    gap: 8px;
    font-size: 12px;
  }
  .cost-name { font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .cost-agent { font-size: 10px; color: var(--weplex-text-muted); }
  .cost-bar-wrap {
    height: 8px;
    background: var(--weplex-surface);
    border-radius: 4px;
    overflow: hidden;
  }
  .cost-bar {
    height: 100%;
    background: var(--weplex-accent);
    border-radius: 4px;
    min-width: 2px;
  }
  .cost-val {
    text-align: right;
    font-variant-numeric: tabular-nums;
    font-weight: 500;
  }
  .cost-total {
    border-top: 1px solid var(--weplex-border);
    padding-top: 6px;
    margin-top: 2px;
  }
  .cost-total .cost-name { font-weight: 700; }
  .cost-total .cost-val { font-weight: 700; }

  /* History */
  .history { display: flex; flex-direction: column; gap: 2px; }
  .hist-row {
    display: grid;
    grid-template-columns: 20px 120px 1fr 80px;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    border-radius: var(--weplex-radius-sm);
    cursor: pointer;
    font-size: 12px;
    transition: background 0.15s;
    border: none;
    background: none;
    color: var(--weplex-text);
    font-family: inherit;
    text-align: left;
  }
  .hist-row:hover { background: var(--weplex-surface); }
  .hist-row.hist-active { background: var(--weplex-surface-hover); }
  .hist-status { text-align: center; }
  .hist-name { font-weight: 500; }
  .hist-task { color: var(--weplex-text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .hist-time { font-size: 10px; color: var(--weplex-text-muted); text-align: right; }
</style>

<script lang="ts">
  import { collabPipelineStore } from '../../stores/collabPipelineStore.svelte';
  import { authStore } from '../../stores/authStore.svelte';
  import { stageStatusIcon, stageStatusClass } from '../../utils/pipelineStatus';
  import { ChevronDown, ChevronRight, Link } from 'lucide-svelte';
  import type { CollaborativeRun, CollaborativeStageInfo, CollaborativeStageStatus } from '../../types';

  let collapsed = $state(false);

  let runs = $derived(collabPipelineStore.runs);
  let activeRuns = $derived(
    runs.filter((r) => r.status === 'running' || r.status === 'pending'),
  );
  let recentRuns = $derived(
    runs
      .filter((r) => r.status !== 'running' && r.status !== 'pending')
      .slice(-3),
  );
  let visibleRuns = $derived([...activeRuns, ...recentRuns]);
  let currentUserId = $derived(authStore.user?.id ?? null);

  // Count stages waiting for current user
  let waitingCount = $derived(
    activeRuns.reduce((sum, run) => {
      return sum + run.stages.filter(
        (s) => s.status === 'waiting' && s.ownerId === currentUserId,
      ).length;
    }, 0),
  );

  function ownerLabel(stage: CollaborativeStageInfo): string {
    if (!stage.ownerEmail) return '';
    if (stage.ownerId === currentUserId) return '(you)';
    // Short email: alice@team.com -> alice
    return stage.ownerEmail.split('@')[0] || stage.ownerEmail;
  }

  function selectRun(run: CollaborativeRun) {
    collabPipelineStore.setActiveRun(run.id);
  }

  function shortId(id: string): string {
    return '#' + id.slice(0, 4);
  }
</script>

{#if visibleRuns.length > 0}
  <div class="collab-section">
    <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
    <div class="collab-header" onclick={() => (collapsed = !collapsed)}>
      <span class="collab-chevron">
        {#if collapsed}<ChevronRight size={12} />{:else}<ChevronDown size={12} />{/if}
      </span>
      <Link size={12} class="collab-icon" />
      <span class="collab-label">Team Pipelines</span>
      <span class="collab-badge">
        {activeRuns.length}
        {#if waitingCount > 0}
          <span class="waiting-dot"></span>
        {/if}
      </span>
    </div>

    {#if !collapsed}
      <div class="collab-runs">
        {#each visibleRuns as run (run.id)}
          {@const isActive = collabPipelineStore.activeRunId === run.id}
          {@const completedCount = run.stages.filter((s) => s.status === 'completed').length}
          <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
          <div
            class="collab-run"
            class:active={isActive}
            class:has-waiting={run.stages.some(
              (s) => s.status === 'waiting' && s.ownerId === currentUserId,
            )}
            onclick={() => selectRun(run)}
          >
            <div class="run-top">
              <span class="run-name">{run.pipelineName}</span>
              <span class="run-id">{shortId(run.id)}</span>
            </div>
            <div class="run-stages">
              {#each run.stages as stage}
                <span
                  class="mini-stage {stageStatusClass(stage.status)}"
                  title="{stage.name} {ownerLabel(stage)}"
                >
                  <span class="mini-icon">{stageStatusIcon(stage.status)}</span>
                  {#if stage.status === 'running' || stage.status === 'waiting'}
                    <span class="mini-owner">{ownerLabel(stage)}</span>
                  {/if}
                </span>
              {/each}
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>
{/if}

<style>
  .collab-section {
    margin: 4px 8px 8px;
  }

  .collab-header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 6px;
    cursor: pointer;
    border-radius: var(--weplex-radius-sm);
    transition: background var(--weplex-duration-fast);
  }

  .collab-header:hover {
    background: var(--weplex-surface-hover);
  }

  .collab-chevron {
    color: var(--weplex-text-muted);
    display: flex;
  }

  :global(.collab-icon) {
    color: var(--weplex-info);
  }

  .collab-label {
    flex: 1;
    font-size: 11px;
    font-weight: 600;
    color: var(--weplex-text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .collab-badge {
    font-size: 10px;
    color: var(--weplex-text-muted);
    font-family: var(--weplex-font-mono);
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .waiting-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--weplex-warning);
    animation: pulse-dot 1.5s ease-in-out infinite;
  }

  @keyframes pulse-dot {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.3; }
  }

  .collab-runs {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding-top: 2px;
  }

  .collab-run {
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-md);
    padding: 6px 8px;
    cursor: pointer;
    background: color-mix(in srgb, var(--weplex-info) 3%, transparent);
    transition: all var(--weplex-duration-fast);
  }

  .collab-run:hover {
    background: color-mix(in srgb, var(--weplex-info) 8%, transparent);
    border-color: var(--weplex-border-active);
  }

  .collab-run.active {
    border-color: var(--weplex-info);
    background: color-mix(in srgb, var(--weplex-info) 10%, transparent);
  }

  .collab-run.has-waiting {
    border-color: color-mix(in srgb, var(--weplex-warning) 40%, var(--weplex-border));
  }

  .run-top {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    margin-bottom: 4px;
  }

  .run-name {
    font-size: 11px;
    font-weight: 600;
    color: var(--weplex-text);
    font-family: var(--weplex-font-mono);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .run-id {
    font-size: 10px;
    color: var(--weplex-text-muted);
    font-family: var(--weplex-font-mono);
    flex-shrink: 0;
  }

  .run-stages {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .mini-stage {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    font-size: 10px;
    font-family: var(--weplex-font-mono);
  }

  .mini-icon {
    font-weight: 700;
    width: 12px;
    text-align: center;
  }

  .mini-owner {
    color: var(--weplex-text-muted);
    font-size: 9px;
  }

  .mini-stage.completed .mini-icon {
    color: var(--weplex-active);
  }

  .mini-stage.running .mini-icon {
    color: var(--weplex-accent);
    animation: pulse-stage 1.2s ease-in-out infinite;
  }

  .mini-stage.waiting .mini-icon {
    color: var(--weplex-warning);
  }

  .mini-stage.failed .mini-icon {
    color: var(--weplex-error);
  }

  .mini-stage.skipped .mini-icon,
  .mini-stage.pending .mini-icon {
    color: var(--weplex-text-muted);
    opacity: 0.4;
  }

  @keyframes pulse-stage {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.3; }
  }
</style>

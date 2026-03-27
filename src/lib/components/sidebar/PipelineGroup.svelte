<script lang="ts">
  import type { StageStatus } from '../../types';
  import { pipelineRunStore } from '../../stores/pipelineRunStore.svelte';
  import { sessionStore } from '../../stores/sessionStore.svelte';
  import { ChevronDown, ChevronRight, X, Workflow, SkipForward } from 'lucide-svelte';

  let { run }: { run: (typeof pipelineRunStore.runs)[0] } = $props();

  let collapsed = $state(false);
  let completedCount = $derived(run.stages.filter((s) => s.status === 'completed').length);

  function statusIcon(status: StageStatus): string {
    switch (status) {
      case 'completed':
        return '✓';
      case 'running':
        return '●';
      case 'failed':
        return '✗';
      case 'skipped':
        return '–';
      default:
        return '○';
    }
  }

  function statusClass(status: StageStatus): string {
    return status;
  }
</script>

<div
  class="pipeline-group"
  class:failed={run.status === 'failed'}
  class:completed={run.status === 'completed'}
>
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <div class="pg-header" onclick={() => (collapsed = !collapsed)}>
    <span class="pg-chevron">
      {#if collapsed}<ChevronRight size={12} />{:else}<ChevronDown size={12} />{/if}
    </span>
    <span class="pg-icon"><Workflow size={12} /></span>
    <span class="pg-name">{run.pipelineName}</span>
    <span class="pg-count">{completedCount}/{run.stages.length}</span>
    {#if run.status === 'running'}
      <button
        class="pg-cancel"
        title="Cancel pipeline"
        onclick={(e) => {
          e.stopPropagation();
          pipelineRunStore.cancelRun(run.id);
        }}
      >
        <X size={11} />
      </button>
    {/if}
  </div>

  {#if !collapsed}
    <div class="pg-stages">
      {#each run.stages as stage, i}
        <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
        <div
          class="pg-stage"
          class:active={stage.sessionId !== null &&
            sessionStore.activeSessionId === stage.sessionId}
          onclick={() => {
            if (stage.sessionId) sessionStore.activate(stage.sessionId);
          }}
        >
          <span class="pg-status {statusClass(stage.status)}">{statusIcon(stage.status)}</span>
          <span class="pg-stage-name">{stage.agent || stage.name}</span>
          {#if stage.status === 'running' && run.currentStageIndex === i}
            <button
              class="pg-next-btn"
              title="Complete & advance to next stage"
              onclick={(e) => {
                e.stopPropagation();
                pipelineRunStore.advanceCurrentStage(run.id);
              }}
            >
              <SkipForward size={10} />
            </button>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .pipeline-group {
    margin: 4px 8px;
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-md);
    background: color-mix(in srgb, var(--weplex-accent) 4%, transparent);
    overflow: hidden;
  }
  .pipeline-group.failed {
    border-color: color-mix(in srgb, var(--weplex-error) 30%, var(--weplex-border));
  }
  .pipeline-group.completed {
    border-color: color-mix(in srgb, var(--weplex-active) 30%, var(--weplex-border));
  }

  .pg-header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    cursor: pointer;
    transition: background var(--weplex-duration-fast);
  }
  .pg-header:hover {
    background: var(--weplex-surface-hover);
  }

  .pg-chevron {
    color: var(--weplex-text-muted);
    display: flex;
  }
  .pg-icon {
    color: var(--weplex-accent);
    display: flex;
  }
  .pg-name {
    flex: 1;
    font-size: 12px;
    font-weight: 600;
    color: var(--weplex-text);
    font-family: var(--weplex-font-mono);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .pg-count {
    font-size: 10px;
    color: var(--weplex-text-muted);
    font-family: var(--weplex-font-mono);
  }
  .pg-cancel {
    border: none;
    background: transparent;
    color: var(--weplex-text-muted);
    cursor: pointer;
    padding: 2px;
    border-radius: 3px;
    display: flex;
  }
  .pg-cancel:hover {
    color: var(--weplex-error);
    background: color-mix(in srgb, var(--weplex-error) 10%, transparent);
  }

  .pg-stages {
    padding: 2px 6px 6px;
  }

  .pg-stage {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 8px;
    border-radius: var(--weplex-radius-sm);
    cursor: pointer;
    transition: background var(--weplex-duration-fast);
  }
  .pg-stage:hover {
    background: var(--weplex-surface-hover);
  }
  .pg-stage.active {
    background: color-mix(in srgb, var(--weplex-accent) 10%, transparent);
  }

  .pg-status {
    font-size: 10px;
    font-weight: 700;
    width: 14px;
    text-align: center;
    flex-shrink: 0;
  }
  .pg-status.completed {
    color: var(--weplex-active);
  }
  .pg-status.running {
    color: var(--weplex-accent);
    animation: pulse 1.2s ease-in-out infinite;
  }
  .pg-status.failed {
    color: var(--weplex-error);
  }
  .pg-status.skipped {
    color: var(--weplex-text-muted);
  }
  .pg-status.pending {
    color: var(--weplex-text-muted);
    opacity: 0.4;
  }

  @keyframes pulse {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.3;
    }
  }

  .pg-stage-name {
    flex: 1;
    font-size: 11px;
    color: var(--weplex-text-secondary);
    font-family: var(--weplex-font-mono);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .pg-next-btn {
    border: 1px solid var(--weplex-border);
    background: transparent;
    color: var(--weplex-text-muted);
    cursor: pointer;
    padding: 2px 4px;
    border-radius: 3px;
    display: flex;
    align-items: center;
    font-size: 9px;
    transition: all var(--weplex-duration-fast);
  }
  .pg-next-btn:hover {
    border-color: var(--weplex-accent);
    color: var(--weplex-accent);
  }
</style>

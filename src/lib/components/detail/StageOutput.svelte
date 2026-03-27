<script lang="ts">
  import { pipelineRunStore } from '../../stores/pipelineRunStore.svelte';
  import type { PipelineRunInfo, StageRunInfo, StageStatus } from '../../types';
  import { Workflow, ChevronDown } from 'lucide-svelte';

  let run = $derived(pipelineRunStore.activeRun);
  let allStages = $derived(run ? flattenStages(run.stages) : []);
  let selectedStage = $state<string | null>(null);

  function flattenStages(stages: StageRunInfo[]): StageRunInfo[] {
    const flat: StageRunInfo[] = [];
    for (const s of stages) {
      if (s.parallel_group) {
        for (const ps of s.parallel_group) flat.push(ps);
      } else {
        flat.push(s);
      }
    }
    return flat;
  }

  function statusLabel(status: StageStatus): string {
    switch (status) {
      case 'completed':
        return '✓ Completed';
      case 'running':
        return '● Running';
      case 'failed':
        return '✗ Failed';
      case 'skipped':
        return '– Skipped';
      default:
        return '○ Pending';
    }
  }

  function statusClass(status: StageStatus): string {
    return status;
  }

  let selectedOutput = $derived(
    run && selectedStage ? pipelineRunStore.getStageOutput(run.id, selectedStage) : '',
  );

  // Auto-select first running or last completed stage
  $effect(() => {
    if (!run || selectedStage) return;
    const running = allStages.find((s) => s.state.status === 'running');
    if (running) {
      selectedStage = running.name;
      return;
    }
    const completed = [...allStages].reverse().find((s) => s.state.status === 'completed');
    if (completed) selectedStage = completed.name;
  });
</script>

{#if run}
  <div class="stage-output">
    <div class="so-header">
      <Workflow size={14} />
      <span class="so-title">{run.pipeline_name}</span>
      <span class="so-status {run.status}">{run.status}</span>
    </div>

    <div class="so-task">{run.task}</div>

    <!-- Stage selector -->
    <div class="so-stages">
      {#each allStages as stage}
        <button
          class="so-stage-btn"
          class:active={selectedStage === stage.name}
          class:completed={stage.state.status === 'completed'}
          class:running={stage.state.status === 'running'}
          class:failed={stage.state.status === 'failed'}
          onclick={() => (selectedStage = stage.name)}
        >
          <span class="so-stage-name">{stage.agent || stage.name}</span>
          {#if stage.state.duration_ms}
            <span class="so-stage-dur">{(stage.state.duration_ms / 1000).toFixed(0)}s</span>
          {/if}
        </button>
      {/each}
    </div>

    <!-- Output view -->
    {#if selectedStage}
      {@const stageInfo = allStages.find((s) => s.name === selectedStage)}
      <div class="so-output-header">
        <span>{selectedStage}</span>
        {#if stageInfo}
          <span class="so-output-status {statusClass(stageInfo.state.status)}"
            >{statusLabel(stageInfo.state.status)}</span
          >
        {/if}
      </div>
      <pre class="so-output">{selectedOutput || '(waiting for output...)'}</pre>
    {/if}
  </div>
{:else}
  <div class="so-empty">No active pipeline run</div>
{/if}

<style>
  .stage-output {
    display: flex;
    flex-direction: column;
    gap: 10px;
    height: 100%;
  }

  .so-header {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--weplex-accent);
  }

  .so-title {
    font-size: 13px;
    font-weight: 600;
    font-family: var(--weplex-font-mono);
    color: var(--weplex-text);
  }

  .so-status {
    font-size: 10px;
    font-weight: 500;
    font-family: var(--weplex-font-mono);
    padding: 1px 6px;
    border-radius: 3px;
    margin-left: auto;
  }
  .so-status.running {
    color: var(--weplex-accent);
  }
  .so-status.completed {
    color: var(--weplex-active);
  }
  .so-status.failed {
    color: var(--weplex-error);
  }

  .so-task {
    font-size: 11px;
    color: var(--weplex-text-muted);
    line-height: 1.4;
  }

  .so-stages {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .so-stage-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 3px 8px;
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-sm);
    background: transparent;
    color: var(--weplex-text-muted);
    font-size: 10px;
    font-family: var(--weplex-font-mono);
    cursor: pointer;
    transition: all var(--weplex-duration-fast);
  }
  .so-stage-btn:hover {
    border-color: var(--weplex-text-muted);
  }
  .so-stage-btn.active {
    border-color: var(--weplex-accent);
    color: var(--weplex-accent);
    background: color-mix(in srgb, var(--weplex-accent) 8%, transparent);
  }
  .so-stage-btn.completed {
    color: var(--weplex-active);
  }
  .so-stage-btn.running {
    color: var(--weplex-accent);
  }
  .so-stage-btn.failed {
    color: var(--weplex-error);
  }

  .so-stage-name {
    font-weight: 500;
  }
  .so-stage-dur {
    opacity: 0.5;
  }

  .so-output-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 11px;
    font-weight: 600;
    font-family: var(--weplex-font-mono);
    color: var(--weplex-text-secondary);
  }

  .so-output-status {
    font-size: 10px;
    font-weight: 500;
  }
  .so-output-status.completed {
    color: var(--weplex-active);
  }
  .so-output-status.running {
    color: var(--weplex-accent);
  }
  .so-output-status.failed {
    color: var(--weplex-error);
  }
  .so-output-status.pending {
    color: var(--weplex-text-muted);
  }
  .so-output-status.skipped {
    color: var(--weplex-text-muted);
  }

  .so-output {
    flex: 1;
    font-size: 11px;
    font-family: var(--weplex-font-mono);
    color: var(--weplex-text-secondary);
    background: var(--weplex-bg);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-sm);
    padding: 8px 10px;
    overflow-y: auto;
    white-space: pre-wrap;
    word-break: break-word;
    line-height: 1.5;
    margin: 0;
  }

  .so-empty {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: var(--weplex-text-muted);
    font-size: 12px;
  }
</style>

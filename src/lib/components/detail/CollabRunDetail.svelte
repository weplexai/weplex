<script lang="ts">
  import { collabPipelineStore } from '../../stores/collabPipelineStore.svelte';
  import { authStore } from '../../stores/authStore.svelte';
  import { timeAgo } from '../../utils/time';
  import { stageStatusIcon, stageStatusClass } from '../../utils/pipelineStatus';
  import { Workflow, Eye, Play, X, ChevronDown, ChevronRight } from 'lucide-svelte';
  import type { CollaborativeRun, CollaborativeStageInfo, CollaborativeStageStatus } from '../../types';

  let run = $derived(collabPipelineStore.activeRun);
  let currentUserId = $derived(authStore.user?.id ?? null);
  let isInitiator = $derived(run ? run.initiatorId === currentUserId : false);

  // Artifact viewer state
  let viewingArtifact = $state<string | null>(null);
  let claimingStage = $state<string | null>(null);
  let cancellingRun = $state(false);

  function statusLabel(status: CollaborativeStageStatus): string {
    switch (status) {
      case 'completed': return 'Completed';
      case 'running': return 'Running';
      case 'waiting': return 'Waiting';
      case 'failed': return 'Failed';
      case 'skipped': return 'Skipped';
      default: return 'Pending';
    }
  }

  function isMyStage(stage: CollaborativeStageInfo): boolean {
    return stage.ownerId === currentUserId;
  }

  function ownerDisplay(stage: CollaborativeStageInfo): string {
    if (!stage.ownerEmail) return 'unassigned';
    if (stage.ownerId === currentUserId) return 'you';
    return stage.ownerEmail;
  }

  function formatRunStatus(run: CollaborativeRun): string {
    switch (run.status) {
      case 'running': return 'Running';
      case 'pending': return 'Pending';
      case 'completed': return 'Completed';
      case 'failed': return 'Failed';
      case 'cancelled': return 'Cancelled';
      default: return run.status;
    }
  }

  function runStatusClass(run: CollaborativeRun): string {
    return run.status;
  }

  function hasArtifact(stage: CollaborativeStageInfo): boolean {
    return run != null && !!run.artifacts[stage.name];
  }

  function toggleArtifact(stageName: string) {
    viewingArtifact = viewingArtifact === stageName ? null : stageName;
  }

  async function handleClaimStage(stageName: string) {
    if (!run) return;
    claimingStage = stageName;
    try {
      await collabPipelineStore.claimStage(run.id, stageName);
    } catch (e: unknown) {
      console.error('Claim stage failed:', e);
    } finally {
      claimingStage = null;
    }
  }

  async function handleCancelRun() {
    if (!run) return;
    cancellingRun = true;
    try {
      await collabPipelineStore.cancelRun(run.id);
    } catch (e: unknown) {
      console.error('Cancel run failed:', e);
    } finally {
      cancellingRun = false;
    }
  }

  function stageDuration(stage: CollaborativeStageInfo): string | null {
    if (!stage.startedAt || !stage.finishedAt) return null;
    const ms = new Date(stage.finishedAt).getTime() - new Date(stage.startedAt).getTime();
    if (ms < 60000) return `${Math.round(ms / 1000)}s`;
    return `${Math.round(ms / 60000)} min`;
  }
</script>

{#if run}
  <section class="section">
    <h3 class="section-title">
      <Workflow size={11} />
      {run.pipelineName}
    </h3>
    <div class="field">
      <span class="field-label">Status</span>
      <span class="field-value status-val {runStatusClass(run)}">{formatRunStatus(run)}</span>
    </div>
    <div class="field">
      <span class="field-label">Started</span>
      <span class="field-value">{timeAgo(new Date(run.createdAt).getTime())}</span>
    </div>
    {#if run.task}
      <div class="task-block">
        <span class="task-label">Task</span>
        <p class="task-text">{run.task}</p>
      </div>
    {/if}
  </section>

  <section class="section">
    <h3 class="section-title">STAGES</h3>
    {#each run.stages as stage}
      {@const myStage = isMyStage(stage)}
      {@const isWaitingForMe = stage.status === 'waiting' && myStage}
      {@const duration = stageDuration(stage)}
      <div class="stage-item" class:my-waiting={isWaitingForMe}>
        <div class="stage-row">
          <span class="stage-status {stageStatusClass(stage.status)}">
            {stageStatusIcon(stage.status)}
          </span>
          <div class="stage-info">
            <span class="stage-name">
              {stage.name}
              {#if stage.role}
                <span class="stage-role">({stage.role})</span>
              {/if}
            </span>
            <span class="stage-owner">
              {ownerDisplay(stage)}
              {#if duration}
                <span class="stage-duration">&middot; {duration}</span>
              {/if}
            </span>
            <span class="stage-status-text">{statusLabel(stage.status)}</span>
          </div>
        </div>

        {#if hasArtifact(stage)}
          <button class="stage-action-btn" onclick={() => toggleArtifact(stage.name)}>
            {#if viewingArtifact === stage.name}
              <ChevronDown size={11} />
            {:else}
              <ChevronRight size={11} />
            {/if}
            <Eye size={11} />
            Artifact
          </button>
          {#if viewingArtifact === stage.name}
            <pre class="artifact-text">{run.artifacts[stage.name]}</pre>
          {/if}
        {/if}

        {#if isWaitingForMe}
          <button
            class="stage-action-btn primary"
            onclick={() => handleClaimStage(stage.name)}
            disabled={claimingStage === stage.name}
          >
            <Play size={11} />
            {claimingStage === stage.name ? 'Starting...' : 'Start Stage'}
          </button>
        {/if}
      </div>
    {/each}
  </section>

  {#if isInitiator && (run.status === 'running' || run.status === 'pending')}
    <section class="section cancel-section">
      <button
        class="cancel-btn"
        onclick={handleCancelRun}
        disabled={cancellingRun}
      >
        <X size={12} />
        {cancellingRun ? 'Cancelling...' : 'Cancel Run'}
      </button>
    </section>
  {/if}
{:else}
  <div class="empty">No collaborative run selected</div>
{/if}

<style>
  .section {
    margin-bottom: 16px;
  }

  .section-title {
    font-size: 10px;
    font-weight: 600;
    color: var(--weplex-text-muted);
    letter-spacing: 0.06em;
    margin-bottom: 8px;
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .field {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    padding: 3px 0;
    font-size: var(--weplex-text-sm);
  }

  .field-label {
    color: var(--weplex-text-muted);
  }

  .field-value {
    color: var(--weplex-text);
  }

  .status-val {
    font-weight: 600;
    font-size: var(--weplex-text-xs);
    padding: 1px 6px;
    border-radius: var(--weplex-radius-full);
  }

  .status-val.running {
    background: color-mix(in srgb, var(--weplex-accent) 15%, transparent);
    color: var(--weplex-accent);
  }

  .status-val.pending {
    background: color-mix(in srgb, var(--weplex-info) 15%, transparent);
    color: var(--weplex-info);
  }

  .status-val.completed {
    background: color-mix(in srgb, var(--weplex-active) 15%, transparent);
    color: var(--weplex-active);
  }

  .status-val.failed,
  .status-val.cancelled {
    background: color-mix(in srgb, var(--weplex-error) 15%, transparent);
    color: var(--weplex-error);
  }

  .task-block {
    margin-top: 8px;
  }

  .task-label {
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
    display: block;
    margin-bottom: 4px;
  }

  .task-text {
    font-size: var(--weplex-text-sm);
    color: var(--weplex-text-secondary);
    line-height: 1.4;
    margin: 0;
    padding: 6px 8px;
    background: var(--weplex-surface);
    border-radius: var(--weplex-radius-sm);
    border: 1px solid var(--weplex-border);
  }

  /* Stage items */
  .stage-item {
    padding: 6px 0;
    border-bottom: 1px solid var(--weplex-border);
  }

  .stage-item:last-child {
    border-bottom: none;
  }

  .stage-item.my-waiting {
    background: color-mix(in srgb, var(--weplex-warning) 5%, transparent);
    margin: 0 -8px;
    padding: 6px 8px;
    border-radius: var(--weplex-radius-sm);
    border-bottom: none;
  }

  .stage-row {
    display: flex;
    gap: 8px;
    align-items: flex-start;
  }

  .stage-status {
    font-size: 12px;
    font-weight: 700;
    width: 16px;
    text-align: center;
    flex-shrink: 0;
    margin-top: 1px;
  }

  .stage-status.completed { color: var(--weplex-active); }
  .stage-status.running {
    color: var(--weplex-accent);
    animation: pulse-status 1.2s ease-in-out infinite;
  }
  .stage-status.waiting { color: var(--weplex-warning); }
  .stage-status.failed { color: var(--weplex-error); }
  .stage-status.skipped,
  .stage-status.pending {
    color: var(--weplex-text-muted);
    opacity: 0.5;
  }

  @keyframes pulse-status {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.3; }
  }

  .stage-info {
    flex: 1;
    min-width: 0;
  }

  .stage-name {
    font-size: var(--weplex-text-sm);
    font-weight: 500;
    color: var(--weplex-text);
    display: block;
  }

  .stage-role {
    font-weight: 400;
    color: var(--weplex-text-muted);
    font-size: var(--weplex-text-xs);
  }

  .stage-owner {
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
    font-family: var(--weplex-font-mono);
    display: block;
    margin-top: 1px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .stage-duration {
    color: var(--weplex-text-muted);
    opacity: 0.7;
  }

  .stage-status-text {
    font-size: 10px;
    color: var(--weplex-text-muted);
    display: block;
    margin-top: 1px;
  }

  /* Action buttons */
  .stage-action-btn {
    margin-top: 4px;
    margin-left: 24px;
    padding: 3px 8px;
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-sm);
    background: transparent;
    color: var(--weplex-text-secondary);
    font-size: var(--weplex-text-xs);
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    transition: all var(--weplex-duration-fast);
  }

  .stage-action-btn:hover {
    background: var(--weplex-surface-hover);
    border-color: var(--weplex-border-active);
  }

  .stage-action-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .stage-action-btn.primary {
    border-color: var(--weplex-accent);
    color: var(--weplex-accent);
  }

  .stage-action-btn.primary:hover {
    background: color-mix(in srgb, var(--weplex-accent) 10%, transparent);
  }

  /* Artifact text */
  .artifact-text {
    margin: 6px 0 0 24px;
    padding: 8px;
    background: var(--weplex-bg);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-sm);
    font-family: var(--weplex-font-mono);
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-secondary);
    white-space: pre-wrap;
    word-break: break-word;
    max-height: 200px;
    overflow-y: auto;
    line-height: 1.4;
  }

  /* Cancel section */
  .cancel-section {
    padding-top: 8px;
    border-top: 1px solid var(--weplex-border);
  }

  .cancel-btn {
    padding: 4px 10px;
    border: 1px solid var(--weplex-error);
    border-radius: var(--weplex-radius-sm);
    background: transparent;
    color: var(--weplex-error);
    font-size: var(--weplex-text-xs);
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    transition: all var(--weplex-duration-fast);
  }

  .cancel-btn:hover {
    background: rgba(239, 68, 68, 0.1);
  }

  .cancel-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .empty {
    padding: 24px;
    text-align: center;
    font-size: var(--weplex-text-sm);
    color: var(--weplex-text-muted);
  }
</style>

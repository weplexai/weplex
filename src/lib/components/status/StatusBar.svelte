<script lang="ts">
  import { sessionStore } from '../../stores/sessionStore';
  import { uiStore } from '../../stores/uiStore';
  import { formatCost } from '../../utils/time';
  import { updateState, installUpdate } from '../../utils/updater';

  let stats = $derived(sessionStore.stats);
</script>

<footer class="status-bar">
  <div class="status-left">
    {#if stats.active > 0}
      <span class="stat active">● {stats.active} active</span>
    {/if}
    {#if stats.idle > 0}
      <span class="stat">○ {stats.idle} idle</span>
    {/if}
    {#if stats.total > 0 && stats.active === 0 && stats.idle === 0}
      <span class="stat">{stats.total} session{stats.total === 1 ? '' : 's'}</span>
    {/if}
    {#if stats.totalCost > 0}
      <span class="stat cost">{formatCost(stats.totalCost)} today</span>
    {/if}
  </div>
  <div class="status-right">
    {#if updateState.available}
      <button class="update-btn" onclick={installUpdate} disabled={updateState.downloading}>
        {#if updateState.downloading}
          Updating... {updateState.progress}%
        {:else}
          v{updateState.version} available — click to update
        {/if}
      </button>
    {/if}
    <button class="palette-hint" onclick={() => uiStore.openOverlay('command-palette')}>
      ⌘K
    </button>
  </div>
</footer>

<style>
  .status-bar {
    height: var(--weplex-status-height);
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 12px;
    background: transparent;
    border-top: 1px solid var(--weplex-border);
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
    flex-shrink: 0;
  }

  .status-left {
    display: flex;
    gap: 10px;
    align-items: center;
  }

  .status-right {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .stat.active {
    color: var(--weplex-active);
  }
  .stat.cost {
    color: var(--weplex-warning);
  }

  .palette-hint {
    padding: 2px 6px;
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-sm);
    background: transparent;
    color: var(--weplex-text-muted);
    font-size: 10px;
    font-family: var(--weplex-font-mono);
    transition: all var(--weplex-duration-fast) var(--weplex-easing);
  }

  .palette-hint:hover {
    border-color: var(--weplex-accent);
    color: var(--weplex-accent);
  }

  .update-btn {
    padding: 2px 8px;
    border: 1px solid var(--weplex-accent);
    border-radius: var(--weplex-radius-sm);
    background: transparent;
    color: var(--weplex-accent);
    font-size: 10px;
    font-family: var(--weplex-font-mono);
    cursor: pointer;
    transition: all var(--weplex-duration-fast) var(--weplex-easing);
  }

  .update-btn:hover:not(:disabled) {
    background: var(--weplex-accent);
    color: var(--weplex-text-inverse);
  }

  .update-btn:disabled {
    opacity: 0.7;
    cursor: default;
  }
</style>

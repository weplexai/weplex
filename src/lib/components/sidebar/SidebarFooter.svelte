<script lang="ts">
  import { Layers, Settings } from 'lucide-svelte';
  import { uiStore } from '../../stores/uiStore';
</script>

<div class="footer">
  <div class="footer-actions">
    <button
      class="icon-btn"
      title="Agents & Pipelines (⌘⇧A)"
      onclick={() => uiStore.openOverlay('agents')}
    >
      <Layers size={15} />
    </button>
    <button class="icon-btn" title="Settings (⌘,)" onclick={() => uiStore.openOverlay('settings')}>
      <Settings size={15} />
    </button>
  </div>
  <button class="new-session" onclick={() => uiStore.openOverlay('new-session')}>
    + New Session
  </button>
</div>

<style>
  .footer {
    padding: 8px;
    border-top: 1px solid var(--weplex-border);
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .footer-actions {
    display: flex;
    gap: 2px;
    justify-content: flex-end;
  }

  .icon-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    border-radius: var(--weplex-radius-sm);
    border: none;
    background: transparent;
    color: var(--weplex-text-muted);
    cursor: pointer;
    transition:
      background 0.1s,
      color 0.1s;
  }

  .icon-btn:hover {
    background: var(--weplex-surface-hover);
    color: var(--weplex-text);
  }

  .new-session {
    width: 100%;
    padding: 8px;
    border: 1px dashed var(--weplex-border);
    border-radius: var(--weplex-radius-md);
    background: transparent;
    color: var(--weplex-text-muted);
    font-size: var(--weplex-text-sm);
    transition: all var(--weplex-duration-fast) var(--weplex-easing);
  }

  .new-session:hover {
    border-color: var(--weplex-accent);
    color: var(--weplex-accent);
    background: color-mix(in srgb, var(--weplex-accent) 5%, transparent);
  }
</style>

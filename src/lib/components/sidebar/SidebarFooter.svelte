<script lang="ts">
  import { Layers, Settings, User } from 'lucide-svelte';
  import { uiStore } from '../../stores/uiStore';
  import { authStore } from '../../stores/authStore.svelte';

  function openAccount() {
    uiStore.enterHubMode('account');
  }
</script>

<div class="footer">
  <div class="footer-actions">
    <button
      class="icon-btn"
      title="Agents & Pipelines (⌘⇧A)"
      onclick={() => uiStore.enterHubMode('agents')}
    >
      <Layers size={15} />
    </button>
    <button class="icon-btn" title="Settings (⌘,)" onclick={() => uiStore.enterHubMode('settings')}>
      <Settings size={15} />
    </button>
    <button
      class="icon-btn account-btn"
      class:signed-in={authStore.isAuthenticated}
      title={authStore.isAuthenticated
        ? authStore.user?.displayName || authStore.user?.email || 'Account'
        : 'Sign In'}
      onclick={openAccount}
    >
      {#if authStore.isAuthenticated}
        <span class="avatar-initial"
          >{(authStore.user?.displayName || authStore.user?.email || '?')[0].toUpperCase()}</span
        >
      {:else}
        <User size={15} />
      {/if}
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

  .account-btn.signed-in {
    color: var(--weplex-accent);
  }

  .avatar-initial {
    font-size: 11px;
    font-weight: 700;
    line-height: 1;
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

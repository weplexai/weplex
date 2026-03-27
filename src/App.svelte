<script lang="ts">
  import './styles.css';
  import { onMount, untrack } from 'svelte';
  import Sidebar from './lib/components/sidebar/Sidebar.svelte';

  import SplitContainer from './lib/components/terminal/SplitContainer.svelte';
  import TerminalView from './lib/components/terminal/TerminalView.svelte';
  import DetailPanel from './lib/components/detail/DetailPanel.svelte';

  import CommandPalette from './lib/components/overlays/CommandPalette.svelte';
  import NewSessionDialog from './lib/components/overlays/NewSessionDialog.svelte';
  import SpaceModal from './lib/components/overlays/SpaceModal.svelte';
  import Settings from './lib/components/overlays/Settings.svelte';
  import AgentsPipelines from './lib/components/overlays/AgentsPipelines.svelte';
  import { sessionStore } from './lib/stores/sessionStore';
  import { spaceStore } from './lib/stores/spaceStore';
  import { uiStore } from './lib/stores/uiStore';
  import { splitStore } from './lib/stores/splitStore';
  import { HYPERSPACE_ID } from './lib/types';
  import { handleGlobalKeydown } from './lib/utils/shortcuts';
  import { checkForUpdates } from './lib/utils/updater';

  onMount(() => {
    if (sessionStore.sessions.length === 0) {
      sessionStore.create({ name: 'terminal' });
    }

    window.addEventListener('keydown', handleGlobalKeydown);

    // Check for updates after a short delay, then every 10 minutes
    const updateTimer = setTimeout(checkForUpdates, 3000);
    const updateInterval = setInterval(checkForUpdates, 10 * 60 * 1000);

    return () => {
      window.removeEventListener('keydown', handleGlobalKeydown);
      clearTimeout(updateTimer);
      clearInterval(updateInterval);
    };
  });

  let activeSession = $derived(sessionStore.activeSession);
  let spaceBgColor = $derived(spaceStore.activeSpace.bgColor || null);
  let activeSpaceId = $derived(spaceStore.activeSpaceId);
  // Ensure layout exists (mutation — must be in $effect, not $derived)
  $effect(() => {
    splitStore.ensureLayout(activeSpaceId);
  });

  let splitLayout = $derived(splitStore.getLayout(activeSpaceId));

  // Sync active session changes to split store (sidebar clicks)
  // Only place the session in layout if it belongs to this space (prevents cross-space loop)
  $effect(() => {
    const activeId = sessionStore.activeSessionId;
    const spaceId = spaceStore.activeSpaceId;
    if (activeId !== null) {
      if (
        spaceId === HYPERSPACE_ID ||
        sessionStore.sessions.find((s) => s.id === activeId)?.spaceId === spaceId
      ) {
        splitStore.ensureSession(spaceId, activeId);
      }
    }
  });

  // Reconcile: remove split panes for sessions that no longer exist or belong to another space
  // Skip in Hyperspace — all sessions are valid there
  $effect(() => {
    const spaceId = spaceStore.activeSpaceId;
    if (spaceId === HYPERSPACE_ID) return;
    const allSessionIds = new Set(sessionStore.sessions.map((s) => s.id));
    const spaceSessionIds = new Set(sessionStore.getBySpace(spaceId).map((s) => s.id));
    // Untrack layout reads: removeSession writes to layouts, which would re-trigger
    // this effect (read→write→read→write loop). Session changes are the real trigger.
    const visible = untrack(() => splitStore.getVisibleSessionIds(spaceId));
    for (const sid of visible) {
      if (!allSessionIds.has(sid) || !spaceSessionIds.has(sid)) {
        splitStore.removeSession(spaceId, sid);
      }
    }
  });
</script>

<div
  class="app"
  style={spaceBgColor
    ? `background: color-mix(in srgb, ${spaceBgColor} 15%, var(--weplex-sidebar-bg))`
    : ''}
>
  {#if uiStore.activeOverlay === 'agents'}
    <AgentsPipelines />
  {:else}
    <Sidebar />

    {#if uiStore.sidebarHidden}
      <button class="sidebar-reveal" onclick={() => uiStore.showSidebar()}>
        <span class="sidebar-reveal-hint">⌘B</span>
      </button>
    {/if}

    <div class="main" class:with-detail={uiStore.detailPanelOpen}>
      <div class="terminal-area">
        {#if splitLayout}
          <SplitContainer node={splitLayout} spaceId={activeSpaceId} />
        {/if}

        <button
          class="detail-btn"
          class:active={uiStore.detailPanelOpen}
          onclick={() => uiStore.toggleDetailPanel()}
          title="Detail panel (⌘.)">ⓘ</button
        >

        {#if !activeSession}
          <div class="empty-state">
            <div class="empty-prompt">
              <span class="prompt-symbol">›</span>
              <span class="prompt-cursor"></span>
            </div>
            <div class="empty-shortcuts">
              <button class="shortcut-card" onclick={() => uiStore.openOverlay('new-session')}>
                <kbd>⌘N</kbd>
                <span>New Session</span>
              </button>
              <button class="shortcut-card" onclick={() => uiStore.openOverlay('command-palette')}>
                <kbd>⌘K</kbd>
                <span>Command Palette</span>
              </button>
            </div>
          </div>
        {/if}
      </div>
    </div>

    {#if uiStore.detailPanelOpen}
      <DetailPanel session={activeSession} />
    {/if}
  {/if}

  <!-- Terminal instances live outside the conditional so they survive overlay switches
       (AgentsPipelines replaces the {:else} block, which would destroy all terminals) -->
  <div id="terminal-host">
    {#each sessionStore.sessions as session (session.id)}
      <TerminalView sessionId={session.id} />
    {/each}
  </div>
</div>

<!-- Overlays -->
{#if uiStore.activeOverlay === 'command-palette'}
  <CommandPalette mode="full" />
{:else if uiStore.activeOverlay === 'quick-switcher'}
  <CommandPalette mode="sessions" />
{:else if uiStore.activeOverlay === 'new-session'}
  <NewSessionDialog />
{:else if uiStore.activeOverlay === 'space-modal'}
  <SpaceModal />
{:else if uiStore.activeOverlay === 'settings'}
  <Settings />
{/if}

<style>
  .app {
    display: flex;
    height: 100%;
    width: 100%;
    position: relative;
    background: var(--weplex-sidebar-bg);
    transition: background 0.3s ease;
  }

  .main {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    position: relative;
    z-index: 0;
    margin: 6px 6px 6px 0;
    border-radius: 10px;
    box-shadow:
      0 0 0 1px rgba(0, 0, 0, 0.4),
      0 2px 8px rgba(0, 0, 0, 0.3);
    background: var(--weplex-bg);
    overflow: hidden;
    transition: background 0.3s ease;
  }

  .terminal-area {
    flex: 1;
    position: relative;
    min-height: 0;
    overflow: hidden;
    background: var(--weplex-bg);
  }

  .detail-btn {
    position: absolute;
    top: 8px;
    right: 22px;
    z-index: 10;
    width: 26px;
    height: 26px;
    border: none;
    border-radius: var(--weplex-radius-sm);
    background: transparent;
    color: var(--weplex-text-muted);
    font-size: 15px;
    display: flex;
    align-items: center;
    justify-content: center;
    opacity: 0;
    transition:
      opacity var(--weplex-duration-fast) var(--weplex-easing),
      background var(--weplex-duration-fast) var(--weplex-easing),
      color var(--weplex-duration-fast) var(--weplex-easing);
  }

  .main:hover .detail-btn {
    opacity: 1;
  }

  .detail-btn:hover {
    background: var(--weplex-surface-hover);
    color: var(--weplex-text);
  }

  .detail-btn.active {
    opacity: 1;
    color: var(--weplex-accent);
  }

  .main.with-detail {
    margin-right: 3px;
  }

  #terminal-host {
    position: absolute;
    width: 0;
    height: 0;
    overflow: hidden;
    pointer-events: none;
  }

  .empty-state {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 32px;
    animation: empty-fade-in 0.4s ease-out;
  }

  @keyframes empty-fade-in {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  .empty-prompt {
    display: flex;
    align-items: center;
    gap: 2px;
  }

  .prompt-symbol {
    font-size: 32px;
    font-weight: 300;
    color: var(--weplex-accent);
    opacity: 0.5;
    font-family: var(--weplex-font-mono);
  }

  .prompt-cursor {
    width: 2px;
    height: 28px;
    background: var(--weplex-accent);
    opacity: 0.6;
    animation: cursor-blink 1.2s steps(2) infinite;
  }

  @keyframes cursor-blink {
    0%,
    100% {
      opacity: 0.6;
    }
    50% {
      opacity: 0;
    }
  }

  .empty-shortcuts {
    display: flex;
    gap: 12px;
  }

  .shortcut-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    width: 140px;
    padding: 16px 12px;
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-lg);
    background: transparent;
    color: var(--weplex-text-muted);
    font-size: var(--weplex-text-sm);
    cursor: pointer;
    transition: all var(--weplex-duration-fast) var(--weplex-easing);
  }

  .shortcut-card:hover {
    border-color: var(--weplex-accent);
    color: var(--weplex-text);
    background: color-mix(in srgb, var(--weplex-accent) 5%, transparent);
  }

  .shortcut-card kbd {
    padding: 4px 10px;
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-sm);
    background: var(--weplex-surface);
    font-family: var(--weplex-font-mono);
    font-size: var(--weplex-text-sm);
    font-weight: 600;
    color: var(--weplex-accent);
    transition: all var(--weplex-duration-fast) var(--weplex-easing);
  }

  .shortcut-card:hover kbd {
    border-color: var(--weplex-accent);
    background: color-mix(in srgb, var(--weplex-accent) 10%, transparent);
  }

  .shortcut-card span {
    font-family: var(--weplex-font-sans, system-ui);
  }

  .sidebar-reveal {
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    width: 8px;
    z-index: 9999;
    border: none;
    background: transparent;
    padding: 0;
    cursor: pointer;
    transition:
      width var(--weplex-duration-fast) var(--weplex-easing),
      background var(--weplex-duration-fast) var(--weplex-easing);
  }

  .sidebar-reveal:hover {
    width: 36px;
    background: var(--weplex-surface-hover);
    border-right: 1px solid var(--weplex-border);
  }

  .sidebar-reveal-hint {
    opacity: 0;
    position: absolute;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    font-size: 11px;
    font-weight: 600;
    font-family: var(--weplex-font-mono);
    color: var(--weplex-accent);
    white-space: nowrap;
    pointer-events: none;
    transition: opacity var(--weplex-duration-fast) var(--weplex-easing);
  }

  .sidebar-reveal:hover .sidebar-reveal-hint {
    opacity: 1;
  }
</style>

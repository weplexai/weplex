<script lang="ts">
  import type { Session } from '../../types';
  import { spaceStore } from '../../stores/spaceStore.svelte';
  import { uiStore } from '../../stores/uiStore';
  import SpaceChat from './SpaceChat.svelte';
  import { chatStore } from '../../stores/chatStore.svelte';
  import { featureFlags } from '../../stores/featureFlagsStore.svelte';
  import ProjectSection from './ProjectSection.svelte';
  import CommandsSection from './CommandsSection.svelte';
  import InfoTab from './InfoTab.svelte';
  import TimelineTab from './TimelineTab.svelte';

  let { session }: { session: Session | undefined } = $props();

  // Resize state — handle is on the LEFT edge of the panel, so dragging
  // left grows the width (inverse of left sidebar).
  let isResizing = $state(false);
  let resizeStartX = 0;
  let resizeStartWidth = 0;
  let pendingResizeWidth: number | null = null;
  let resizeRaf: number | null = null;

  function handleResizeStart(e: PointerEvent) {
    e.preventDefault();
    isResizing = true;
    resizeStartX = e.clientX;
    resizeStartWidth = uiStore.detailPanelWidth;
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
  }

  function handleResizeMove(e: PointerEvent) {
    if (!isResizing) return;
    const delta = resizeStartX - e.clientX;
    pendingResizeWidth = resizeStartWidth + delta;

    if (resizeRaf !== null) return;
    resizeRaf = requestAnimationFrame(() => {
      resizeRaf = null;
      if (pendingResizeWidth === null || !isResizing) return;
      uiStore.setDetailPanelWidth(pendingResizeWidth);
      pendingResizeWidth = null;
    });
  }

  function handleResizeEnd() {
    isResizing = false;
  }

  let space = $derived(
    session ? spaceStore.spaces.find((s) => s.id === session.spaceId) : undefined,
  );

  // Tabs — Info always visible, Timeline for agent sessions (private activity
  // journal), Chat for shared/team spaces, Project/Cmds for agent sessions.
  let isSharedSpace = $derived(space?.shared === true || space?.type === 'team');
  let hasTimeline = $derived(session?.type === 'agent');
  let hasProject = $derived(session?.type === 'agent' && !!session?.cwd);
  let hasCommands = $derived(session?.type === 'agent' && featureFlags.commands);
  let activeTab = $state<'info' | 'timeline' | 'chat' | 'project' | 'cmds'>('info');
  let chatUnread = $derived(space?.serverId ? chatStore.getUnread(space.serverId) : 0);

  // Reset tab only when switching to a DIFFERENT session (not on status/prop updates)
  let prevSessionId = $state<number | undefined>(undefined);
  $effect(() => {
    const currentId = session?.id;
    if (currentId !== prevSessionId) {
      prevSessionId = currentId;
      activeTab = 'info';
    }
  });
</script>

<aside
  class="detail-panel"
  class:resizing={isResizing}
  style="width: {uiStore.detailPanelWidth}px; min-width: {uiStore.detailPanelWidth}px"
>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="resize-handle"
    onpointerdown={handleResizeStart}
    onpointermove={handleResizeMove}
    onpointerup={handleResizeEnd}
    onpointercancel={handleResizeEnd}
  ></div>
  {#if session}
    <!-- Tab switcher — visible when multiple tabs available -->
    {#if hasTimeline || isSharedSpace || hasProject || hasCommands}
      <div class="tab-bar">
        <button
          class="tab-btn"
          class:active={activeTab === 'info'}
          onclick={() => (activeTab = 'info')}
        >
          Info
        </button>
        {#if hasTimeline}
          <button
            class="tab-btn"
            class:active={activeTab === 'timeline'}
            onclick={() => (activeTab = 'timeline')}
          >
            Timeline
          </button>
        {/if}
        {#if hasProject}
          <button
            class="tab-btn"
            class:active={activeTab === 'project'}
            onclick={() => (activeTab = 'project')}
          >
            Project
          </button>
        {/if}
        {#if hasCommands}
          <button
            class="tab-btn"
            class:active={activeTab === 'cmds'}
            onclick={() => (activeTab = 'cmds')}
          >
            Cmds
          </button>
        {/if}
        {#if isSharedSpace}
          <button
            class="tab-btn"
            class:active={activeTab === 'chat'}
            onclick={() => (activeTab = 'chat')}
          >
            Chat
            {#if chatUnread > 0}
              <span class="unread-badge">{chatUnread > 99 ? '99+' : chatUnread}</span>
            {/if}
          </button>
        {/if}
      </div>
    {/if}

    {#if activeTab === 'timeline' && hasTimeline && session}
      <TimelineTab sessionId={session.id} />
    {:else if activeTab === 'chat' && isSharedSpace && space?.serverId}
      <SpaceChat serverId={space.serverId} sessionId={session?.id} />
    {:else if activeTab === 'project' && hasProject && session?.cwd}
      <ProjectSection cwd={session.cwd} />
    {:else if activeTab === 'cmds' && hasCommands && session}
      <CommandsSection {session} />
    {:else}
      <InfoTab {session} />
    {/if}
  {:else}
    <div class="empty">No session selected</div>
  {/if}
</aside>

<style>
  .detail-panel {
    position: relative;
    margin: 9px 9px 9px 3px;
    border-radius: 10px;
    box-shadow:
      0 0 0 1px rgba(0, 0, 0, 0.4),
      0 2px 8px rgba(0, 0, 0, 0.3);
    background: var(--weplex-bg);
    overflow-y: auto;
    padding: 16px;
    flex-shrink: 0;
  }

  .detail-panel:not(.resizing) {
    transition:
      width var(--weplex-duration-normal) var(--weplex-easing),
      min-width var(--weplex-duration-normal) var(--weplex-easing);
  }

  .resize-handle {
    position: absolute;
    left: -3px;
    top: 0;
    bottom: 0;
    width: 6px;
    cursor: col-resize;
    z-index: 30;
  }

  .resize-handle:hover,
  .resize-handle:active {
    background: var(--weplex-accent);
    opacity: 0.3;
  }

  .empty {
    padding: 24px;
    text-align: center;
    font-size: var(--weplex-text-sm);
    color: var(--weplex-text-muted);
  }

  .tab-bar {
    display: flex;
    gap: 2px;
    margin-bottom: 12px;
    border-bottom: 1px solid var(--weplex-border);
    padding-bottom: 0;
  }

  .tab-btn {
    flex: 1;
    padding: 6px 0;
    background: none;
    border: none;
    border-bottom: 2px solid transparent;
    color: var(--weplex-text-muted);
    font-size: var(--weplex-text-xs);
    font-weight: 600;
    letter-spacing: 0.04em;
    cursor: pointer;
    text-transform: uppercase;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 4px;
  }

  .tab-btn:hover {
    color: var(--weplex-text);
  }

  .tab-btn.active {
    color: var(--weplex-text);
    border-bottom-color: var(--weplex-accent);
  }

  .unread-badge {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 16px;
    height: 16px;
    padding: 0 4px;
    border-radius: 8px;
    background: var(--weplex-accent);
    color: var(--weplex-bg);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0;
    text-transform: none;
  }
</style>

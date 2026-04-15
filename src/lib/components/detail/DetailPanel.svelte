<script lang="ts">
  import type { Session } from '../../types';
  import { spaceStore } from '../../stores/spaceStore.svelte';
  import SpaceChat from './SpaceChat.svelte';
  import { chatStore } from '../../stores/chatStore.svelte';
  import ProjectSection from './ProjectSection.svelte';
  import CommandsSection from './CommandsSection.svelte';
  import InfoTab from './InfoTab.svelte';

  let { session }: { session: Session | undefined } = $props();

  let space = $derived(
    session ? spaceStore.spaces.find((s) => s.id === session.spaceId) : undefined,
  );

  // Tabs — Info always visible, Chat for shared/team spaces, Project/Cmds for agent sessions
  let isSharedSpace = $derived(space?.shared === true || space?.type === 'team');
  let hasProject = $derived(session?.type === 'agent' && !!session?.cwd);
  let hasCommands = $derived(session?.type === 'agent');
  let activeTab = $state<'info' | 'chat' | 'project' | 'cmds'>('info');
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

<aside class="detail-panel">
  {#if session}
    <!-- Tab switcher — visible when multiple tabs available -->
    {#if isSharedSpace || hasProject || hasCommands}
      <div class="tab-bar">
        <button
          class="tab-btn"
          class:active={activeTab === 'info'}
          onclick={() => (activeTab = 'info')}
        >
          Info
        </button>
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

    {#if activeTab === 'chat' && isSharedSpace && space?.serverId}
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
    width: var(--weplex-detail-width);
    min-width: var(--weplex-detail-width);
    margin: 9px 9px 9px 3px;
    border-radius: 10px;
    box-shadow:
      0 0 0 1px rgba(0, 0, 0, 0.4),
      0 2px 8px rgba(0, 0, 0, 0.3);
    background: var(--weplex-bg);
    overflow-y: auto;
    padding: 16px;
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

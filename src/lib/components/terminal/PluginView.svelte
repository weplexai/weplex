<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { sessionStore } from '../../stores/sessionStore';
  import { pluginStore } from '../../stores/pluginStore.svelte';

  let {
    sessionId,
    pluginId,
  }: {
    sessionId: number;
    pluginId: string;
  } = $props();

  let container: HTMLDivElement;
  let destroyed = false;

  let isVisible = $derived(sessionId === sessionStore.activeSessionId);

  onMount(() => {
    const module = pluginStore.getLoadedModule(pluginId);
    if (module?.sessionType?.render && container) {
      const session = sessionStore.sessions.find((s) => s.id === sessionId);
      if (session) {
        module.sessionType.render(container, session);
      }
    }
  });

  onDestroy(() => {
    destroyed = true;
    const module = pluginStore.getLoadedModule(pluginId);
    if (module?.sessionType?.destroy) {
      module.sessionType.destroy(String(sessionId));
    }
  });
</script>

<div class="plugin-view" class:visible={isVisible}>
  <div class="plugin-container" bind:this={container}></div>
</div>

<style>
  .plugin-view {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    display: none;
  }

  .plugin-view.visible {
    display: flex;
  }

  .plugin-container {
    flex: 1;
    overflow: hidden;
  }
</style>

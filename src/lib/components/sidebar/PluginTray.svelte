<script lang="ts">
  import { pluginStore } from '../../stores/pluginStore.svelte';

  let activePlugins = $derived(pluginStore.activePlugins);
  let activePanelId = $derived(pluginStore.activePanelId);

  function togglePlugin(pluginId: string) {
    pluginStore.togglePanel(pluginId);
  }

  function getIcon(icon: string): string {
    // Support emoji and lucide icon names
    if (icon.length <= 2) return icon; // emoji
    // Map common lucide names to unicode
    const iconMap: Record<string, string> = {
      globe: '🌐',
      database: '🗄️',
      monitor: '📊',
      'file-text': '📝',
      plug: '🔌',
      docker: '🐳',
      'scroll-text': '📋',
    };
    return iconMap[icon] || '🧩';
  }
</script>

{#if activePlugins.length > 0}
  <div class="plugin-tray">
    {#each activePlugins.slice(0, 8) as plugin (plugin.manifest.id)}
      <button
        class="tray-icon"
        class:active={activePanelId === plugin.manifest.id}
        onclick={() => togglePlugin(plugin.manifest.id)}
        title={plugin.manifest.name}
      >
        {getIcon(plugin.manifest.icon)}
      </button>
    {/each}
  </div>
{/if}

<style>
  .plugin-tray {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 4px;
    padding: 6px 8px;
    border-top: 1px solid var(--weplex-border);
  }

  .tray-icon {
    width: 28px;
    height: 28px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: var(--weplex-radius-sm);
    background: none;
    border: none;
    font-size: 16px;
    cursor: pointer;
    opacity: 0.6;
    transition: opacity 0.15s, background 0.15s;
  }

  .tray-icon:hover {
    opacity: 1;
    background: var(--weplex-surface-hover);
  }

  .tray-icon.active {
    opacity: 1;
    background: var(--weplex-surface-active);
  }
</style>

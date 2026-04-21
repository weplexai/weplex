<script lang="ts">
  import { uiStore } from '../../stores/uiStore';
  import { featureFlags } from '../../stores/featureFlagsStore.svelte';
  import { invoke } from '@tauri-apps/api/core';
  import {
    Bot,
    Store,
    LayoutGrid,
    Settings,
    User,
    ArrowLeft,
    Zap,
  } from 'lucide-svelte';
  import type { HubSection } from '../../types';

  // Resize
  let isResizing = $state(false);
  let resizeStartX = 0;
  let resizeStartWidth = 0;

  function handleResizeStart(e: PointerEvent) {
    e.preventDefault();
    isResizing = true;
    resizeStartX = e.clientX;
    resizeStartWidth = uiStore.sidebarWidthRaw;
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
  }

  function handleResizeMove(e: PointerEvent) {
    if (!isResizing) return;
    const delta = e.clientX - resizeStartX;
    uiStore.setSidebarWidth(resizeStartWidth + delta);
  }

  function handleResizeEnd() {
    isResizing = false;
  }

  // Swipe left to exit hub
  import { onMount, onDestroy } from 'svelte';
  let swipeAccum = 0;
  let swipeCooldown = 0;
  let swipeTimer: ReturnType<typeof setTimeout>;

  function handleSwipe(e: WheelEvent) {
    if (Math.abs(e.deltaX) <= Math.abs(e.deltaY)) return;
    if (Math.abs(e.deltaX) < 2) return;
    if (Date.now() < swipeCooldown) return;

    // Accumulate only rightward swipes (deltaX > 0)
    if (e.deltaX > 0) {
      swipeAccum += e.deltaX;
    } else {
      swipeAccum = 0;
      return;
    }

    if (swipeAccum > 100) {
      swipeCooldown = Date.now() + 600;
      swipeAccum = 0;
      uiStore.exitHubMode();
      return;
    }

    clearTimeout(swipeTimer);
    swipeTimer = setTimeout(() => { swipeAccum = 0; }, 200);
  }

  onMount(() => {
    window.addEventListener('wheel', handleSwipe, { passive: true });
  });
  onDestroy(() => {
    window.removeEventListener('wheel', handleSwipe);
  });

  // Show traffic lights in hub mode too
  $effect(() => {
    invoke('set_traffic_lights_visible', { visible: true }).catch(() => {});
  });

  // Primary sections (features). Resources/Commands/Marketplace are gated
  // behind feature flags — hidden for alpha until each feature is ready.
  const mainSections = $derived(
    [
      featureFlags.resources ? { id: 'resources' as HubSection, label: 'Resources', icon: Bot } : null,
      featureFlags.commands ? { id: 'commands' as HubSection, label: 'Commands', icon: Zap } : null,
      featureFlags.marketplace ? { id: 'marketplace' as HubSection, label: 'Marketplace', icon: Store } : null,
      { id: 'spaces' as HubSection, label: 'Spaces', icon: LayoutGrid },
    ].filter((s): s is { id: HubSection; label: string; icon: typeof Bot } => s !== null),
  );

  // Utility sections
  const utilSections: { id: HubSection; label: string; icon: typeof Bot }[] = [
    { id: 'settings', label: 'Settings', icon: Settings },
    { id: 'account', label: 'Account', icon: User },
  ];
</script>

<aside class="hub-sidebar" class:exiting={uiStore.hubExiting} style="width: {uiStore.sidebarWidthRaw}px; min-width: {uiStore.sidebarWidthRaw}px">
  <div class="traffic-light-area" data-tauri-drag-region></div>

  <nav class="hub-nav">
    {#each mainSections as section, i (section.id)}
      <button
        class="hub-nav-item"
        class:active={uiStore.hubSection === section.id}
        onclick={() => uiStore.setHubSection(section.id)}
        style="animation-delay: {i * 30}ms"
      >
        <section.icon size={18} />
        <span>{section.label}</span>
      </button>
    {/each}

    <div class="hub-nav-divider"></div>

    {#each utilSections as section, i (section.id)}
      <button
        class="hub-nav-item"
        class:active={uiStore.hubSection === section.id}
        onclick={() => uiStore.setHubSection(section.id)}
        style="animation-delay: {(mainSections.length + i) * 30}ms"
      >
        <section.icon size={18} />
        <span>{section.label}</span>
      </button>
    {/each}
  </nav>

  <div class="hub-spacer"></div>

  <button class="hub-back" onclick={() => uiStore.exitHubMode()} title="Back (Esc)">
    <ArrowLeft size={15} />
    <span>Back</span>
  </button>

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="resize-handle"
    onpointerdown={handleResizeStart}
    onpointermove={handleResizeMove}
    onpointerup={handleResizeEnd}
    onpointercancel={handleResizeEnd}
  ></div>
</aside>

<style>
  .hub-sidebar {
    position: relative;
    height: 100%;
    background: var(--weplex-sidebar-bg);
    display: flex;
    flex-direction: column;
    z-index: 20;
    flex-shrink: 0;
    overflow: hidden;
  }

  .traffic-light-area {
    height: 38px;
    flex-shrink: 0;
    -webkit-app-region: drag;
  }

  .hub-spacer {
    flex: 1;
  }

  .hub-back {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 8px;
    padding: 8px 12px;
    border: none;
    border-radius: var(--weplex-radius-md);
    background: transparent;
    color: rgba(255, 255, 255, 0.4);
    font-size: var(--weplex-text-sm);
    font-weight: 500;
    cursor: pointer;
    transition:
      color 0.15s,
      background 0.15s;
  }

  .hub-back:hover {
    color: rgba(255, 255, 255, 0.8);
    background: rgba(255, 255, 255, 0.06);
  }

  .hub-nav {
    display: flex;
    flex-direction: column;
    padding: 12px 8px 8px;
    gap: 2px;
  }

  .hub-nav-item {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 12px;
    border: none;
    border-radius: var(--weplex-radius-md);
    background: transparent;
    color: rgba(255, 255, 255, 0.5);
    font-size: var(--weplex-text-sm);
    font-weight: 500;
    cursor: pointer;
    transition:
      color 0.15s,
      background 0.15s;
    animation: hub-item-in 0.2s ease-out both;
  }

  .hub-nav-item:hover {
    color: rgba(255, 255, 255, 0.85);
    background: rgba(255, 255, 255, 0.06);
  }

  .hub-nav-item.active {
    color: var(--weplex-accent, rgba(255, 255, 255, 0.95));
    background: color-mix(in srgb, var(--weplex-accent, white) 12%, transparent);
  }

  .hub-nav-divider {
    height: 1px;
    margin: 8px 12px;
    background: rgba(255, 255, 255, 0.06);
  }

  .hub-sidebar.exiting {
    animation: hub-slide-out 0.2s ease-in forwards;
  }

  @keyframes hub-slide-out {
    to {
      opacity: 0;
      transform: translateX(-20px);
    }
  }

  .resize-handle {
    position: absolute;
    right: -3px;
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

  @keyframes hub-item-in {
    from {
      opacity: 0;
      transform: translateX(-8px);
    }
    to {
      opacity: 1;
      transform: translateX(0);
    }
  }
</style>

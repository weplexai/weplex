<script lang="ts">
  import { uiStore } from '../../stores/uiStore';
  import { featureFlags } from '../../stores/featureFlagsStore.svelte';
  import {
    Bot,
    Store,
    LayoutGrid,
    Settings as SettingsIcon,
    User,
  } from 'lucide-svelte';
  import SettingsPanel from '../overlays/Settings.svelte';
  import AuthPanel from '../overlays/AuthOverlay.svelte';
  import HubSpaces from './HubSpaces.svelte';
  import HubResources from './HubResources.svelte';
  import HubCommands from './HubCommands.svelte';
  import MarketplacePanel from '../overlays/MarketplaceOverlay.svelte';
  import type { HubSection } from '../../types';

  // Placeholder meta for sections not yet wired up
  const placeholders: Partial<Record<HubSection, { icon: typeof Bot; title: string; description: string }>> = {
  };

  // Sections with real components
  const liveComponents: Set<HubSection> = new Set(['settings', 'account', 'spaces', 'resources', 'commands', 'marketplace']);

  let isPlaceholder = $derived(!liveComponents.has(uiStore.hubSection));
  let placeholder = $derived(placeholders[uiStore.hubSection]);

  let prevSection = $state(uiStore.hubSection);
  let slideKey = $state(0);

  $effect(() => {
    if (uiStore.hubSection !== prevSection) {
      prevSection = uiStore.hubSection;
      slideKey++;
    }
  });
</script>

<div class="hub-content" class:exiting={uiStore.hubExiting}>
  {#key slideKey}
    <div class="hub-section-view">
      {#if uiStore.hubSection === 'resources' && featureFlags.resources}
        <HubResources />
      {:else if uiStore.hubSection === 'commands' && featureFlags.commands}
        <HubCommands />
      {:else if uiStore.hubSection === 'marketplace' && featureFlags.marketplace}
        <MarketplacePanel />
      {:else if uiStore.hubSection === 'settings'}
        <SettingsPanel />
      {:else if uiStore.hubSection === 'account'}
        <AuthPanel />
      {:else if uiStore.hubSection === 'spaces'}
        <HubSpaces />
      {:else if isPlaceholder && placeholder}
        <div class="hub-placeholder">
          <placeholder.icon size={48} strokeWidth={1.2} />
          <h1>{placeholder.title}</h1>
          <p>{placeholder.description}</p>
          <span class="hub-placeholder-badge">Coming soon</span>
        </div>
      {/if}
    </div>
  {/key}
</div>

<style>
  .hub-content {
    flex: 1;
    min-width: 0;
    position: relative;
    margin: 0;
    border-radius: 0;
    background: var(--weplex-bg);
    overflow: hidden;
  }

  .hub-section-view {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    animation: hub-slide-in 0.2s ease-out;
  }

  .hub-placeholder {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 16px;
    color: var(--weplex-text-muted);
    text-align: center;
    animation: hub-placeholder-in 0.3s ease-out;
  }

  .hub-placeholder h1 {
    font-size: 24px;
    font-weight: 600;
    color: var(--weplex-text);
    margin: 0;
  }

  .hub-placeholder p {
    font-size: var(--weplex-text-sm);
    margin: 0;
    max-width: 300px;
    line-height: 1.5;
  }

  .hub-placeholder-badge {
    display: inline-block;
    margin-top: 8px;
    padding: 4px 12px;
    border-radius: var(--weplex-radius-full, 999px);
    background: rgba(255, 255, 255, 0.06);
    color: var(--weplex-text-muted);
    font-size: 12px;
    font-weight: 500;
  }

  .hub-content.exiting {
    animation: hub-content-out 0.2s ease-in forwards;
  }

  @keyframes hub-content-out {
    to {
      opacity: 0;
    }
  }

  @keyframes hub-slide-in {
    from {
      opacity: 0;
      transform: translateX(-12px);
    }
    to {
      opacity: 1;
      transform: translateX(0);
    }
  }

  @keyframes hub-placeholder-in {
    from {
      opacity: 0;
      transform: scale(0.96);
    }
    to {
      opacity: 1;
      transform: scale(1);
    }
  }
</style>

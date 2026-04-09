<script lang="ts">
  import './styles.css';
  import { onMount, untrack } from 'svelte';
  import Sidebar from './lib/components/sidebar/Sidebar.svelte';

  import SplitContainer from './lib/components/terminal/SplitContainer.svelte';
  import TerminalView from './lib/components/terminal/TerminalView.svelte';
  import OrchestrationDashboard from './lib/components/dashboard/OrchestrationDashboard.svelte';
  import ProjectDashboard from './lib/components/dashboard/ProjectDashboard.svelte';
  import SpaceDashboard from './lib/components/dashboard/SpaceDashboard.svelte';
  import PipelineDashboard from './lib/components/dashboard/PipelineDashboard.svelte';
  import SpectatorView from './lib/components/terminal/SpectatorView.svelte';
  import MarketplaceOverlay from './lib/components/overlays/MarketplaceOverlay.svelte';
  import PluginView from './lib/components/terminal/PluginView.svelte';
  import DetailPanel from './lib/components/detail/DetailPanel.svelte';
  import SpaceChat from './lib/components/detail/SpaceChat.svelte';

  import CommandPalette from './lib/components/overlays/CommandPalette.svelte';
  import NewSessionDialog from './lib/components/overlays/NewSessionDialog.svelte';
  import SpaceModal from './lib/components/overlays/SpaceModal.svelte';
  import Settings from './lib/components/overlays/Settings.svelte';
  import AuthOverlay from './lib/components/overlays/AuthOverlay.svelte';
  import AgentsPipelines from './lib/components/overlays/AgentsPipelines.svelte';
  import UIKit from './lib/components/overlays/UIKit.svelte';
  import HubSidebar from './lib/components/hub/HubSidebar.svelte';
  import HubContent from './lib/components/hub/HubContent.svelte';
  import { sessionStore } from './lib/stores/sessionStore';
  import { spaceStore } from './lib/stores/spaceStore';
  import { uiStore } from './lib/stores/uiStore';
  import { splitStore } from './lib/stores/splitStore';
  import { authStore } from './lib/stores/authStore.svelte';
  import { pipelineRunStore } from './lib/stores/pipelineRunStore.svelte';
  import { HYPERSPACE_ID } from './lib/types';
  import { handleGlobalKeydown } from './lib/utils/shortcuts';
  import { checkForUpdates } from './lib/utils/updater';
  import { initNotifications } from './lib/services/notificationService';
  import { pluginStore } from './lib/stores/pluginStore.svelte';
  import { loadActivePlugins } from './lib/services/pluginLoader';
  import { invoke } from '@tauri-apps/api/core';

  onMount(() => {
    if (sessionStore.sessions.length === 0) {
      sessionStore.create({ name: 'terminal' });
    }

    // Initialize auth (load tokens, fetch profile, sync) — silent on failure
    authStore.init().catch((e) => console.error('[Weplex] Auth init failed:', e));

    // Initialize OS notifications (request permission, track focus, listen to hooks)
    initNotifications().catch((e) => console.warn('[Weplex] Notifications init:', e));

    // Load installed plugins and activate
    pluginStore.refresh().then(() => loadActivePlugins());

    // Initialize MCP event listener for pipeline stage completions
    pipelineRunStore.init();

    // Register MCP server config in Claude's settings (~/.claude.json)
    invoke('register_mcp_in_claude').catch((e) =>
      console.warn('[Weplex] Failed to register MCP in Claude config:', e),
    );

    window.addEventListener('keydown', handleGlobalKeydown);

    // Check for updates after a short delay, then every minute
    const updateTimer = setTimeout(checkForUpdates, 3000);
    const updateInterval = setInterval(checkForUpdates, 60 * 1000);

    return () => {
      window.removeEventListener('keydown', handleGlobalKeydown);
      clearTimeout(updateTimer);
      clearInterval(updateInterval);
    };
  });

  let activeSession = $derived(sessionStore.activeSession);
  let spaceBgColor = $derived(spaceStore.activeSpace.bgColor || null);
  let spaceGrain = $derived(spaceStore.activeSpace.grain ?? 0);
  let spaceBgMode = $derived(spaceStore.activeSpace.bgMode ?? 'dark');
  let spaceMixBase = $derived(spaceBgMode === 'light' ? 'var(--weplex-bg)' : 'var(--weplex-sidebar-bg)');
  let activeSpaceId = $derived(spaceStore.activeSpaceId);
  let chatServerId = $derived(spaceStore.activeSpace?.serverId ?? null);
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
    ? `background: color-mix(in srgb, ${spaceBgColor} 35%, ${spaceMixBase})`
    : ''}
>
  {#if spaceGrain > 0}
    <div class="app-grain" style="opacity: {Math.min(spaceGrain * 1.5, 1)}"></div>
  {/if}
  {#if uiStore.hubMode || uiStore.hubExiting}
    <HubSidebar />
    <HubContent />
  {/if}
  {#if uiStore.activeOverlay === 'agents' && !uiStore.hubMode}
    <AgentsPipelines />
  {/if}
  <div class="work-layout" class:hidden={uiStore.hubMode || uiStore.hubExiting || uiStore.activeOverlay === 'agents'}>
    <Sidebar />

    {#if uiStore.sidebarHidden}
      <button class="sidebar-reveal" onclick={() => uiStore.showSidebar()}>
        <span class="sidebar-reveal-hint">⌘B</span>
      </button>
    {/if}

    <div class="main" class:with-detail={uiStore.detailPanelOpen} class:no-sidebar={uiStore.sidebarHidden}>
      <div class="terminal-area">
        {#if uiStore.spaceChatOpen && chatServerId}
          <div class="space-chat-view">
            <SpaceChat serverId={chatServerId} />
          </div>
        {:else}
          {#if splitLayout}
            <SplitContainer node={splitLayout} spaceId={activeSpaceId} />
          {/if}
        {/if}

        <button
          class="detail-btn"
          class:active={uiStore.detailPanelOpen}
          onclick={() => uiStore.toggleDetailPanel()}
          title="Detail panel (⌘.)">ⓘ</button
        >

        {#if !activeSession}
          <div class="empty-state">
            <svg
              class="empty-logo"
              width="140"
              height="140"
              viewBox="0 0 165 165"
              fill="none"
              xmlns="http://www.w3.org/2000/svg"
            >
              <path
                d="M37.9855 28.4787C39.5323 27.7584 41.3338 28.2112 42.7969 28.9218C44.5323 30.0304 46.2645 31.1438 48.0015 32.2492C38.7154 38.7686 29.4341 45.2913 20.1464 51.8075C18.9315 49.666 17.7295 47.5149 16.5016 45.3799C15.7411 43.7621 16.5274 41.6255 18.1565 40.8859C24.7726 36.7641 31.3758 32.6133 37.9855 28.4787Z"
                fill="currentColor"
              /><path
                d="M122.21 28.917C123.476 28.3063 124.941 27.9018 126.344 28.237C127.119 28.403 127.727 28.9476 128.392 29.344C134.546 33.187 140.687 37.0526 146.847 40.8875C148.473 41.6271 149.256 43.7605 148.502 45.3767C147.271 47.5117 146.07 49.6661 144.852 51.8075C135.566 45.2913 126.283 38.7686 116.999 32.2492C118.737 31.1438 120.471 30.0272 122.21 28.917Z"
                fill="currentColor"
              /><path
                class="nav-light"
                d="M21.3452 53.928C30.9648 47.1685 40.5877 40.4122 50.2105 33.6575C51.7139 34.6114 53.2141 35.5701 54.7142 36.5256C44.4017 43.7589 34.094 51.0002 23.7848 58.2383C22.9694 56.801 22.1589 55.3637 21.3452 53.928Z"
                fill="currentColor"
              /><path
                d="M110.286 36.5256C111.786 35.5701 113.286 34.6114 114.789 33.6575C124.412 40.4105 134.034 47.1701 143.655 53.9264C142.841 55.3621 142.032 56.8026 141.215 58.2367C130.906 50.9986 120.598 43.7589 110.286 36.5256Z"
                fill="currentColor"
              /><path
                d="M24.9821 60.3492C35.6249 52.8774 46.2677 45.4041 56.9121 37.934C61.4496 40.8231 65.9726 43.7364 70.4892 46.6593C70.9968 46.9655 71.2659 47.6164 71.058 48.1852C70.913 48.7637 70.2636 48.9458 69.7528 49.0586C68.1528 49.3583 66.4947 49.5968 65.0558 50.4089C60.8631 52.7695 56.6704 55.1317 52.4762 57.4907C51.461 58.0417 50.5007 58.7008 49.6338 59.4645C48.9248 60.209 48.2287 60.976 47.6663 61.838C46.9429 62.9434 46.5755 64.2679 46.5803 65.586C46.5706 67.1957 46.8671 68.7861 47.083 70.3765C47.4101 72.6935 47.8259 74.9977 48.2287 77.3036C48.3238 78.0125 48.5123 78.7376 48.3463 79.4515C48.2142 80.2523 48.0079 81.124 47.365 81.6767C46.9896 81.9136 46.5126 81.9812 46.076 81.949C43.5043 81.5123 40.9229 81.1466 38.3464 80.747C37.6987 80.6277 36.9736 80.6487 36.4338 80.2184C35.902 79.814 35.6281 79.1872 35.3074 78.6216C31.8689 72.5292 28.4158 66.4448 24.9821 60.3492Z"
                fill="currentColor"
              /><path
                d="M94.5124 46.6577C99.029 43.7347 103.552 40.8231 108.088 37.934C118.732 45.4057 129.377 52.8774 140.021 60.3508C136.662 66.3014 133.294 72.2472 129.931 78.1978C129.504 78.8843 129.224 79.7012 128.569 80.2168C128.073 80.6164 127.411 80.6229 126.811 80.7228C124.42 81.0869 122.029 81.4463 119.639 81.8249C118.984 81.9845 118.239 82.0521 117.641 81.6815C116.836 80.9677 116.686 79.814 116.583 78.8053C116.686 77.476 116.99 76.1724 117.197 74.8576C117.638 72.3616 118.017 69.8528 118.328 67.3375C118.534 65.615 118.463 63.7764 117.575 62.2392C116.493 60.4265 114.963 58.8828 113.126 57.8323C108.763 55.3701 104.395 52.9177 100.031 50.4572C98.7663 49.7273 97.3162 49.426 95.8933 49.181C95.302 49.0602 94.6446 49.0086 94.1725 48.5945C93.6536 48.0209 93.8486 47.0347 94.5124 46.6577Z"
                fill="currentColor"
              /><path
                d="M80.4762 50.4571C82.6596 50.3024 84.8446 50.4797 87.0263 50.5892C89.7253 50.7488 92.4258 51.0034 95.0862 51.4948C96.7152 51.9412 98.3845 52.3585 99.8637 53.2012C103.979 55.3636 108.015 57.6759 111.981 60.0993C113.37 60.913 114.601 62.0248 115.471 63.3848C115.906 64.1518 116.211 65.0219 116.204 65.9114C116.096 69.7689 115.042 73.5201 114.793 77.3631C114.706 78.5957 114.429 79.8719 114.82 81.0804C115.149 82.3163 115.685 83.5667 116.775 84.3192C124.478 90.9546 132.172 97.5998 139.879 104.232C140.745 104.851 141.365 105.808 141.554 106.857C141.72 107.817 141.283 108.824 140.516 109.412C136.309 112.282 132.132 115.197 127.935 118.081C126.976 118.655 125.848 119.1 124.715 118.956C119.549 117.158 114.329 115.5 109.143 113.75C109.063 115.081 108.779 116.393 108.397 117.669C110.266 118.905 112.144 120.129 114.014 121.364C110.705 127.123 107.392 132.88 104.079 138.637C102.527 137.872 100.972 137.116 99.4271 136.339C100.726 132.844 102.16 129.399 103.515 125.925C102.777 125.075 102.047 124.216 101.348 123.333C103.51 121.829 105.044 119.567 106.053 117.164C106.499 115.8 106.683 114.361 106.855 112.94C106.929 111.539 107.163 110.157 107.313 108.765C108.915 95.2359 110.511 81.7072 112.115 68.1785C112.186 67.4341 112.065 66.6832 111.905 65.9597C111.596 64.9349 110.687 64.2855 109.896 63.6442C107.627 61.6768 104.975 60.2347 102.373 58.7716C100.4 57.7548 98.4458 56.5898 96.2366 56.1789C94.1129 55.6955 91.9215 55.7503 89.7688 55.4812C87.5065 55.3507 85.249 55.072 82.9802 55.1219C79.5884 55.0462 76.2143 55.4313 72.8402 55.6939C71.0226 55.789 69.1937 55.9775 67.447 56.5125C65.6794 57.0926 64.06 58.0304 62.4165 58.8844C59.8867 60.3184 57.3102 61.7251 55.1059 63.6442C54.2035 64.3564 53.1513 65.1476 53.0079 66.3787C52.6985 67.6919 53.0111 69.0212 53.1433 70.3345C54.7852 84.1629 56.4046 97.9961 58.0578 111.823C58.1803 113.583 58.4059 115.35 58.9038 117.045C59.8996 119.498 61.4577 121.804 63.6524 123.333C62.953 124.216 62.2247 125.075 61.4851 125.924C62.779 129.182 64.0681 132.44 65.3588 135.7C65.4361 135.911 65.5022 136.13 65.5666 136.349C64.0197 137.113 62.4713 137.877 60.9212 138.637C57.6083 132.88 54.2954 127.123 50.9857 121.364C52.8565 120.129 54.7337 118.906 56.6028 117.669C56.2209 116.393 55.9373 115.081 55.8568 113.75C50.9164 115.408 45.9568 117.011 41.0261 118.692C40.0899 119.206 38.9829 118.966 38.0467 118.584C37.1621 118.239 36.4274 117.627 35.6475 117.103C32.1493 114.694 28.6624 112.271 25.1593 109.872C24.4616 109.464 23.7929 108.903 23.5496 108.102C23.1516 106.915 23.714 105.635 24.5132 104.756C32.4828 97.8978 40.4396 91.0239 48.4076 84.1629C49.6628 83.2009 50.1317 81.5686 50.375 80.0766C50.338 77.9802 50.093 75.8871 49.7998 73.8117C49.3937 71.2932 48.9264 68.7747 48.8007 66.2224C48.7202 64.9204 49.2358 63.6281 50.0624 62.6388C50.8246 61.6784 51.7624 60.8631 52.8097 60.2282C56.8461 57.7548 60.955 55.3991 65.1461 53.1964C66.6204 52.3569 68.2833 51.9428 69.9075 51.4964C73.3928 50.8471 76.9394 50.6231 80.4762 50.4571Z"
                fill="currentColor"
              /><path
                d="M77.5146 58.2689C79.7076 58.1658 81.9039 57.9676 84.1001 58.0917C87.4774 58.2802 90.8902 58.3092 94.2047 59.0617C95.2279 59.2937 96.0384 59.9898 96.9472 60.4716C99.3513 61.7929 101.738 63.1593 103.982 64.7432C105.239 65.6681 106.614 66.4835 107.611 67.7097C107.972 68.2173 108.228 68.8215 108.304 69.4403C106.746 84.2161 105.163 98.992 103.6 113.768C103.291 116.082 101.813 118.094 99.9733 119.461C98.1203 120.817 95.7887 121.729 93.4668 121.451C93.4941 120.595 93.6698 119.757 93.7906 118.913C94.946 111.378 96.0642 103.839 97.2695 96.3124C97.9914 91.8409 98.7261 87.3727 99.4528 82.9029C99.925 79.98 100.491 77.0135 100.167 74.0406C99.896 71.2031 98.8679 68.3993 97.0278 66.1983C95.0781 63.828 92.305 62.2441 89.3756 61.411C86.1497 60.5151 82.745 60.362 79.4256 60.6956C76.398 60.9905 73.3848 61.888 70.8485 63.6008C68.7989 64.9785 67.1248 66.9201 66.1112 69.176C64.8222 72.0119 64.5176 75.2201 64.8753 78.2929C65.2572 81.6203 65.9114 84.909 66.4158 88.2171C67.845 96.8328 69.1711 105.465 70.4763 114.1C70.8147 116.551 71.2933 118.984 71.5382 121.444C69.6932 121.702 67.8208 121.111 66.2111 120.231C63.8554 118.906 61.8605 116.651 61.4223 113.919C59.8609 99.3223 58.3253 84.7205 56.7591 70.1251C56.5577 69.2679 56.9089 68.3993 57.3923 67.7049C58.364 66.5125 59.6949 65.7084 60.9211 64.8125C63.322 63.1013 65.8969 61.6608 68.4718 60.2348C69.2823 59.724 70.096 59.1713 71.0596 58.9973C73.185 58.5783 75.349 58.3366 77.5146 58.2689Z"
                fill="currentColor"
              /><path
                d="M80.9596 68.5976C83.5345 68.1899 86.2431 68.8441 88.3813 70.3266C90.4696 71.7558 91.9794 73.9859 92.5611 76.4448C93.051 78.4364 92.8979 80.5698 92.1631 82.4824C91.2044 85.0428 89.1822 87.1811 86.6733 88.2751C84.9121 89.0373 82.9318 89.3837 81.0288 89.0582C78.723 88.7199 76.5397 87.5887 74.9525 85.8807C73.0624 83.8682 72.0247 81.0741 72.1665 78.3139C72.2971 75.7342 73.4298 73.2189 75.2861 71.4223C76.8217 69.9189 78.8374 68.9247 80.9596 68.5976ZM80.6453 73.5927C78.5522 73.9762 76.4704 74.8721 75.0009 76.4464C73.989 77.6726 73.1865 79.3597 73.7005 80.9629C74.4482 84.2114 77.1504 86.9023 80.4036 87.6322C82.5854 88.114 84.9556 87.8465 86.9102 86.7363C89.4432 85.3377 91.2704 82.6468 91.4815 79.7448C91.2543 77.8789 90.12 76.1708 88.5135 75.2024C86.2125 73.712 83.3459 73.1319 80.6453 73.5927Z"
                fill="currentColor"
              /><path
                d="M80.2684 96.2205C83.4056 95.982 86.5831 96.2495 89.6479 96.952C91.0062 97.4967 92.0568 99.0758 91.6685 100.557C90.7484 106.573 89.8686 112.595 88.9631 118.613C88.8519 119.453 88.7681 120.373 88.1558 121.019C87.6289 121.546 87.0101 122.069 86.2448 122.169C85.3859 122.31 84.5126 122.31 83.6473 122.4C82.1826 122.55 80.7195 122.326 79.2581 122.239C77.774 122.176 76.3416 120.983 76.1643 119.48C75.2185 113.175 74.2952 106.865 73.3332 100.561C72.9062 98.8663 74.3032 96.9956 76.0128 96.7941C77.4179 96.5122 78.8407 96.3269 80.2684 96.2205Z"
                fill="currentColor"
              /><path
                d="M45.5039 124.964C46.5303 124.287 47.5599 123.613 48.5896 122.94C51.8412 128.595 55.0977 134.248 58.3462 139.905C57.4728 140.316 56.6285 140.796 55.7358 141.164C53.6362 140.853 51.5592 140.374 49.4661 140.013C49.0858 139.921 48.5944 139.923 48.3817 139.532C46.8654 137.111 45.4249 134.641 43.8845 132.237C43.7314 131.95 43.783 131.617 43.8361 131.314C44.189 129.461 44.6885 127.638 45.0752 125.792C45.1558 125.489 45.188 125.118 45.5039 124.964Z"
                fill="currentColor"
              /><path
                d="M116.41 122.94C117.442 123.613 118.471 124.288 119.499 124.965C119.847 125.142 119.852 125.571 119.95 125.903C120.344 127.824 120.9 129.711 121.211 131.649C121.294 132.235 120.814 132.664 120.542 133.127C119.253 135.228 117.973 137.335 116.684 139.436C116.546 139.698 116.299 139.881 116.003 139.918C113.863 140.324 111.721 140.735 109.58 141.138C109.263 141.24 108.979 141.048 108.705 140.922C108.023 140.576 107.335 140.247 106.654 139.904C109.904 134.248 113.159 128.595 116.41 122.94Z"
                fill="currentColor"
              /><path
                d="M65.3233 124.335C65.3877 124.304 65.5166 124.242 65.5811 124.211C66.767 124.522 67.9159 124.97 69.1212 125.224C76.0547 126.781 83.259 127.063 90.3085 126.201C93.1638 125.803 96.0336 125.345 98.7632 124.388C99.079 124.3 99.4818 124.068 99.7445 124.393C100.386 125.118 101.009 125.866 101.598 126.634C101.578 126.92 101.441 127.181 101.338 127.443C100.171 130.145 99.0258 132.857 97.8689 135.563C96.8618 137.082 95.3198 138.186 93.6794 138.947C90.9821 140.203 88.0237 140.804 85.0733 141.07C81.3544 141.372 77.5597 141.164 73.9552 140.16C72.1392 139.635 70.3394 138.913 68.8521 137.72C68.0625 137.092 67.4132 136.281 67.0297 135.344C65.9727 132.815 64.8866 130.299 63.8086 127.78C63.6572 127.405 63.4477 127.042 63.4026 126.638C64.0149 125.85 64.6385 125.06 65.3233 124.335Z"
                fill="currentColor"
              />
              <path
                d="M37.9855 28.4787C39.5323 27.7584 41.3338 28.2112 42.7969 28.9218C44.5323 30.0304 46.2645 31.1438 48.0015 32.2492C38.7154 38.7686 29.4341 45.2913 20.1464 51.8075C18.9315 49.666 17.7295 47.5149 16.5016 45.3799C15.7411 43.7621 16.5274 41.6255 18.1565 40.8859C24.7726 36.7641 31.3758 32.6133 37.9855 28.4787Z"
              /><path
                d="M122.21 28.917C123.476 28.3063 124.941 27.9018 126.344 28.237C127.119 28.403 127.727 28.9476 128.392 29.344C134.546 33.187 140.687 37.0526 146.847 40.8875C148.473 41.6271 149.256 43.7605 148.502 45.3767C147.271 47.5117 146.07 49.6661 144.852 51.8075C135.566 45.2913 126.283 38.7686 116.999 32.2492C118.737 31.1438 120.471 30.0272 122.21 28.917Z"
              /><path
                d="M21.3452 53.928C30.9648 47.1685 40.5877 40.4122 50.2105 33.6575C51.7139 34.6114 53.2141 35.5701 54.7142 36.5256C44.4017 43.7589 34.094 51.0002 23.7848 58.2383C22.9694 56.801 22.1589 55.3637 21.3452 53.928Z"
              /><path
                d="M110.286 36.5256C111.786 35.5701 113.286 34.6114 114.789 33.6575C124.412 40.4105 134.034 47.1701 143.655 53.9264C142.841 55.3621 142.032 56.8026 141.215 58.2367C130.906 50.9986 120.598 43.7589 110.286 36.5256Z"
              /><path
                d="M24.9821 60.3492C35.6249 52.8774 46.2677 45.4041 56.9121 37.934C61.4496 40.8231 65.9726 43.7364 70.4892 46.6593C70.9968 46.9655 71.2659 47.6164 71.058 48.1852C70.913 48.7637 70.2636 48.9458 69.7528 49.0586C68.1528 49.3583 66.4947 49.5968 65.0558 50.4089C60.8631 52.7695 56.6704 55.1317 52.4762 57.4907C51.461 58.0417 50.5007 58.7008 49.6338 59.4645C48.9248 60.209 48.2287 60.976 47.6663 61.838C46.9429 62.9434 46.5755 64.2679 46.5803 65.586C46.5706 67.1957 46.8671 68.7861 47.083 70.3765C47.4101 72.6935 47.8259 74.9977 48.2287 77.3036C48.3238 78.0125 48.5123 78.7376 48.3463 79.4515C48.2142 80.2523 48.0079 81.124 47.365 81.6767C46.9896 81.9136 46.5126 81.9812 46.076 81.949C43.5043 81.5123 40.9229 81.1466 38.3464 80.747C37.6987 80.6277 36.9736 80.6487 36.4338 80.2184C35.902 79.814 35.6281 79.1872 35.3074 78.6216C31.8689 72.5292 28.4158 66.4448 24.9821 60.3492Z"
              /><path
                d="M94.5124 46.6577C99.029 43.7347 103.552 40.8231 108.088 37.934C118.732 45.4057 129.377 52.8774 140.021 60.3508C136.662 66.3014 133.294 72.2472 129.931 78.1978C129.504 78.8843 129.224 79.7012 128.569 80.2168C128.073 80.6164 127.411 80.6229 126.811 80.7228C124.42 81.0869 122.029 81.4463 119.639 81.8249C118.984 81.9845 118.239 82.0521 117.641 81.6815C116.836 80.9677 116.686 79.814 116.583 78.8053C116.686 77.476 116.99 76.1724 117.197 74.8576C117.638 72.3616 118.017 69.8528 118.328 67.3375C118.534 65.615 118.463 63.7764 117.575 62.2392C116.493 60.4265 114.963 58.8828 113.126 57.8323C108.763 55.3701 104.395 52.9177 100.031 50.4572C98.7663 49.7273 97.3162 49.426 95.8933 49.181C95.302 49.0602 94.6446 49.0086 94.1725 48.5945C93.6536 48.0209 93.8486 47.0347 94.5124 46.6577Z"
              /><path
                d="M80.4762 50.4571C82.6596 50.3024 84.8446 50.4797 87.0263 50.5892C89.7253 50.7488 92.4258 51.0034 95.0862 51.4948C96.7152 51.9412 98.3845 52.3585 99.8637 53.2012C103.979 55.3636 108.015 57.6759 111.981 60.0993C113.37 60.913 114.601 62.0248 115.471 63.3848C115.906 64.1518 116.211 65.0219 116.204 65.9114C116.096 69.7689 115.042 73.5201 114.793 77.3631C114.706 78.5957 114.429 79.8719 114.82 81.0804C115.149 82.3163 115.685 83.5667 116.775 84.3192C124.478 90.9546 132.172 97.5998 139.879 104.232C140.745 104.851 141.365 105.808 141.554 106.857C141.72 107.817 141.283 108.824 140.516 109.412C136.309 112.282 132.132 115.197 127.935 118.081C126.976 118.655 125.848 119.1 124.715 118.956C119.549 117.158 114.329 115.5 109.143 113.75C109.063 115.081 108.779 116.393 108.397 117.669C110.266 118.905 112.144 120.129 114.014 121.364C110.705 127.123 107.392 132.88 104.079 138.637C102.527 137.872 100.972 137.116 99.4271 136.339C100.726 132.844 102.16 129.399 103.515 125.925C102.777 125.075 102.047 124.216 101.348 123.333C103.51 121.829 105.044 119.567 106.053 117.164C106.499 115.8 106.683 114.361 106.855 112.94C106.929 111.539 107.163 110.157 107.313 108.765C108.915 95.2359 110.511 81.7072 112.115 68.1785C112.186 67.4341 112.065 66.6832 111.905 65.9597C111.596 64.9349 110.687 64.2855 109.896 63.6442C107.627 61.6768 104.975 60.2347 102.373 58.7716C100.4 57.7548 98.4458 56.5898 96.2366 56.1789C94.1129 55.6955 91.9215 55.7503 89.7688 55.4812C87.5065 55.3507 85.249 55.072 82.9802 55.1219C79.5884 55.0462 76.2143 55.4313 72.8402 55.6939C71.0226 55.789 69.1937 55.9775 67.447 56.5125C65.6794 57.0926 64.06 58.0304 62.4165 58.8844C59.8867 60.3184 57.3102 61.7251 55.1059 63.6442C54.2035 64.3564 53.1513 65.1476 53.0079 66.3787C52.6985 67.6919 53.0111 69.0212 53.1433 70.3345C54.7852 84.1629 56.4046 97.9961 58.0578 111.823C58.1803 113.583 58.4059 115.35 58.9038 117.045C59.8996 119.498 61.4577 121.804 63.6524 123.333C62.953 124.216 62.2247 125.075 61.4851 125.924C62.779 129.182 64.0681 132.44 65.3588 135.7C65.4361 135.911 65.5022 136.13 65.5666 136.349C64.0197 137.113 62.4713 137.877 60.9212 138.637C57.6083 132.88 54.2954 127.123 50.9857 121.364C52.8565 120.129 54.7337 118.906 56.6028 117.669C56.2209 116.393 55.9373 115.081 55.8568 113.75C50.9164 115.408 45.9568 117.011 41.0261 118.692C40.0899 119.206 38.9829 118.966 38.0467 118.584C37.1621 118.239 36.4274 117.627 35.6475 117.103C32.1493 114.694 28.6624 112.271 25.1593 109.872C24.4616 109.464 23.7929 108.903 23.5496 108.102C23.1516 106.915 23.714 105.635 24.5132 104.756C32.4828 97.8978 40.4396 91.0239 48.4076 84.1629C49.6628 83.2009 50.1317 81.5686 50.375 80.0766C50.338 77.9802 50.093 75.8871 49.7998 73.8117C49.3937 71.2932 48.9264 68.7747 48.8007 66.2224C48.7202 64.9204 49.2358 63.6281 50.0624 62.6388C50.8246 61.6784 51.7624 60.8631 52.8097 60.2282C56.8461 57.7548 60.955 55.3991 65.1461 53.1964C66.6204 52.3569 68.2833 51.9428 69.9075 51.4964C73.3928 50.8471 76.9394 50.6231 80.4762 50.4571Z"
              /><path
                d="M77.5146 58.2689C79.7076 58.1658 81.9039 57.9676 84.1001 58.0917C87.4774 58.2802 90.8902 58.3092 94.2047 59.0617C95.2279 59.2937 96.0384 59.9898 96.9472 60.4716C99.3513 61.7929 101.738 63.1593 103.982 64.7432C105.239 65.6681 106.614 66.4835 107.611 67.7097C107.972 68.2173 108.228 68.8215 108.304 69.4403C106.746 84.2161 105.163 98.992 103.6 113.768C103.291 116.082 101.813 118.094 99.9733 119.461C98.1203 120.817 95.7887 121.729 93.4668 121.451C93.4941 120.595 93.6698 119.757 93.7906 118.913C94.946 111.378 96.0642 103.839 97.2695 96.3124C97.9914 91.8409 98.7261 87.3727 99.4528 82.9029C99.925 79.98 100.491 77.0135 100.167 74.0406C99.896 71.2031 98.8679 68.3993 97.0278 66.1983C95.0781 63.828 92.305 62.2441 89.3756 61.411C86.1497 60.5151 82.745 60.362 79.4256 60.6956C76.398 60.9905 73.3848 61.888 70.8485 63.6008C68.7989 64.9785 67.1248 66.9201 66.1112 69.176C64.8222 72.0119 64.5176 75.2201 64.8753 78.2929C65.2572 81.6203 65.9114 84.909 66.4158 88.2171C67.845 96.8328 69.1711 105.465 70.4763 114.1C70.8147 116.551 71.2933 118.984 71.5382 121.444C69.6932 121.702 67.8208 121.111 66.2111 120.231C63.8554 118.906 61.8605 116.651 61.4223 113.919C59.8609 99.3223 58.3253 84.7205 56.7591 70.1251C56.5577 69.2679 56.9089 68.3993 57.3923 67.7049C58.364 66.5125 59.6949 65.7084 60.9211 64.8125C63.322 63.1013 65.8969 61.6608 68.4718 60.2348C69.2823 59.724 70.096 59.1713 71.0596 58.9973C73.185 58.5783 75.349 58.3366 77.5146 58.2689Z"
              /><path
                d="M80.9596 68.5976C83.5345 68.1899 86.2431 68.8441 88.3813 70.3266C90.4696 71.7558 91.9794 73.9859 92.5611 76.4448C93.051 78.4364 92.8979 80.5698 92.1631 82.4824C91.2044 85.0428 89.1822 87.1811 86.6733 88.2751C84.9121 89.0373 82.9318 89.3837 81.0288 89.0582C78.723 88.7199 76.5397 87.5887 74.9525 85.8807C73.0624 83.8682 72.0247 81.0741 72.1665 78.3139C72.2971 75.7342 73.4298 73.2189 75.2861 71.4223C76.8217 69.9189 78.8374 68.9247 80.9596 68.5976ZM80.6453 73.5927C78.5522 73.9762 76.4704 74.8721 75.0009 76.4464C73.989 77.6726 73.1865 79.3597 73.7005 80.9629C74.4482 84.2114 77.1504 86.9023 80.4036 87.6322C82.5854 88.114 84.9556 87.8465 86.9102 86.7363C89.4432 85.3377 91.2704 82.6468 91.4815 79.7448C91.2543 77.8789 90.12 76.1708 88.5135 75.2024C86.2125 73.712 83.3459 73.1319 80.6453 73.5927Z"
              /><path
                d="M80.2684 96.2205C83.4056 95.982 86.5831 96.2495 89.6479 96.952C91.0062 97.4967 92.0568 99.0758 91.6685 100.557C90.7484 106.573 89.8686 112.595 88.9631 118.613C88.8519 119.453 88.7681 120.373 88.1558 121.019C87.6289 121.546 87.0101 122.069 86.2448 122.169C85.3859 122.31 84.5126 122.31 83.6473 122.4C82.1826 122.55 80.7195 122.326 79.2581 122.239C77.774 122.176 76.3416 120.983 76.1643 119.48C75.2185 113.175 74.2952 106.865 73.3332 100.561C72.9062 98.8663 74.3032 96.9956 76.0128 96.7941C77.4179 96.5122 78.8407 96.3269 80.2684 96.2205Z"
              /><path
                d="M45.5039 124.964C46.5303 124.287 47.5599 123.613 48.5896 122.94C51.8412 128.595 55.0977 134.248 58.3462 139.905C57.4728 140.316 56.6285 140.796 55.7358 141.164C53.6362 140.853 51.5592 140.374 49.4661 140.013C49.0858 139.921 48.5944 139.923 48.3817 139.532C46.8654 137.111 45.4249 134.641 43.8845 132.237C43.7314 131.95 43.783 131.617 43.8361 131.314C44.189 129.461 44.6885 127.638 45.0752 125.792C45.1558 125.489 45.188 125.118 45.5039 124.964Z"
              /><path
                d="M116.41 122.94C117.442 123.613 118.471 124.288 119.499 124.965C119.847 125.142 119.852 125.571 119.95 125.903C120.344 127.824 120.9 129.711 121.211 131.649C121.294 132.235 120.814 132.664 120.542 133.127C119.253 135.228 117.973 137.335 116.684 139.436C116.546 139.698 116.299 139.881 116.003 139.918C113.863 140.324 111.721 140.735 109.58 141.138C109.263 141.24 108.979 141.048 108.705 140.922C108.023 140.576 107.335 140.247 106.654 139.904C109.904 134.248 113.159 128.595 116.41 122.94Z"
              /><path
                d="M65.3233 124.335C65.3877 124.304 65.5166 124.242 65.5811 124.211C66.767 124.522 67.9159 124.97 69.1212 125.224C76.0547 126.781 83.259 127.063 90.3085 126.201C93.1638 125.803 96.0336 125.345 98.7632 124.388C99.079 124.3 99.4818 124.068 99.7445 124.393C100.386 125.118 101.009 125.866 101.598 126.634C101.578 126.92 101.441 127.181 101.338 127.443C100.171 130.145 99.0258 132.857 97.8689 135.563C96.8618 137.082 95.3198 138.186 93.6794 138.947C90.9821 140.203 88.0237 140.804 85.0733 141.07C81.3544 141.372 77.5597 141.164 73.9552 140.16C72.1392 139.635 70.3394 138.913 68.8521 137.72C68.0625 137.092 67.4132 136.281 67.0297 135.344C65.9727 132.815 64.8866 130.299 63.8086 127.78C63.6572 127.405 63.4477 127.042 63.4026 126.638C64.0149 125.85 64.6385 125.06 65.3233 124.335Z"
              />
            </svg>
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
  </div>

  <!-- Terminal instances live outside the conditional so they survive overlay switches
       (AgentsPipelines replaces the {:else} block, which would destroy all terminals) -->
  <div id="terminal-host">
    {#each sessionStore.sessions as session (session.id)}
      {#if session.type === 'dashboard' && session.dashboardType === 'orchestration'}
        <OrchestrationDashboard sessionId={session.id} orchestratorId={session.orchestratorId} />
      {:else if session.type === 'dashboard' && session.dashboardType === 'project'}
        <ProjectDashboard sessionId={session.id} />
      {:else if session.type === 'dashboard' && session.dashboardType === 'space'}
        <SpaceDashboard sessionId={session.id} />
      {:else if session.type === 'dashboard' && session.dashboardType === 'pipeline'}
        <PipelineDashboard sessionId={session.id} />
      {:else if session.type === 'plugin' && session.pluginId}
        <PluginView sessionId={session.id} pluginId={session.pluginId} />
      {:else if session.type === 'spectator' && session.spectateSpaceId && session.spectateSessionName}
        <SpectatorView
          spaceId={session.spectateSpaceId}
          sessionName={session.spectateSessionName}
          ownerName={session.spectateOwnerName || 'Unknown'}
          sessionId={session.id}
        />
      {:else}
        <TerminalView sessionId={session.id} />
      {/if}
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
{:else if uiStore.activeOverlay === 'auth'}
  <AuthOverlay />
{:else if uiStore.activeOverlay === 'marketplace'}
  <MarketplaceOverlay />
{:else if uiStore.activeOverlay === 'uikit'}
  <UIKit />
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

  .work-layout {
    display: contents;
  }

  .work-layout.hidden {
    display: none;
  }

  .app-grain {
    position: absolute;
    inset: 0;
    z-index: 0;
    pointer-events: none;
    background-image: url("data:image/svg+xml,%3Csvg viewBox='0 0 200 200' xmlns='http://www.w3.org/2000/svg'%3E%3Cfilter id='n'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='0.75' numOctaves='4' stitchTiles='stitch'/%3E%3CfeColorMatrix type='saturate' values='0'/%3E%3C/filter%3E%3Crect width='100%25' height='100%25' filter='url(%23n)'/%3E%3C/svg%3E");
    background-repeat: repeat;
    background-size: 200px 200px;
    mix-blend-mode: soft-light;
    transition: opacity 0.3s ease;
  }


  .main {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    position: relative;
    z-index: 0;
    margin: 9px 9px 9px 0;
    border-radius: 10px;
    box-shadow:
      0 0 0 1px rgba(0, 0, 0, 0.4),
      0 2px 8px rgba(0, 0, 0, 0.3);
    background: var(--weplex-bg);
    overflow: hidden;
    transition: background 0.3s ease, margin 0.2s ease;
  }

  .main.no-sidebar {
    margin-left: 9px;
  }

  .terminal-area {
    flex: 1;
    position: relative;
    min-height: 0;
    overflow: hidden;
    background: var(--weplex-bg);
  }

  .space-chat-view {
    position: absolute;
    inset: 0;
    padding: 16px;
    display: flex;
    flex-direction: column;
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

  .empty-logo {
    color: var(--weplex-color-text-muted, #6b6b80);
    opacity: 0.3;
  }

  .empty-logo :global(.nav-light) {
    animation: nav-light 2s ease-in-out 1s infinite;
  }

  @keyframes nav-light {
    0%,
    70%,
    100% {
      fill: currentColor;
      opacity: 1;
    }
    15% {
      fill: #ff6600;
      opacity: 3;
    }
    30% {
      fill: currentColor;
      opacity: 1;
    }
    45% {
      fill: #ff6600;
      opacity: 3;
    }
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

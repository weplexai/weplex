<script lang="ts">
  import { onMount } from 'svelte';
  import { HYPERSPACE_ID } from '../../types';
  import { sessionStore } from '../../stores/sessionStore';
  import { spaceStore } from '../../stores/spaceStore';
  import { folderStore } from '../../stores/folderStore';
  import { uiStore } from '../../stores/uiStore';
  import { dragStore } from '../../stores/dragStore';
  import { pipelineRunStore } from '../../stores/pipelineRunStore.svelte';
  import { Plus, X, User } from 'lucide-svelte';
  import { authStore } from '../../stores/authStore.svelte';
  import SpaceSwitcher from './SpaceSwitcher.svelte';
  import SidebarSearch from './SidebarSearch.svelte';
  import SessionItem from './SessionItem.svelte';
  import FolderItem from './FolderItem.svelte';
  import HyperspaceView from './HyperspaceView.svelte';
  import PipelineGroup from './PipelineGroup.svelte';
  import { splitStore } from '../../stores/splitStore';
  import { findNode } from '../../utils/splitTree';
  import type { DropTargetType } from '../../stores/dragStore';

  // Active space data (for drag & drop context)
  let spaceId = $derived(spaceStore.activeSpaceId);
  let activeBgColor = $derived(
    spaceStore.activeSpaceId === HYPERSPACE_ID ? null : spaceStore.activeSpace.bgColor || null,
  );
  let folders = $derived(folderStore.getBySpace(spaceId));

  // Slider: active index + swipe offset
  // Hyperspace is at index 0, regular spaces start at index 1
  let activeIdx = $derived(
    spaceStore.activeSpaceId === HYPERSPACE_ID
      ? 0
      : spaceStore.spaces.findIndex((s) => s.id === spaceStore.activeSpaceId) + 1,
  );
  let totalSlides = $derived(spaceStore.spaces.length + 1); // +1 for Hyperspace
  let swipeOffset = $state(0);
  let snapping = $state(false);

  let showBgMenu = $state(false);
  let bgMenuPos = $state({ x: 0, y: 0 });
  let scrollEl = $state<HTMLElement>();

  // Resize state
  let isResizing = $state(false);
  let resizeStartX = 0;
  let resizeStartWidth = 0;

  // Zone highlight when dragging over empty zone area
  let pinnedZoneHighlight = $derived(
    dragStore.isDragging && dragStore.dropTarget?.type === 'pinned-zone',
  );
  let unpinnedZoneHighlight = $derived(
    dragStore.isDragging && dragStore.dropTarget?.type === 'unpinned-zone',
  );

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
    const newWidth = resizeStartWidth + delta;

    if (newWidth < uiStore.MIN_WIDTH) {
      // Below minimum — hide sidebar
      uiStore.hideSidebar();
      isResizing = false;
    } else {
      if (uiStore.sidebarHidden) uiStore.showSidebar();
      uiStore.setSidebarWidth(newWidth);
    }
  }

  function handleResizeEnd() {
    isResizing = false;
  }

  function handleBgContext(e: MouseEvent) {
    if ((e.target as HTMLElement).closest('.session-item, .folder')) return;
    e.preventDefault();
    showBgMenu = true;
    bgMenuPos = { x: e.clientX, y: e.clientY };
  }

  function createFolder() {
    showBgMenu = false;
    folderStore.create('New Folder', spaceId);
  }

  // --- Pointer-based drag & drop ---

  function handlePointerMove(e: PointerEvent) {
    if (!dragStore.dragType) return;
    dragStore.updatePosition(e.clientX, e.clientY);
    if (!dragStore.isDragging) return;

    // Hit-test against all session items and folder headers
    const target = findDropTarget(e.clientX, e.clientY);
    dragStore.setDropTarget(target);
  }

  function handlePointerUp(e: PointerEvent) {
    if (!dragStore.dragType) return;

    if (dragStore.isDragging && dragStore.dropTarget) {
      executeDrop();
    }

    dragStore.endDrag();
  }

  function findDropTarget(x: number, y: number): typeof dragStore.dropTarget {
    if (!scrollEl) return null;

    const draggedId = dragStore.dragId;
    const dragType = dragStore.dragType;

    // Only handle session drags for now
    if (dragType !== 'session') return null;

    // Check folder headers first
    const folderEls = scrollEl.querySelectorAll<HTMLElement>('[data-folder-id]');
    for (const folderEl of folderEls) {
      const header = folderEl.querySelector('.folder-header');
      if (!header) continue;
      const rect = header.getBoundingClientRect();
      if (y >= rect.top && y <= rect.bottom && x >= rect.left && x <= rect.right) {
        const folderId = folderEl.dataset.folderId!;
        return { type: 'folder', id: folderId };
      }
    }

    // Check session items
    const sessionEls = scrollEl.querySelectorAll<HTMLElement>('[data-session-id]');
    for (const el of sessionEls) {
      const rect = el.getBoundingClientRect();
      if (y >= rect.top && y <= rect.bottom && x >= rect.left && x <= rect.right) {
        const sessionId = getSessionIdFromEl(el);
        if (sessionId === null || sessionId === draggedId) continue;

        const midY = rect.top + rect.height / 2;
        return y < midY
          ? { type: 'before-session', id: sessionId }
          : { type: 'after-session', id: sessionId };
      }
    }

    // Check if pointer is in pinned or unpinned zone (for zone-level drops)
    const pinnedZone = scrollEl.querySelector<HTMLElement>('.pinned-zone');
    const unpinnedZone = scrollEl.querySelector<HTMLElement>('.unpinned-zone');

    if (pinnedZone) {
      const rect = pinnedZone.getBoundingClientRect();
      if (y >= rect.top && y <= rect.bottom && x >= rect.left && x <= rect.right) {
        return { type: 'pinned-zone' };
      }
    }

    if (unpinnedZone) {
      const rect = unpinnedZone.getBoundingClientRect();
      if (y >= rect.top && y <= rect.bottom && x >= rect.left && x <= rect.right) {
        return { type: 'unpinned-zone' };
      }
    }

    // Hyperspace "By Space" fallback: hovering over a group area but not on a specific session
    if (spaceStore.isHyperspace) {
      const spaceGroupEls = scrollEl.querySelectorAll<HTMLElement>(
        '.hyperspace-group[data-space-id]',
      );
      for (const groupEl of spaceGroupEls) {
        const rect = groupEl.getBoundingClientRect();
        if (y >= rect.top && y <= rect.bottom && x >= rect.left && x <= rect.right) {
          const targetSpaceId = groupEl.dataset.spaceId!;
          const draggedSession = sessionStore.sessions.find((s) => s.id === draggedId);
          if (draggedSession && draggedSession.spaceId !== targetSpaceId) {
            return { type: 'space-group', id: targetSpaceId };
          }
          return null;
        }
      }
    }

    // Check terminal panes (for drag-to-split) — disabled in Hyperspace
    if (spaceStore.isHyperspace) return null;
    const paneEls = document.querySelectorAll<HTMLElement>('[data-pane-id]');
    for (const paneEl of paneEls) {
      const rect = paneEl.getBoundingClientRect();
      if (y >= rect.top && y <= rect.bottom && x >= rect.left && x <= rect.right) {
        const paneId = paneEl.dataset.paneId!;
        // Skip if dragging onto the pane that already shows this session
        const layout = splitStore.getLayout(spaceId);
        if (layout) {
          const leaf = findNode(layout, paneId);
          if (leaf && leaf.type === 'leaf' && leaf.sessionId === draggedId) return null;
        }
        // Determine which zone based on pointer position within pane
        const dx = (x - rect.left) / rect.width; // 0..1
        const dy = (y - rect.top) / rect.height; // 0..1
        const distLeft = dx;
        const distRight = 1 - dx;
        const distTop = dy;
        const distBottom = 1 - dy;
        const minDist = Math.min(distLeft, distRight, distTop, distBottom);
        const EDGE_THRESHOLD = 0.25;

        let zone: DropTargetType;
        if (minDist > EDGE_THRESHOLD) {
          zone = 'split-center';
        } else if (minDist === distLeft) {
          zone = 'split-left';
        } else if (minDist === distRight) {
          zone = 'split-right';
        } else if (minDist === distTop) {
          zone = 'split-top';
        } else {
          zone = 'split-bottom';
        }
        return { type: zone, id: paneId };
      }
    }

    return null;
  }

  function getSessionIdFromEl(el: HTMLElement): number | null {
    const raw = el.dataset.sessionId;
    if (!raw) return null;
    const id = Number(raw);
    return isNaN(id) ? null : id;
  }

  function executeDrop() {
    const target = dragStore.dropTarget;
    if (!target || dragStore.dragType !== 'session') return;

    const draggedId = dragStore.dragId as number;

    switch (target.type) {
      case 'folder': {
        const folderId = target.id as string;
        sessionStore.reorder(draggedId, null, { pinned: true, folderId });
        // Auto-expand folder on drop
        const folder = folders.find((f) => f.id === folderId);
        if (folder?.collapsed) folderStore.toggle(folderId);
        break;
      }
      case 'before-session': {
        const beforeId = target.id as number;
        const beforeSession = findSession(beforeId);
        if (beforeSession) {
          const draggedSession = findSession(draggedId);
          if (draggedSession && draggedSession.spaceId !== beforeSession.spaceId) {
            sessionStore.moveToSpace(draggedId, beforeSession.spaceId);
          } else {
            sessionStore.reorder(draggedId, beforeId, {
              pinned: beforeSession.pinned,
              folderId: beforeSession.folderId,
            });
          }
        }
        break;
      }
      case 'after-session': {
        const afterId = target.id as number;
        const afterSession = findSession(afterId);
        if (afterSession) {
          const draggedSession = findSession(draggedId);
          if (draggedSession && draggedSession.spaceId !== afterSession.spaceId) {
            sessionStore.moveToSpace(draggedId, afterSession.spaceId);
          } else {
            const siblings = getSiblings(afterSession);
            const idx = siblings.findIndex((s) => s.id === afterId);
            const nextId = idx < siblings.length - 1 ? siblings[idx + 1].id : null;
            sessionStore.reorder(draggedId, nextId, {
              pinned: afterSession.pinned,
              folderId: afterSession.folderId,
            });
          }
        }
        break;
      }
      case 'pinned-zone': {
        sessionStore.reorder(draggedId, null, { pinned: true, folderId: undefined });
        break;
      }
      case 'unpinned-zone': {
        sessionStore.reorder(draggedId, null, { pinned: false, folderId: undefined });
        break;
      }
      case 'space-group': {
        const targetSpaceId = target.id as string;
        sessionStore.moveToSpace(draggedId, targetSpaceId);
        break;
      }
      case 'split-left': {
        const paneId = target.id as string;
        splitStore.splitWithExistingSession(spaceId, paneId, 'horizontal', draggedId, 'before');
        break;
      }
      case 'split-right': {
        const paneId = target.id as string;
        splitStore.splitWithExistingSession(spaceId, paneId, 'horizontal', draggedId, 'after');
        break;
      }
      case 'split-top': {
        const paneId = target.id as string;
        splitStore.splitWithExistingSession(spaceId, paneId, 'vertical', draggedId, 'before');
        break;
      }
      case 'split-bottom': {
        const paneId = target.id as string;
        splitStore.splitWithExistingSession(spaceId, paneId, 'vertical', draggedId, 'after');
        break;
      }
      case 'split-center': {
        const paneId = target.id as string;
        splitStore.replaceSessionInPane(spaceId, paneId, draggedId);
        break;
      }
    }
  }

  function findSession(id: number) {
    return sessionStore.sessions.find((s) => s.id === id);
  }

  function getSiblings(session: { pinned: boolean; folderId?: string; spaceId: string }) {
    if (session.pinned && session.folderId) {
      return sessionStore.getByFolder(session.folderId);
    }
    if (session.pinned) {
      return sessionStore.getPinnedStandalone(session.spaceId);
    }
    return sessionStore.getUnpinned(session.spaceId);
  }

  // Swipe gesture for space switching
  let gestureEndTimer: ReturnType<typeof setTimeout>;
  let viewportEl = $state<HTMLElement>();
  let ignoreUntil = 0;

  function handleSwipeWheel(e: WheelEvent) {
    if (Math.abs(e.deltaX) <= Math.abs(e.deltaY)) return;
    if (Math.abs(e.deltaX) < 2) return;
    if (Date.now() < ignoreUntil) {
      // Keep extending cooldown while inertia events arrive
      ignoreUntil = Math.max(ignoreUntil, Date.now() + 5);
      return;
    }

    const w = viewportEl?.clientWidth ?? 240;
    const canNext = activeIdx < totalSlides - 1;
    const canPrev = activeIdx > 0;

    let delta = -e.deltaX * 0.5;

    // Rubber band at edges
    if ((!canNext && swipeOffset + delta < 0) || (!canPrev && swipeOffset + delta > 0)) {
      delta *= 0.1;
    }

    swipeOffset += delta;

    // Switch at 40% threshold — transition handles the rest
    if (swipeOffset < -w * 0.4 && canNext) {
      clearTimeout(gestureEndTimer);
      snapping = true;
      spaceStore.switchToNext();
      sessionStore.activateForSpace(spaceStore.activeSpaceId);
      swipeOffset = 0;
      ignoreUntil = Date.now() + 600;
      setTimeout(() => {
        snapping = false;
      }, 300);
      return;
    }
    if (swipeOffset > w * 0.4 && canPrev) {
      clearTimeout(gestureEndTimer);
      snapping = true;
      spaceStore.switchToPrevious();
      sessionStore.activateForSpace(spaceStore.activeSpaceId);
      swipeOffset = 0;
      ignoreUntil = Date.now() + 600;
      setTimeout(() => {
        snapping = false;
      }, 300);
      return;
    }

    // Snap back when gesture ends without crossing threshold
    clearTimeout(gestureEndTimer);
    gestureEndTimer = setTimeout(() => {
      snapping = true;
      swipeOffset = 0;
      setTimeout(() => {
        snapping = false;
      }, 300);
    }, 80);
  }

  // Register global pointer listeners
  onMount(() => {
    pipelineRunStore.init();

    const onMove = (e: PointerEvent) => handlePointerMove(e);
    const onUp = (e: PointerEvent) => handlePointerUp(e);

    window.addEventListener('pointermove', onMove);
    window.addEventListener('pointerup', onUp);
    window.addEventListener('wheel', handleSwipeWheel, { passive: true });

    return () => {
      window.removeEventListener('pointermove', onMove);
      window.removeEventListener('pointerup', onUp);
      window.removeEventListener('wheel', handleSwipeWheel);
    };
  });
</script>

{#if !uiStore.sidebarHidden}
  <aside
    class="sidebar"
    class:resizing={isResizing}
    style="width: {uiStore.sidebarWidth}px; min-width: {uiStore.sidebarWidth}px{activeBgColor
      ? `; background: color-mix(in srgb, ${activeBgColor} 15%, var(--weplex-sidebar-bg))`
      : ''}"
  >
    <div class="traffic-light-area" data-tauri-drag-region>
      <button
        class="account-btn"
        class:signed-in={authStore.isAuthenticated}
        title={authStore.isAuthenticated
          ? authStore.user?.displayName || authStore.user?.email || 'Account'
          : 'Sign In'}
        onclick={() => {
          uiStore.openOverlay('settings');
          setTimeout(() => {
            const tab = document.querySelector('[data-tab="account"]') as HTMLElement;
            tab?.click();
          }, 50);
        }}
      >
        {#if authStore.isAuthenticated}
          <span class="avatar-initial"
            >{(authStore.user?.displayName || authStore.user?.email || '?')[0].toUpperCase()}</span
          >
        {:else}
          <User size={14} />
        {/if}
      </button>
    </div>
    <SidebarSearch />

    <!-- Slider viewport -->
    <div class="slider-viewport" bind:this={viewportEl}>
      <!-- svelte-ignore a11y_no_static_element_interactions a11y_click_events_have_key_events -->
      <div
        bind:this={scrollEl}
        class="slider-track"
        class:snapping
        style:transform="translateX(calc(-{activeIdx} * 100% + {swipeOffset}px))"
        oncontextmenu={handleBgContext}
        onclick={() => (showBgMenu = false)}
      >
        <!-- Hyperspace slide (always first) -->
        <div class="slider-slide">
          <HyperspaceView />
        </div>

        <!-- Regular space slides -->
        {#each spaceStore.spaces as space (space.id)}
          {@const spaceFolders = folderStore.getBySpace(space.id)}
          {@const spacePinned = sessionStore.getPinnedStandalone(space.id)}
          {@const spaceUnpinned = sessionStore.getUnpinned(space.id)}
          {@const splitIds = splitStore.hasSplits(space.id)
            ? splitStore.getVisibleSessionIds(space.id)
            : []}
          <div class="slider-slide">
            <!-- Split group (Arc-style grouped indicator) -->
            {#if splitIds.length > 1}
              <div class="split-group">
                <div class="split-group-header">
                  <span class="split-group-label">{splitIds.length} split</span>
                  <button
                    class="split-group-unsplit"
                    title="Unsplit"
                    onclick={(e) => {
                      e.stopPropagation();
                      splitStore.reset(space.id);
                    }}><X size={12} /></button
                  >
                </div>
                {#each splitIds as sid (sid)}
                  {@const session =
                    spacePinned.find((s) => s.id === sid) ||
                    spaceUnpinned.find((s) => s.id === sid)}
                  {#if session}
                    <SessionItem
                      {session}
                      active={session.id === sessionStore.activeSessionId}
                      onclick={() => sessionStore.activate(session.id)}
                    />
                  {/if}
                {/each}
              </div>
            {/if}

            <!-- Active pipeline runs -->
            {#each pipelineRunStore.runs.filter((r) => r.status === 'running' || r.status === 'pending') as run (run.id)}
              <PipelineGroup {run} />
            {/each}

            <!-- Recently finished pipeline runs -->
            {#each pipelineRunStore.runs
              .filter((r) => r.status !== 'running' && r.status !== 'pending')
              .slice(-3) as run (run.id)}
              <PipelineGroup {run} />
            {/each}

            <!-- Pinned zone -->
            <div
              class="pinned-zone"
              class:zone-highlight={pinnedZoneHighlight && space.id === spaceId}
            >
              {#each spaceFolders as folder (folder.id)}
                <FolderItem {folder} />
              {/each}

              {#each spacePinned as session (session.id)}
                {#if !splitIds.includes(session.id) || splitIds.length <= 1}
                  <SessionItem
                    {session}
                    active={session.id === sessionStore.activeSessionId}
                    onclick={() => sessionStore.activate(session.id)}
                  />
                {/if}
              {/each}
            </div>

            <button class="new-session-btn" onclick={() => uiStore.openOverlay('new-session')}>
              <Plus size={12} />
              <span>New Session</span>
            </button>

            <!-- Unpinned zone -->
            <div
              class="unpinned-zone"
              class:zone-highlight={unpinnedZoneHighlight && space.id === spaceId}
            >
              {#each spaceUnpinned as session (session.id)}
                {#if !splitIds.includes(session.id) || splitIds.length <= 1}
                  <SessionItem
                    {session}
                    active={session.id === sessionStore.activeSessionId}
                    onclick={() => sessionStore.activate(session.id)}
                  />
                {/if}
              {/each}
            </div>
          </div>
        {/each}
      </div>

      {#if showBgMenu}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="bg-menu"
          style="left: {bgMenuPos.x}px; top: {bgMenuPos.y}px"
          onfocusout={(e) => {
            if (!(e.currentTarget as HTMLElement).contains(e.relatedTarget as Node))
              showBgMenu = false;
          }}
        >
          <button
            class="bg-menu-item"
            onclick={(e) => {
              e.stopPropagation();
              createFolder();
            }}>New Folder</button
          >
          <button
            class="bg-menu-item"
            onclick={(e) => {
              e.stopPropagation();
              showBgMenu = false;
              uiStore.openOverlay('new-session');
            }}>New Session</button
          >
        </div>
      {/if}
    </div>

    <SpaceSwitcher />

    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="resize-handle"
      onpointerdown={handleResizeStart}
      onpointermove={handleResizeMove}
      onpointerup={handleResizeEnd}
      onpointercancel={handleResizeEnd}
    ></div>
  </aside>
{/if}

<style>
  .sidebar {
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
    display: flex;
    align-items: center;
    justify-content: flex-end;
    padding-right: 10px;
  }

  .account-btn {
    -webkit-app-region: no-drag;
    margin-top: 2px;
    width: 22px;
    height: 22px;
    border-radius: var(--weplex-radius-sm);
    border: none;
    background: transparent;
    color: rgba(255, 255, 255, 0.2);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    flex-shrink: 0;
    transition: color 0.15s;
  }

  .account-btn:hover {
    color: rgba(255, 255, 255, 0.45);
  }

  .account-btn.signed-in {
    color: var(--weplex-accent);
  }

  .avatar-initial {
    font-size: 10px;
    font-weight: 700;
    line-height: 1;
  }

  .sidebar:not(.resizing) {
    transition:
      width var(--weplex-duration-normal) var(--weplex-easing),
      min-width var(--weplex-duration-normal) var(--weplex-easing),
      background 0.3s ease;
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

  .slider-viewport {
    flex: 1;
    overflow: hidden;
    position: relative;
  }

  .slider-track {
    display: flex;
    height: 100%;
    will-change: transform;
  }

  .slider-track.snapping {
    transition: transform 0.25s cubic-bezier(0.25, 0.46, 0.45, 0.94);
  }

  .slider-slide {
    min-width: 100%;
    max-width: 100%;
    overflow-y: auto;
    padding: 4px 8px;
    display: flex;
    flex-direction: column;
  }

  .pinned-zone {
    display: flex;
    flex-direction: column;
    gap: 1px;
    border-radius: var(--weplex-radius-md);
    transition: background var(--weplex-duration-fast) var(--weplex-easing);
  }

  .pinned-zone.zone-highlight {
    background: color-mix(in srgb, var(--weplex-accent) 8%, transparent);
  }

  .split-group {
    display: flex;
    flex-direction: column;
    gap: 1px;
    border-left: 2px solid var(--weplex-accent);
    border-radius: 0 var(--weplex-radius-md) var(--weplex-radius-md) 0;
    margin-bottom: 4px;
    padding-left: 4px;
    background: color-mix(in srgb, var(--weplex-accent) 5%, transparent);
  }

  .split-group-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 3px 8px;
  }

  .split-group-label {
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .split-group-unsplit {
    width: 18px;
    height: 18px;
    border: none;
    border-radius: var(--weplex-radius-sm);
    background: transparent;
    color: var(--weplex-text-muted);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    padding: 0;
  }

  .split-group-unsplit:hover {
    background: var(--weplex-surface-hover);
    color: var(--weplex-text);
  }

  .unpinned-zone {
    display: flex;
    flex-direction: column;
    gap: 1px;
    flex: 1;
    border-radius: var(--weplex-radius-md);
    transition: background var(--weplex-duration-fast) var(--weplex-easing);
  }

  .unpinned-zone.zone-highlight {
    background: color-mix(in srgb, var(--weplex-accent) 8%, transparent);
  }

  .bg-menu {
    position: fixed;
    z-index: 60;
    min-width: 140px;
    background: var(--weplex-surface);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-lg);
    padding: 4px;
    box-shadow: var(--weplex-shadow-md);
  }

  .bg-menu-item {
    display: block;
    width: 100%;
    padding: 6px 10px;
    border: none;
    border-radius: var(--weplex-radius-sm);
    background: transparent;
    color: var(--weplex-text);
    font-size: var(--weplex-text-sm);
    text-align: left;
    cursor: pointer;
  }

  .bg-menu-item:hover {
    background: var(--weplex-surface-hover);
  }

  .new-session-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 6px 12px;
    margin: 6px 0;
    border: none;
    border-radius: var(--weplex-radius-md);
    background: transparent;
    color: var(--weplex-text-muted);
    font-size: var(--weplex-text-sm);
    cursor: pointer;
    flex-shrink: 0;
    transition:
      color var(--weplex-duration-fast) var(--weplex-easing),
      background var(--weplex-duration-fast) var(--weplex-easing);
  }

  .new-session-btn:hover {
    color: var(--weplex-text);
    background: var(--weplex-surface-hover);
  }
</style>

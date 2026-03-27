<script lang="ts">
  import type { SplitNode } from '../../types';
  import { splitStore } from '../../stores/splitStore';
  import { dragStore } from '../../stores/dragStore';
  import { terminalRegistry } from '../../stores/terminalRegistry';
  import { sessionStore } from '../../stores/sessionStore';
  import SplitDivider from './SplitDivider.svelte';
  import SplitContainer from './SplitContainer.svelte';

  let {
    node,
    spaceId,
    isSplit = false,
  }: { node: SplitNode | null; spaceId: string; isSplit?: boolean } = $props();

  let containerEl: HTMLDivElement | undefined = $state();
  let slotEl: HTMLDivElement | undefined = $state();

  function handleResize(delta: number) {
    if (!node || node.type !== 'branch' || !containerEl) return;
    const rect = containerEl.getBoundingClientRect();
    const size = node.direction === 'horizontal' ? rect.width : rect.height;
    if (size <= 0) return;
    const ratioDelta = delta / size;
    const newRatio = Math.max(0.15, Math.min(0.85, node.ratio + ratioDelta));
    splitStore.setRatio(spaceId, node.id, newRatio);
  }

  function handleResizeEnd() {
    splitStore.persistLayout(spaceId);
  }

  function handlePaneClick(paneId: string) {
    splitStore.focusPane(paneId);
    if (node?.type === 'leaf') {
      sessionStore.activate(node.sessionId);
    }
  }

  let isFocused = $derived(
    isSplit && node != null && splitStore.focusedPaneForSpace(spaceId) === node.id,
  );

  // Portal: move terminal DOM element into this slot.
  // Uses polling because TerminalView may register after SplitContainer mounts.
  // IMPORTANT: capture sessionId in a local var so cleanup unmounts the OLD terminal,
  // not the new one (Svelte 5 signals read current value in cleanup).
  $effect(() => {
    if (!node || node.type !== 'leaf' || !slotEl) return;
    const sessionId = node.sessionId;
    const slot = slotEl;

    let mounted = false;
    let retryTimer: ReturnType<typeof setTimeout> | undefined;
    let retryCount = 0;
    const MAX_RETRIES = 100; // 5 seconds at 50ms intervals

    function tryMount() {
      const termEl = terminalRegistry.get(sessionId);
      if (!termEl) {
        if (retryCount++ < MAX_RETRIES) {
          retryTimer = setTimeout(tryMount, 50);
        } else {
          console.error(`[Weplex] Terminal mount failed for session ${sessionId} after ${MAX_RETRIES} retries`);
        }
        return;
      }
      mounted = true;
      slot.appendChild(termEl);
      termEl.style.display = '';
    }

    tryMount();

    return () => {
      if (retryTimer) clearTimeout(retryTimer);
      const termEl = terminalRegistry.get(sessionId);
      if (termEl && mounted && termEl.parentElement === slot) {
        const host = document.getElementById('terminal-host');
        if (host) {
          host.appendChild(termEl);
          termEl.style.display = 'none';
        }
      }
    };
  });

  // Drop zone: detect if this leaf is the target of a drag-to-split
  let dropZone = $derived.by(() => {
    if (!node || node.type !== 'leaf') return null;
    if (!dragStore.isDragging || dragStore.dragType !== 'session') return null;
    const target = dragStore.dropTarget;
    if (!target || target.id !== node.id) return null;
    if (target.type === 'split-left') return 'left';
    if (target.type === 'split-right') return 'right';
    if (target.type === 'split-top') return 'top';
    if (target.type === 'split-bottom') return 'bottom';
    if (target.type === 'split-center') return 'center';
    return null;
  });

  let showDropOverlay = $derived(
    node != null &&
      node.type === 'leaf' &&
      dragStore.isDragging &&
      dragStore.dragType === 'session',
  );
</script>

{#if !node}
  <!-- null guard -->
{:else if node.type === 'leaf'}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_noninteractive_element_interactions -->
  <div
    class="split-leaf"
    class:focused={isFocused}
    data-pane-id={node.id}
    onclick={() => handlePaneClick(node.id)}
    role="group"
  >
    <div class="terminal-slot" bind:this={slotEl}></div>
    {#if showDropOverlay}
      <div class="drop-overlay">
        {#if dropZone}
          <div class="drop-zone {dropZone}"></div>
        {/if}
      </div>
    {/if}
  </div>
{:else}
  <div
    class="split-branch"
    class:horizontal={node.direction === 'horizontal'}
    class:vertical={node.direction === 'vertical'}
    bind:this={containerEl}
  >
    <div class="split-child" style="flex: {node.ratio}">
      <SplitContainer node={node.children[0]} {spaceId} isSplit={true} />
    </div>
    <SplitDivider
      direction={node.direction}
      onResize={handleResize}
      onResizeEnd={handleResizeEnd}
    />
    <div class="split-child" style="flex: {1 - node.ratio}">
      <SplitContainer node={node.children[1]} {spaceId} isSplit={true} />
    </div>
  </div>
{/if}

<style>
  .split-leaf {
    width: 100%;
    height: 100%;
    position: relative;
    border: 1px solid transparent;
    border-radius: 2px;
    transition: border-color 0.15s ease;
  }

  .split-leaf.focused {
    border-color: color-mix(in srgb, var(--weplex-accent) 40%, transparent);
  }

  .terminal-slot {
    width: 100%;
    height: 100%;
  }

  .split-branch {
    display: flex;
    width: 100%;
    height: 100%;
    min-width: 0;
    min-height: 0;
  }

  .split-branch.horizontal {
    flex-direction: row;
  }

  .split-branch.vertical {
    flex-direction: column;
  }

  .split-child {
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }

  .drop-overlay {
    position: absolute;
    inset: 0;
    z-index: 10;
    pointer-events: none;
  }

  .drop-zone {
    position: absolute;
    background: color-mix(in srgb, var(--weplex-accent) 15%, transparent);
    border: 2px solid color-mix(in srgb, var(--weplex-accent) 50%, transparent);
    border-radius: 4px;
    transition: all 0.1s ease;
  }

  .drop-zone.left {
    left: 0;
    top: 0;
    bottom: 0;
    width: 50%;
  }

  .drop-zone.right {
    right: 0;
    top: 0;
    bottom: 0;
    width: 50%;
  }

  .drop-zone.top {
    left: 0;
    top: 0;
    right: 0;
    height: 50%;
  }

  .drop-zone.bottom {
    left: 0;
    bottom: 0;
    right: 0;
    height: 50%;
  }

  .drop-zone.center {
    inset: 8px;
    background: color-mix(in srgb, var(--weplex-accent) 10%, transparent);
  }
</style>

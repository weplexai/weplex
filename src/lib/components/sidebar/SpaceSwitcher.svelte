<script lang="ts">
  import { Layers, Eye, Users } from 'lucide-svelte';
  import { HYPERSPACE_ID } from '../../types';
  import { spaceStore } from '../../stores/spaceStore';
  import { sessionStore } from '../../stores/sessionStore';
  import { folderStore } from '../../stores/folderStore';
  import { uiStore } from '../../stores/uiStore';
  import { authStore } from '../../stores/authStore.svelte';

  // Filter out shared/team spaces when not authenticated
  let visibleSpaces = $derived(
    authStore.isAuthenticated
      ? spaceStore.spaces
      : spaceStore.spaces.filter(s => !s.shared && s.type !== 'team')
  );

  let showMenu = $state(false);
  let menuBtnEl = $state<HTMLButtonElement>();

  // Drag state
  let dragIndex = $state<number | null>(null);
  let dropIndex = $state<number | null>(null); // insertion point (drop BEFORE this index)
  let dragStartX = 0;
  let dragStartY = 0;
  let isDragging = $state(false);
  let suppressClick = $state(false);
  const DRAG_THRESHOLD = 4;

  function handlePillPointerDown(e: PointerEvent, index: number) {
    if (e.button !== 0) return;
    dragIndex = index;
    dragStartX = e.clientX;
    dragStartY = e.clientY;
    isDragging = false;
    suppressClick = false;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function handlePillPointerMove(e: PointerEvent) {
    if (dragIndex === null) return;
    if (!isDragging) {
      const dist = Math.sqrt((e.clientX - dragStartX) ** 2 + (e.clientY - dragStartY) ** 2);
      if (dist > DRAG_THRESHOLD) {
        isDragging = true;
        suppressClick = true;
      } else return;
    }

    // Find insertion point based on pill midpoints
    const pills = document.querySelectorAll<HTMLElement>('.space-pill:not(.hyperspace)');
    let best: number | null = null;
    for (let i = 0; i < pills.length; i++) {
      const rect = pills[i].getBoundingClientRect();
      const midX = rect.left + rect.width / 2;
      if (e.clientX < midX) {
        best = i;
        break;
      }
    }
    // If past all pills, drop at the end
    if (best === null) best = pills.length;
    dropIndex = best;
  }

  function handlePillPointerUp() {
    if (dragIndex !== null && isDragging && dropIndex !== null) {
      // Convert insertion index to reorder: if dropping after the dragged item, adjust
      let targetIndex = dropIndex;
      if (targetIndex > dragIndex) targetIndex--;
      if (targetIndex !== dragIndex) {
        spaceStore.reorder(dragIndex, targetIndex);
      }
    }
    dragIndex = null;
    dropIndex = null;
    isDragging = false;
  }

  // Close menu on click outside
  $effect(() => {
    if (!showMenu) return;
    function onClickOutside(e: MouseEvent) {
      const target = e.target as HTMLElement;
      if (!target.closest('.add-wrapper')) {
        showMenu = false;
      }
    }
    const timer = setTimeout(() => {
      window.addEventListener('click', onClickOutside, { capture: true });
    }, 0);
    return () => {
      clearTimeout(timer);
      window.removeEventListener('click', onClickOutside, { capture: true });
    };
  });

  function handleNewSession() {
    showMenu = false;
    uiStore.openOverlay('new-session');
  }

  function handleNewSpace() {
    showMenu = false;
    spaceStore.editingSpaceId = null;
    uiStore.openOverlay('space-modal');
  }

  function handleEditSpace(id: string) {
    spaceStore.editingSpaceId = id;
    uiStore.openOverlay('space-modal');
  }

  function handleNewFolder() {
    showMenu = false;
    folderStore.create('New Folder', spaceStore.activeSpaceId);
  }
</script>

<div class="spaces">
  <!-- Hyperspace pill (always first) -->
  <button
    class="space-pill hyperspace"
    class:active={spaceStore.activeSpaceId === HYPERSPACE_ID}
    onclick={() => {
      spaceStore.activate(HYPERSPACE_ID);
      sessionStore.activateForSpace(HYPERSPACE_ID);
    }}
    title="All Spaces"
  >
    <Layers size={14} />
  </button>

  {#each visibleSpaces as space, i (space.id)}
    {#if isDragging && dropIndex === i && dragIndex !== i}
      <div class="drop-indicator"></div>
    {/if}
    <button
      class="space-pill"
      class:active={space.id === spaceStore.activeSpaceId}
      class:drag-source={isDragging && dragIndex === i}
      style="--space-color: {space.color}"
      onclick={() => {
        if (!suppressClick) {
          spaceStore.activate(space.id);
          sessionStore.activateForSpace(space.id);
        }
      }}
      oncontextmenu={(e) => {
        e.preventDefault();
        handleEditSpace(space.id);
      }}
      onpointerdown={(e) => handlePillPointerDown(e, i)}
      onpointermove={handlePillPointerMove}
      onpointerup={handlePillPointerUp}
      onpointercancel={handlePillPointerUp}
      title={space.name}
    >
      <span class="pill-letter">{space.name[0].toUpperCase()}</span>
      {#if space.type === 'team'}
        <span class="pill-badge"><Users size={8} /></span>
      {:else if space.shared}
        <span class="pill-badge"><Eye size={8} /></span>
      {/if}
    </button>
  {/each}
  {#if isDragging && dropIndex === spaceStore.spaces.length}
    <div class="drop-indicator"></div>
  {/if}

  <div
    class="add-wrapper"
    onfocusout={(e) => {
      if (!(e.currentTarget as HTMLElement).contains(e.relatedTarget as Node)) showMenu = false;
    }}
  >
    <button
      bind:this={menuBtnEl}
      class="space-add"
      onclick={() => (showMenu = !showMenu)}
      title="New...">+</button
    >

    {#if showMenu}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="add-menu">
        <button class="add-menu-item" onclick={handleNewSpace}>New Space</button>
        <button class="add-menu-item" onclick={handleNewFolder}>New Folder</button>
        <button class="add-menu-item" onclick={handleNewSession}
          >New Session <span class="shortcut">&#8984;N</span></button
        >
      </div>
    {/if}
  </div>
</div>

<style>
  .spaces {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 8px 12px 18px;
    position: relative;
  }

  .space-pill {
    width: 28px;
    height: 28px;
    border-radius: var(--weplex-radius-md);
    border: 2px solid transparent;
    background: color-mix(in srgb, var(--space-color) 20%, transparent);
    color: var(--space-color);
    font-size: var(--weplex-text-sm);
    font-weight: 600;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all var(--weplex-duration-fast) var(--weplex-easing);
  }

  .space-pill:hover {
    background: color-mix(in srgb, var(--space-color) 30%, transparent);
  }

  .space-pill.active {
    border-color: var(--space-color);
    background: color-mix(in srgb, var(--space-color) 25%, transparent);
  }

  .space-pill {
    position: relative;
  }

  .pill-letter {
    line-height: 1;
  }

  .pill-badge {
    position: absolute;
    bottom: -2px;
    right: -2px;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--weplex-surface);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--space-color);
    pointer-events: none;
  }

  .space-pill.drag-source {
    opacity: 0.35;
    transform: scale(0.9);
  }

  .drop-indicator {
    width: 2px;
    height: 22px;
    border-radius: 1px;
    background: var(--weplex-accent);
    flex-shrink: 0;
    margin: 0 -1px;
    z-index: 1;
    animation: drop-indicator-fade-in 0.1s ease-out;
  }

  @keyframes drop-indicator-fade-in {
    from {
      opacity: 0;
      transform: scaleY(0.5);
    }
    to {
      opacity: 1;
      transform: scaleY(1);
    }
  }

  .space-pill.hyperspace {
    --space-color: var(--weplex-text-secondary);
  }

  .add-wrapper {
    position: relative;
  }

  .space-add {
    width: 28px;
    height: 28px;
    border-radius: var(--weplex-radius-md);
    border: 1px dashed var(--weplex-border);
    background: transparent;
    color: var(--weplex-text-muted);
    font-size: var(--weplex-text-md);
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all var(--weplex-duration-fast) var(--weplex-easing);
  }

  .space-add:hover {
    border-color: var(--weplex-accent);
    color: var(--weplex-accent);
  }

  .add-menu {
    position: absolute;
    bottom: 100%;
    right: 0;
    margin-bottom: 6px;
    min-width: 180px;
    background: var(--weplex-surface);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-lg);
    padding: 4px;
    box-shadow: var(--weplex-shadow-md);
    z-index: 60;
  }

  .add-menu-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
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

  .add-menu-item:hover {
    background: var(--weplex-surface-hover);
  }

  .shortcut {
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
  }
</style>

<script lang="ts">
  import type { Session } from '../../types';
  import { STATUS_COLORS } from '../../types';
  import { sessionStore } from '../../stores/sessionStore';
  import { folderStore } from '../../stores/folderStore';
  import { spaceStore } from '../../stores/spaceStore';
  import { dragStore } from '../../stores/dragStore';
  import SessionIcon from '../SessionIcon.svelte';
  import SpaceBadge from './SpaceBadge.svelte';

  let {
    session,
    active = false,
    onclick,
    badgeLetter,
    badgeColor,
  }: {
    session: Session;
    active?: boolean;
    onclick: () => void;
    badgeLetter?: string;
    badgeColor?: string;
  } = $props();

  let showMenu = $state(false);
  let showFolderMenu = $state(false);
  let showSpaceMenu = $state(false);
  let showIconPicker = $state(false);

  let hovered = $state(false);
  let renaming = $state(false);
  let renameValue = $state('');
  let renameInput = $state<HTMLInputElement>();
  let itemEl = $state<HTMLElement>();

  let folders = $derived(folderStore.getBySpace(spaceStore.activeSpaceId));
  let otherSpaces = $derived(spaceStore.spaces.filter((s) => s.id !== session.spaceId));
  let isDragged = $derived(
    dragStore.isDragging && dragStore.dragType === 'session' && dragStore.dragId === session.id,
  );

  // Drop indicator: set by parent Sidebar via dragStore.dropTarget
  let dropPos = $derived.by(() => {
    const t = dragStore.dropTarget;
    if (!t) return null;
    if (t.type === 'before-session' && t.id === session.id) return 'before';
    if (t.type === 'after-session' && t.id === session.id) return 'after';
    return null;
  });

  const ICON_OPTIONS = [
    // Lucide icons
    'lucide:rocket',
    'lucide:zap',
    'lucide:star',
    'lucide:flame',
    'lucide:sparkles',
    'lucide:brain',
    'lucide:bot',
    'lucide:code',
    'lucide:terminal',
    'lucide:bug',
    'lucide:wrench',
    'lucide:shield',
    'lucide:lock',
    'lucide:key',
    'lucide:eye',
    'lucide:search',
    'lucide:globe',
    'lucide:server',
    'lucide:database',
    'lucide:cloud',
    'lucide:cpu',
    'lucide:layers',
    'lucide:git-branch',
    'lucide:package',
    'lucide:box',
    'lucide:network',
    'lucide:monitor',
    'lucide:lightbulb',
    'lucide:atom',
    'lucide:palette',
    'lucide:heart',
    'lucide:bookmark',
    'lucide:flag',
    'lucide:bell',
    'lucide:command',
    'lucide:coffee',
    'lucide:moon',
    'lucide:sun',
    'lucide:home',
    'lucide:users',
    'lucide:file-text',
    'lucide:link',
    'lucide:send',
    'lucide:fingerprint',
    'lucide:gamepad2',
  ];

  function setIcon(icon: string | undefined) {
    sessionStore.update(session.id, { icon });
    showIconPicker = false;
    showMenu = false;
  }

  function handleContextMenu(e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    showFolderMenu = false;
    showSpaceMenu = false;
    showIconPicker = false;
    if (showMenu) {
      showMenu = false;
      return;
    }
    showMenu = true;
  }

  function moveToSpace(spaceId: string) {
    sessionStore.moveToSpace(session.id, spaceId);
    showMenu = false;
    showSpaceMenu = false;
  }

  function menuAction(action: string) {
    showMenu = false;
    showFolderMenu = false;
    showSpaceMenu = false;
    switch (action) {
      case 'pin':
        sessionStore.pin(session.id);
        break;
      case 'rename': {
        renameValue = session.name;
        renaming = true;
        requestAnimationFrame(() => renameInput?.focus());
        break;
      }
      case 'kill':
        sessionStore.kill(session.id);
        break;
      case 'remove-from-folder': {
        sessionStore.update(session.id, { folderId: undefined, pinned: false });
        break;
      }
    }
  }

  function moveToFolder(folderId: string) {
    sessionStore.update(session.id, { folderId, pinned: true });
    showMenu = false;
    showFolderMenu = false;
  }

  function createFolderAndMove() {
    const folder = folderStore.create('New Folder', spaceStore.activeSpaceId);
    sessionStore.update(session.id, { folderId: folder.id, pinned: true });
    showMenu = false;
    showFolderMenu = false;
  }

  let suppressClick = false;

  function handlePointerDown(e: PointerEvent) {
    // Only left button, skip if renaming or menu is open
    if (e.button !== 0 || renaming || showMenu) return;
    if (!itemEl) return;
    suppressClick = false;
    dragStore.startPotentialDrag('session', session.id, e.clientX, e.clientY, itemEl);
  }

  function handleClick() {
    // Suppress click if we just finished a drag
    if (suppressClick) {
      suppressClick = false;
      return;
    }
    onclick();
  }

  // Watch for drag activation to suppress click on release
  $effect(() => {
    if (dragStore.isDragging && dragStore.dragId === session.id) {
      suppressClick = true;
    }
  });

  // Close menu on click outside
  $effect(() => {
    if (!showMenu) return;
    function onClickOutside(e: MouseEvent) {
      const target = e.target as HTMLElement;
      if (!target.closest('.context-menu')) {
        showMenu = false;
        showFolderMenu = false;
        showSpaceMenu = false;
      }
    }
    // Delay to avoid catching the same click that opened the menu
    const timer = setTimeout(() => {
      window.addEventListener('click', onClickOutside, { capture: true });
    }, 0);
    return () => {
      clearTimeout(timer);
      window.removeEventListener('click', onClickOutside, { capture: true });
    };
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  bind:this={itemEl}
  class="session-item"
  class:active
  class:dragged={isDragged}
  class:drop-before={dropPos === 'before'}
  class:drop-after={dropPos === 'after'}
  data-session-id={session.id}
  role="button"
  tabindex="0"
  onmouseenter={() => (hovered = true)}
  onmouseleave={() => {
    hovered = false;
  }}
  onclick={handleClick}
  oncontextmenu={handleContextMenu}
  onkeydown={(e) => e.key === 'Enter' && onclick()}
  onpointerdown={handlePointerDown}
>
  {#if session.icon}
    <span class="icon"><SessionIcon icon={session.icon} /></span>
  {/if}
  <span
    class="dot"
    class:pulse={session.status === 'active'}
    style="background: {STATUS_COLORS[session.status]}"
  ></span>
  {#if renaming}
    <input
      bind:this={renameInput}
      class="rename-input"
      type="text"
      bind:value={renameValue}
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => {
        e.stopPropagation();
        if (e.key === 'Enter' && renameValue.trim()) {
          sessionStore.rename(session.id, renameValue.trim());
          renaming = false;
        }
        if (e.key === 'Escape') renaming = false;
      }}
      onblur={() => {
        if (renameValue.trim()) sessionStore.rename(session.id, renameValue.trim());
        renaming = false;
      }}
    />
  {:else}
    <span class="name">{session.name}</span>
  {/if}

  {#if badgeLetter && badgeColor}
    <SpaceBadge letter={badgeLetter} color={badgeColor} />
  {/if}

  {#if hovered && !dragStore.isDragging}
    <button
      class="menu-btn"
      onclick={(e) => {
        e.stopPropagation();
        if (showMenu) {
          showMenu = false;
          return;
        }
        showMenu = true;
      }}>⋯</button
    >
  {/if}

  {#if showMenu}
    <div class="context-menu">
      <button
        class="menu-item"
        onclick={(e) => {
          e.stopPropagation();
          menuAction('pin');
        }}
      >
        {session.pinned ? 'Unpin' : 'Pin'}
      </button>
      <button
        class="menu-item"
        onclick={(e) => {
          e.stopPropagation();
          menuAction('rename');
        }}>Rename</button
      >

      <button
        class="menu-item has-sub"
        onclick={(e) => {
          e.stopPropagation();
          showIconPicker = !showIconPicker;
          showFolderMenu = false;
          showSpaceMenu = false;
        }}
      >
        Icon ▸
      </button>

      {#if showIconPicker}
        <div class="icon-picker" onclick={(e) => e.stopPropagation()}>
          <div class="icon-grid">
            {#each ICON_OPTIONS as ic (ic)}
              <button
                class="icon-option"
                class:selected={session.icon === ic}
                onclick={() => setIcon(ic)}
                title={ic.replace('lucide:', '')}
              >
                <SessionIcon icon={ic} size={14} />
              </button>
            {/each}
          </div>
          {#if session.icon}
            <button class="menu-item remove-icon" onclick={() => setIcon(undefined)}
              >Remove icon</button
            >
          {/if}
        </div>
      {/if}

      <button
        class="menu-item has-sub"
        onclick={(e) => {
          e.stopPropagation();
          showFolderMenu = !showFolderMenu;
        }}
      >
        Move to Folder ▸
      </button>

      {#if showFolderMenu}
        <div class="submenu">
          {#each folders as f (f.id)}
            {#if f.id !== session.folderId}
              <button
                class="menu-item"
                onclick={(e) => {
                  e.stopPropagation();
                  moveToFolder(f.id);
                }}
              >
                📁 {f.name}
              </button>
            {/if}
          {/each}
          <div class="menu-divider"></div>
          <button
            class="menu-item"
            onclick={(e) => {
              e.stopPropagation();
              createFolderAndMove();
            }}
          >
            + New Folder
          </button>
        </div>
      {/if}

      {#if otherSpaces.length > 0}
        <button
          class="menu-item has-sub"
          onclick={(e) => {
            e.stopPropagation();
            showSpaceMenu = !showSpaceMenu;
            showFolderMenu = false;
          }}
        >
          Move to Space ▸
        </button>

        {#if showSpaceMenu}
          <div class="submenu">
            {#each otherSpaces as space (space.id)}
              <button
                class="menu-item"
                onclick={(e) => {
                  e.stopPropagation();
                  moveToSpace(space.id);
                }}
              >
                <span class="space-dot" style="background: {space.color}"></span>
                {space.name}
              </button>
            {/each}
          </div>
        {/if}
      {/if}

      {#if session.folderId}
        <button
          class="menu-item"
          onclick={(e) => {
            e.stopPropagation();
            menuAction('remove-from-folder');
          }}
        >
          Remove from Folder
        </button>
      {/if}

      <div class="menu-divider"></div>
      <button
        class="menu-item danger"
        onclick={(e) => {
          e.stopPropagation();
          menuAction('kill');
        }}>Kill</button
      >
    </div>
  {/if}
</div>

<style>
  .session-item {
    position: relative;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 8px;
    border-radius: var(--weplex-radius-md);
    cursor: pointer;
    transition: background var(--weplex-duration-fast) var(--weplex-easing);
    min-height: 36px;
    user-select: none;
    touch-action: none;
  }

  .session-item:hover {
    background: var(--weplex-surface);
  }

  .session-item.active {
    background: var(--weplex-surface-hover);
  }

  .session-item.dragged {
    opacity: 0.4;
  }

  .session-item.drop-before {
    border-top: 2px solid var(--weplex-accent);
    padding-top: 5px;
  }

  .session-item.drop-after {
    border-bottom: 2px solid var(--weplex-accent);
    padding-bottom: 5px;
  }

  .icon {
    font-size: 12px;
    width: 20px;
    height: 20px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--weplex-text-muted);
    flex-shrink: 0;
  }

  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex-shrink: 0;
    transition: background 0.3s ease;
  }

  .dot.pulse {
    animation: dot-pulse 1.4s ease-in-out infinite;
  }

  @keyframes dot-pulse {
    0%,
    100% {
      opacity: 1;
      transform: scale(1);
    }
    50% {
      opacity: 0.45;
      transform: scale(0.75);
    }
  }

  .name {
    font-size: var(--weplex-text-base);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
  }

  .rename-input {
    flex: 1;
    min-width: 0;
    padding: 1px 4px;
    border: 1px solid var(--weplex-accent);
    border-radius: var(--weplex-radius-sm);
    background: var(--weplex-bg);
    color: var(--weplex-text);
    font-size: var(--weplex-text-base);
    outline: none;
  }

  .menu-btn {
    flex-shrink: 0;
    width: 20px;
    height: 20px;
    border: none;
    border-radius: var(--weplex-radius-sm);
    background: transparent;
    color: var(--weplex-text-muted);
    font-size: 14px;
    display: flex;
    align-items: center;
    justify-content: center;
    letter-spacing: 1px;
  }

  .menu-btn:hover {
    background: var(--weplex-surface);
    color: var(--weplex-text);
  }

  .context-menu {
    position: absolute;
    top: 100%;
    left: 0;
    right: 0;
    z-index: 100;
    min-width: 160px;
    background: var(--weplex-surface);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-lg);
    padding: 4px;
    box-shadow: var(--weplex-shadow-md);
  }

  .submenu {
    padding: 2px 0;
    border-top: 1px solid var(--weplex-border);
    margin-top: 2px;
  }

  .menu-item {
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

  .menu-item:hover {
    background: var(--weplex-surface-hover);
  }

  .menu-item.has-sub {
    display: flex;
    justify-content: space-between;
  }

  .menu-item.danger {
    color: var(--weplex-error);
  }

  .space-dot {
    display: inline-block;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    vertical-align: middle;
    margin-right: 2px;
  }

  .menu-divider {
    height: 1px;
    margin: 4px 0;
    background: var(--weplex-border);
  }

  .icon-picker {
    border-top: 1px solid var(--weplex-border);
    margin-top: 2px;
    padding: 6px;
  }

  .icon-grid {
    display: grid;
    grid-template-columns: repeat(9, 1fr);
    gap: 2px;
  }

  .icon-option {
    width: 26px;
    height: 26px;
    display: flex;
    align-items: center;
    justify-content: center;
    border: none;
    border-radius: var(--weplex-radius-sm);
    background: transparent;
    color: var(--weplex-text-muted);
    cursor: pointer;
  }

  .icon-option:hover {
    background: var(--weplex-surface-hover);
    color: var(--weplex-text);
  }

  .icon-option.selected {
    background: var(--weplex-accent);
    color: var(--weplex-bg);
  }

  .remove-icon {
    margin-top: 4px;
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
  }
</style>

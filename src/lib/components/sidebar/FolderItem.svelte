<script lang="ts">
  import type { Folder } from '../../types';
  import { sessionStore } from '../../stores/sessionStore';
  import { folderStore } from '../../stores/folderStore';
  import { dragStore } from '../../stores/dragStore';
  import SessionItem from './SessionItem.svelte';
  import { Folder as FolderIcon, FolderOpen } from 'lucide-svelte';

  let { folder }: { folder: Folder } = $props();

  let sessions = $derived(sessionStore.getByFolder(folder.id));
  let showMenu = $state(false);
  let renaming = $state(false);
  let renameValue = $state('');
  let renameInput = $state<HTMLInputElement>();

  // Drop highlight driven by dragStore.dropTarget
  let dropHighlight = $derived(
    dragStore.isDragging &&
      dragStore.dropTarget?.type === 'folder' &&
      dragStore.dropTarget?.id === folder.id,
  );

  function handleContextMenu(e: MouseEvent) {
    e.preventDefault();
    showMenu = !showMenu;
  }

  function startRename() {
    showMenu = false;
    renameValue = folder.name;
    renaming = true;
    requestAnimationFrame(() => renameInput?.focus());
  }

  function commitRename() {
    if (renameValue.trim()) {
      folderStore.rename(folder.id, renameValue.trim());
    }
    renaming = false;
  }

  function deleteFolder() {
    showMenu = false;
    for (const s of sessions) {
      sessionStore.update(s.id, { folderId: undefined, pinned: false });
    }
    folderStore.remove(folder.id);
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="folder"
  data-folder-id={folder.id}
  oncontextmenu={handleContextMenu}
  onfocusout={(e) => {
    if (!(e.currentTarget as HTMLElement).contains(e.relatedTarget as Node)) showMenu = false;
  }}
>
  <button
    class="folder-header"
    class:drop-highlight={dropHighlight}
    onclick={() => folderStore.toggle(folder.id)}
  >
    <span class="folder-icon">
      {#if folder.collapsed}
        <FolderIcon size={14} />
      {:else}
        <FolderOpen size={14} />
      {/if}
    </span>
    {#if renaming}
      <input
        bind:this={renameInput}
        class="rename-input"
        type="text"
        bind:value={renameValue}
        onclick={(e) => e.stopPropagation()}
        onkeydown={(e) => {
          e.stopPropagation();
          if (e.key === 'Enter') commitRename();
          if (e.key === 'Escape') renaming = false;
        }}
        onblur={commitRename}
      />
    {:else}
      <span class="folder-name">{folder.name}</span>
    {/if}
    {#if sessions.length > 0}
      <span class="count">{sessions.length}</span>
    {/if}
  </button>

  {#if !folder.collapsed}
    <div class="folder-children">
      {#each sessions as session (session.id)}
        <SessionItem
          {session}
          active={session.id === sessionStore.activeSessionId}
          onclick={() => sessionStore.activate(session.id)}
        />
      {/each}
    </div>
  {/if}

  {#if showMenu}
    <div class="context-menu">
      <button
        class="menu-item"
        onclick={(e) => {
          e.stopPropagation();
          startRename();
        }}>Rename</button
      >
      <div class="menu-divider"></div>
      <button
        class="menu-item danger"
        onclick={(e) => {
          e.stopPropagation();
          deleteFolder();
        }}>Delete</button
      >
    </div>
  {/if}
</div>

<style>
  .folder {
    position: relative;
  }

  .folder-header {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 7px 8px;
    border: none;
    border-radius: var(--weplex-radius-md);
    background: transparent;
    color: var(--weplex-text);
    font-size: var(--weplex-text-base);
    font-weight: 600;
    cursor: pointer;
    transition: background var(--weplex-duration-fast) var(--weplex-easing);
  }

  .folder-header:hover {
    background: var(--weplex-surface);
  }

  .folder-header.drop-highlight {
    background: color-mix(in srgb, var(--weplex-accent) 15%, transparent);
    outline: 1px dashed var(--weplex-accent);
  }

  .folder-icon {
    color: var(--weplex-text-muted);
    width: 16px;
    height: 14px;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .folder-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
    text-align: left;
  }

  .count {
    font-size: var(--weplex-text-xs);
    font-weight: 400;
    color: var(--weplex-text-muted);
    flex-shrink: 0;
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
    font-weight: 600;
    outline: none;
  }

  .folder-children {
    padding-left: 18px;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .context-menu {
    position: absolute;
    right: 0;
    top: 100%;
    z-index: 50;
    min-width: 140px;
    background: var(--weplex-surface);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-lg);
    padding: 4px;
    box-shadow: var(--weplex-shadow-md);
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

  .menu-item.danger {
    color: var(--weplex-error);
  }

  .menu-divider {
    height: 1px;
    margin: 4px 0;
    background: var(--weplex-border);
  }
</style>

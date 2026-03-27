<script lang="ts">
  import { untrack } from 'svelte';
  import { spaceStore } from '../../stores/spaceStore';
  import { profileStore } from '../../stores/profileStore';
  import { uiStore } from '../../stores/uiStore';
  import { SPACE_COLORS, SPACE_BG_COLORS } from '../../types';

  // If editing, spaceStore sets this before opening modal
  let editingId = $state<string | null>(null);
  let name = $state('');
  let color = $state(SPACE_COLORS[0]);
  let bgColor = $state<string | null>(null);
  let profileId = $state('default');
  let nameInputEl = $state<HTMLInputElement>();
  let showProfileDropdown = $state(false);

  // Check if we're editing (set by spaceStore.editingSpaceId)
  // Only react to editingSpaceId changes; untrack inner reads to avoid
  // re-triggering when spaces array mutates (e.g. during save/delete)
  $effect(() => {
    const id = spaceStore.editingSpaceId;
    untrack(() => {
      if (id) {
        editingId = id;
        const space = spaceStore.spaces.find((s) => s.id === id);
        if (space) {
          name = space.name;
          color = space.color;
          bgColor = space.bgColor || null;
          profileId = space.profileId || 'default';
        }
      } else {
        editingId = null;
        name = '';
        color = SPACE_COLORS[spaceStore.spaces.length % SPACE_COLORS.length];
        bgColor = null;
        profileId = 'default';
      }
    });
  });

  $effect(() => {
    nameInputEl?.focus();
  });

  let currentProfileName = $derived(profileStore.getById(profileId)?.name ?? 'Default');

  function save() {
    if (!name.trim()) return;
    if (editingId) {
      spaceStore.update(editingId, {
        name: name.trim(),
        color,
        bgColor: bgColor || undefined,
        profileId: profileId === 'default' ? undefined : profileId,
      });
    } else {
      spaceStore.create(
        name.trim(),
        color,
        profileId === 'default' ? undefined : profileId,
        bgColor || undefined,
      );
    }
    close();
  }

  function remove() {
    if (editingId && editingId !== 'default') {
      spaceStore.remove(editingId);
    }
    close();
  }

  function close() {
    spaceStore.editingSpaceId = null;
    uiStore.closeOverlay();
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) save();
    if (e.key === 'Escape') close();
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions a11y_no_noninteractive_element_interactions -->
<div class="overlay-backdrop" role="presentation" onclick={close}>
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions a11y_no_noninteractive_element_interactions a11y_interactive_supports_focus -->
  <div
    class="dialog"
    role="dialog"
    tabindex="-1"
    aria-label={editingId ? 'Edit Space' : 'New Space'}
    onclick={(e) => e.stopPropagation()}
    onkeydown={handleKeydown}
  >
    <h2 class="dialog-title">{editingId ? 'Edit Space' : 'New Space'}</h2>

    <label class="field-label" for="space-name">Name</label>
    <input
      id="space-name"
      bind:this={nameInputEl}
      class="field-input"
      type="text"
      bind:value={name}
      placeholder="Work, Personal, Hackathons..."
    />

    <span class="field-label">Color</span>
    <div class="color-picker">
      {#each SPACE_COLORS as c}
        <button
          class="color-swatch"
          class:active={color === c}
          style="--swatch-color: {c}"
          onclick={() => (color = c)}
        ></button>
      {/each}
    </div>

    <span class="field-label">Background</span>
    <div class="bg-picker">
      <button
        class="bg-swatch bg-swatch-none"
        class:active={!bgColor}
        onclick={() => (bgColor = null)}
        title="Default"
      >
        <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
          <line
            x1="2"
            y1="14"
            x2="14"
            y2="2"
            stroke="currentColor"
            stroke-width="1.5"
            stroke-linecap="round"
          />
        </svg>
      </button>
      {#each SPACE_BG_COLORS as bg}
        <button
          class="bg-swatch"
          class:active={bgColor === bg}
          style="--swatch-bg: {bg}"
          onclick={() => (bgColor = bg)}
        ></button>
      {/each}
    </div>

    <span class="field-label">Profile</span>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="custom-select"
      onfocusout={(e) => {
        if (!(e.currentTarget as HTMLElement).contains(e.relatedTarget as Node))
          showProfileDropdown = false;
      }}
    >
      <button class="select-trigger" onclick={() => (showProfileDropdown = !showProfileDropdown)}>
        <span>{currentProfileName}</span>
        <span class="select-chevron">{showProfileDropdown ? '\u25B4' : '\u25BE'}</span>
      </button>
      {#if showProfileDropdown}
        <div class="select-dropdown">
          {#each profileStore.profiles as profile (profile.id)}
            <button
              class="select-option"
              class:active={profileId === profile.id}
              onclick={() => {
                profileId = profile.id;
                showProfileDropdown = false;
              }}>{profile.name}</button
            >
          {/each}
        </div>
      {/if}
    </div>

    <div class="dialog-actions">
      {#if editingId && editingId !== 'default'}
        <button class="btn-delete" onclick={remove}>Delete</button>
      {/if}
      <div class="actions-right">
        <button class="btn-cancel" onclick={close}>Cancel</button>
        <button class="btn-create" onclick={save} disabled={!name.trim()}>
          {editingId ? 'Save' : 'Create'}
        </button>
      </div>
    </div>
  </div>
</div>

<style>
  .overlay-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.4);
    display: flex;
    justify-content: center;
    padding-top: 15vh;
    z-index: 100;
  }

  .dialog {
    width: 380px;
    max-height: fit-content;
    background: var(--weplex-surface);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-xl);
    box-shadow: var(--weplex-shadow-overlay);
    padding: 20px;
  }

  .dialog-title {
    font-size: var(--weplex-text-lg);
    font-weight: 600;
    margin-bottom: 16px;
  }

  .field-label {
    display: block;
    font-size: var(--weplex-text-sm);
    color: var(--weplex-text-secondary);
    margin-bottom: 4px;
    margin-top: 14px;
  }

  .dialog-title + .field-label {
    margin-top: 0;
  }

  .field-input {
    width: 100%;
    padding: 8px 10px;
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-md);
    background: var(--weplex-bg);
    color: var(--weplex-text);
    font-size: var(--weplex-text-base);
    outline: none;
    transition: border-color var(--weplex-duration-fast) var(--weplex-easing);
  }

  .field-input:focus {
    border-color: var(--weplex-accent);
  }

  .color-picker {
    display: flex;
    gap: 8px;
    margin-top: 4px;
  }

  .color-swatch {
    width: 28px;
    height: 28px;
    border-radius: var(--weplex-radius-md);
    border: 2px solid transparent;
    background: var(--swatch-color);
    cursor: pointer;
    transition: all var(--weplex-duration-fast) var(--weplex-easing);
    opacity: 0.6;
  }

  .color-swatch:hover {
    opacity: 0.85;
  }

  .color-swatch.active {
    border-color: var(--weplex-text);
    opacity: 1;
  }

  .bg-picker {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    margin-top: 4px;
  }

  .bg-swatch {
    width: 28px;
    height: 28px;
    border-radius: 50%;
    border: 2px solid transparent;
    background: color-mix(in srgb, var(--swatch-bg) 60%, var(--weplex-surface));
    cursor: pointer;
    transition: all var(--weplex-duration-fast) var(--weplex-easing);
    padding: 0;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .bg-swatch:hover {
    background: color-mix(in srgb, var(--swatch-bg) 75%, var(--weplex-surface));
  }

  .bg-swatch.active {
    border-color: var(--swatch-bg);
    background: color-mix(in srgb, var(--swatch-bg) 75%, var(--weplex-surface));
    box-shadow: 0 0 8px color-mix(in srgb, var(--swatch-bg) 40%, transparent);
  }

  .bg-swatch-none {
    background: var(--weplex-bg) !important;
    color: var(--weplex-text-muted);
  }

  .bg-swatch-none:hover {
    background: var(--weplex-surface-hover) !important;
  }

  .bg-swatch-none.active {
    border-color: var(--weplex-text-muted);
    background: var(--weplex-bg) !important;
    box-shadow: none;
  }

  .custom-select {
    position: relative;
    margin-top: 4px;
  }

  .select-trigger {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 8px 10px;
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-md);
    background: var(--weplex-bg);
    color: var(--weplex-text);
    font-size: var(--weplex-text-sm);
    cursor: pointer;
    transition: border-color var(--weplex-duration-fast) var(--weplex-easing);
  }

  .select-trigger:hover {
    border-color: var(--weplex-accent);
  }

  .select-chevron {
    margin-left: auto;
    font-size: 10px;
    color: var(--weplex-text-muted);
  }

  .select-dropdown {
    position: absolute;
    top: calc(100% + 2px);
    left: 0;
    right: 0;
    background: var(--weplex-bg);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-md);
    box-shadow: var(--weplex-shadow-md);
    z-index: 10;
    padding: 4px;
  }

  .select-option {
    display: block;
    width: 100%;
    padding: 6px 8px;
    border: none;
    border-radius: var(--weplex-radius-sm);
    background: transparent;
    color: var(--weplex-text);
    font-size: var(--weplex-text-sm);
    text-align: left;
    cursor: pointer;
  }

  .select-option:hover {
    background: var(--weplex-surface-hover);
  }

  .select-option.active {
    color: var(--weplex-accent);
  }

  .dialog-actions {
    display: flex;
    align-items: center;
    margin-top: 20px;
  }

  .actions-right {
    display: flex;
    gap: 8px;
    margin-left: auto;
  }

  .btn-cancel {
    padding: 7px 14px;
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-md);
    background: transparent;
    color: var(--weplex-text-secondary);
    font-size: var(--weplex-text-sm);
  }

  .btn-cancel:hover {
    background: var(--weplex-surface-hover);
  }

  .btn-create {
    padding: 7px 18px;
    border: none;
    border-radius: var(--weplex-radius-md);
    background: var(--weplex-accent);
    color: white;
    font-size: var(--weplex-text-sm);
    font-weight: 500;
  }

  .btn-create:hover {
    background: var(--weplex-accent-hover);
  }

  .btn-create:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .btn-delete {
    padding: 7px 14px;
    border: 1px solid var(--weplex-error);
    border-radius: var(--weplex-radius-md);
    background: transparent;
    color: var(--weplex-error);
    font-size: var(--weplex-text-sm);
  }

  .btn-delete:hover {
    background: rgba(239, 68, 68, 0.1);
  }
</style>

<script lang="ts">
  import { untrack } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { spaceStore } from '../../stores/spaceStore';
  import { profileStore } from '../../stores/profileStore';
  import { teamStore } from '../../stores/teamStore.svelte';
  import { authStore } from '../../stores/authStore.svelte';
  import { uiStore } from '../../stores/uiStore';
  import { SPACE_COLORS, SPACE_BG_COLORS } from '../../types';
  import type { SpaceType } from '../../types';
  import { Button, Modal, Input } from '../ui';
  import { Eye, Users } from 'lucide-svelte';

  // If editing, spaceStore sets this before opening modal
  let editingId = $state<string | null>(null);
  let name = $state('');
  let color = $state(SPACE_COLORS[0]);
  let bgColor = $state<string | null>(null);
  let profileId = $state('default');
  let directory = $state('');
  let showProfileDropdown = $state(false);
  let spaceType = $state<SpaceType>('personal');
  let shared = $state(false);
  let selectedTeamId = $state<string | null>(null);
  let showTeamDropdown = $state(false);

  // Only show team options if user is authenticated and has teams
  let hasTeams = $derived(authStore.isAuthenticated && teamStore.teams.length > 0);
  let selectedTeamName = $derived(
    teamStore.teams.find((t) => t.id === selectedTeamId)?.name ?? 'Select team',
  );

  // Directory autocomplete
  let dirSuggestions = $state<string[]>([]);
  let selectedDirSuggestion = $state(-1);
  let dirDebounceTimer: ReturnType<typeof setTimeout>;

  async function fetchDirSuggestions(value: string) {
    if (!value) {
      dirSuggestions = [];
      return;
    }
    try {
      dirSuggestions = await invoke<string[]>('list_dirs', { partial: value });
      selectedDirSuggestion = -1;
    } catch {
      dirSuggestions = [];
    }
  }

  function onDirInput() {
    clearTimeout(dirDebounceTimer);
    dirDebounceTimer = setTimeout(() => fetchDirSuggestions(directory), 100);
  }

  function acceptDirSuggestion(s: string) {
    directory = s + '/';
    selectedDirSuggestion = -1;
    clearTimeout(dirDebounceTimer);
    fetchDirSuggestions(directory);
  }

  function onDirKeydown(e: KeyboardEvent) {
    if (dirSuggestions.length === 0) return;

    if (e.key === 'Tab') {
      e.preventDefault();
      const idx = selectedDirSuggestion >= 0 ? selectedDirSuggestion : 0;
      if (dirSuggestions[idx]) acceptDirSuggestion(dirSuggestions[idx]);
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      selectedDirSuggestion = Math.min(selectedDirSuggestion + 1, dirSuggestions.length - 1);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      selectedDirSuggestion = Math.max(selectedDirSuggestion - 1, -1);
    } else if (e.key === 'Enter' && selectedDirSuggestion >= 0) {
      e.preventDefault();
      e.stopPropagation();
      acceptDirSuggestion(dirSuggestions[selectedDirSuggestion]);
    } else if (e.key === 'Escape') {
      e.stopPropagation();
      dirSuggestions = [];
      selectedDirSuggestion = -1;
    }
  }

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
          directory = space.directory || '';
          spaceType = space.type ?? 'personal';
          shared = space.shared ?? false;
          selectedTeamId = space.teamId || teamStore.activeTeamId;
        }
      } else {
        editingId = null;
        name = '';
        color = SPACE_COLORS[spaceStore.spaces.length % SPACE_COLORS.length];
        bgColor = null;
        profileId = 'default';
        directory = '';
        spaceType = 'personal';
        shared = false;
        selectedTeamId = teamStore.activeTeamId;
      }
    });
  });

  $effect(() => {
    const el = document.getElementById('space-name') as HTMLInputElement | null;
    el?.focus();
  });

  let currentProfileName = $derived(profileStore.getById(profileId)?.name ?? 'Default');

  async function save() {
    if (!name.trim()) return;
    const trimmedDir = directory.trim().replace(/\/+$/, '') || undefined;

    if (editingId) {
      spaceStore.update(editingId, {
        name: name.trim(),
        color,
        bgColor: bgColor || undefined,
        profileId: profileId === 'default' ? undefined : profileId,
        directory: trimmedDir,
        type: spaceType,
        shared,
        teamId: (shared || spaceType === 'team') ? (selectedTeamId || undefined) : undefined,
      });
    } else if (spaceType === 'team' && selectedTeamId) {
      // Create team space on server first
      try {
        await spaceStore.createTeamSpace(
          name.trim(),
          color,
          selectedTeamId,
          authStore.user?.id,
        );
      } catch (e) {
        console.error('[Weplex] Failed to create team space:', e);
      }
    } else {
      const space = spaceStore.create(
        name.trim(),
        color,
        profileId === 'default' ? undefined : profileId,
        bgColor || undefined,
        trimmedDir,
      );
      // Update sharing fields if applicable
      if (shared && selectedTeamId) {
        spaceStore.update(space.id, { shared, teamId: selectedTeamId });
      }
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
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<Modal onclose={close} position="top" label={editingId ? 'Edit Space' : 'New Space'} class="dialog">
  <div onkeydown={handleKeydown}>
    <h2 class="dialog-title">{editingId ? 'Edit Space' : 'New Space'}</h2>

    <label class="field-label" for="space-name">Name</label>
    <Input
      id="space-name"
      bind:value={name}
      placeholder="Work, Personal, Hackathons..."
    />

    <label class="field-label" for="space-dir">Directory</label>
    <div
      class="dir-autocomplete"
      onfocusout={() => {
        dirSuggestions = [];
        selectedDirSuggestion = -1;
      }}
    >
      <Input
        id="space-dir"
        mono
        bind:value={directory}
        placeholder="~/projects/my-app (optional)"
        oninput={onDirInput}
        onkeydown={onDirKeydown}
      />
      {#if dirSuggestions.length > 0}
        <div class="suggestions-dropdown">
          {#each dirSuggestions as s, i}
            <button
              class="suggestion"
              class:selected={i === selectedDirSuggestion}
              onmousedown={(e) => {
                e.preventDefault();
                acceptDirSuggestion(s);
              }}>{s}</button
            >
          {/each}
        </div>
      {/if}
    </div>

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

    {#if hasTeams}
      <span class="field-label">Sharing</span>
      <div class="sharing-section">
        <div class="sharing-row">
          <button
            class="sharing-option"
            class:active={spaceType === 'personal' && !shared}
            onclick={() => { spaceType = 'personal'; shared = false; }}
            title="Private — only you can see this space"
          >
            <span class="sharing-icon">🔒</span>
            <span>Private</span>
          </button>
          <button
            class="sharing-option"
            class:active={spaceType === 'personal' && shared}
            onclick={() => { spaceType = 'personal'; shared = true; }}
            title="Shared — team members can see your sessions"
          >
            <Eye size={14} />
            <span>Shared</span>
          </button>
          <button
            class="sharing-option"
            class:active={spaceType === 'team'}
            onclick={() => { spaceType = 'team'; shared = true; }}
            title="Team — collaborative space for the whole team"
          >
            <Users size={14} />
            <span>Team</span>
          </button>
        </div>

        {#if shared || spaceType === 'team'}
          <!-- Team selector -->
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="custom-select team-select"
            onfocusout={(e) => {
              if (!(e.currentTarget as HTMLElement).contains(e.relatedTarget as Node))
                showTeamDropdown = false;
            }}
          >
            <button class="select-trigger" onclick={() => (showTeamDropdown = !showTeamDropdown)}>
              <Users size={12} />
              <span>{selectedTeamName}</span>
              <span class="select-chevron">{showTeamDropdown ? '\u25B4' : '\u25BE'}</span>
            </button>
            {#if showTeamDropdown}
              <div class="select-dropdown">
                {#each teamStore.teams as team (team.id)}
                  <button
                    class="select-option"
                    class:active={selectedTeamId === team.id}
                    onclick={() => {
                      selectedTeamId = team.id;
                      showTeamDropdown = false;
                    }}>{team.name}</button
                  >
                {/each}
              </div>
            {/if}
          </div>
        {/if}
      </div>
    {/if}

    <div class="dialog-actions">
      {#if editingId && editingId !== 'default'}
        <Button variant="danger" onclick={remove}>Delete</Button>
      {/if}
      <div class="actions-right">
        <Button variant="secondary" onclick={close}>Cancel</Button>
        <Button variant="primary" onclick={save} disabled={!name.trim()}>
          {editingId ? 'Save' : 'Create'}
        </Button>
      </div>
    </div>
  </div>
</Modal>

<style>
  :global(.dialog) {
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

  .sharing-section {
    margin-top: 4px;
  }

  .sharing-row {
    display: flex;
    gap: 4px;
  }

  .sharing-option {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 4px;
    padding: 6px 8px;
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-md);
    background: transparent;
    color: var(--weplex-text-secondary);
    font-size: var(--weplex-text-sm);
    cursor: pointer;
    transition: all var(--weplex-duration-fast) var(--weplex-easing);
  }

  .sharing-option:hover {
    border-color: var(--weplex-accent);
    color: var(--weplex-text);
  }

  .sharing-option.active {
    border-color: var(--weplex-accent);
    background: color-mix(in srgb, var(--weplex-accent) 10%, transparent);
    color: var(--weplex-accent);
  }

  .sharing-icon {
    font-size: 12px;
    line-height: 1;
  }

  .team-select {
    margin-top: 8px;
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

  .dir-autocomplete {
    position: relative;
  }

  .suggestions-dropdown {
    position: absolute;
    top: 100%;
    left: 0;
    right: 0;
    max-height: 180px;
    overflow-y: auto;
    background: var(--weplex-bg);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-md);
    box-shadow: var(--weplex-shadow-md);
    z-index: 10;
    margin-top: 2px;
  }

  .suggestion {
    display: block;
    width: 100%;
    padding: 6px 10px;
    border: none;
    background: transparent;
    color: var(--weplex-text);
    font-size: var(--weplex-text-sm);
    font-family: var(--weplex-font-mono);
    text-align: left;
    cursor: pointer;
  }

  .suggestion:hover,
  .suggestion.selected {
    background: var(--weplex-surface-hover);
  }
</style>

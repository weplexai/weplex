<script lang="ts">
  import { untrack } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { spaceStore } from '../../stores/spaceStore';
  import { profileStore } from '../../stores/profileStore';
  import { teamStore } from '../../stores/teamStore.svelte';
  import { authStore } from '../../stores/authStore.svelte';
  import { uiStore } from '../../stores/uiStore';
  import { SPACE_COLORS } from '../../types';
  import type { SpaceType } from '../../types';
  import { Button, Modal, Input, Select } from '../ui';
  import { workflowStore } from '../../stores/workflowStore.svelte';
  import { Eye, Users } from 'lucide-svelte';

  // ── HSB ↔ Hex conversion ──
  function hexToHsb(hex: string): [number, number, number] {
    const r = parseInt(hex.slice(1, 3), 16) / 255;
    const g = parseInt(hex.slice(3, 5), 16) / 255;
    const b = parseInt(hex.slice(5, 7), 16) / 255;
    const max = Math.max(r, g, b), min = Math.min(r, g, b);
    const d = max - min;
    let h = 0;
    if (d !== 0) {
      if (max === r) h = ((g - b) / d + 6) % 6;
      else if (max === g) h = (b - r) / d + 2;
      else h = (r - g) / d + 4;
      h *= 60;
    }
    const s = max === 0 ? 0 : d / max;
    return [h, s, max];
  }

  function hsbToHex(h: number, s: number, b: number): string {
    const c = b * s;
    const x = c * (1 - Math.abs(((h / 60) % 2) - 1));
    const m = b - c;
    let r = 0, g = 0, bl = 0;
    if (h < 60) { r = c; g = x; }
    else if (h < 120) { r = x; g = c; }
    else if (h < 180) { g = c; bl = x; }
    else if (h < 240) { g = x; bl = c; }
    else if (h < 300) { r = x; bl = c; }
    else { r = c; bl = x; }
    const toHex = (v: number) => Math.round((v + m) * 255).toString(16).padStart(2, '0');
    return `#${toHex(r)}${toHex(g)}${toHex(bl)}`;
  }

  // If editing, spaceStore sets this before opening modal
  let editingId = $state<string | null>(null);
  let name = $state('');
  let color = $state(SPACE_COLORS[0]);
  let bgColor = $state<string | null>(null);
  let grain = $state(0);
  let bgMode = $state<'dark' | 'light'>('dark');

  // Base color for mixing depends on mode
  let mixBase = $derived(bgMode === 'light' ? '#e8e8ee' : '#12121a');
  let mixBaseVar = $derived(bgMode === 'light' ? 'var(--weplex-bg)' : 'var(--weplex-sidebar-bg)');

  // HSB picker state — derived from bgColor
  let pickerHue = $state(0);
  let pickerSat = $state(0.8);
  let pickerBri = $state(0.7);
  let pickerDragging = $state(false);

  function syncPickerFromBgColor(hex: string | null, pick = false) {
    if (!hex) return;
    const [h, s, b] = hexToHsb(hex);
    pickerHue = h;
    pickerSat = s;
    pickerBri = b;
    if (pick) autoPickColor();
  }

  function syncBgColorFromPicker() {
    bgColor = hsbToHex(pickerHue, pickerSat, pickerBri);
    autoPickColor();
  }

  // Auto-pick best accent color for contrast against bgColor mixed with sidebar bg
  function relativeLuminance(hex: string): number {
    const r = parseInt(hex.slice(1, 3), 16) / 255;
    const g = parseInt(hex.slice(3, 5), 16) / 255;
    const b = parseInt(hex.slice(5, 7), 16) / 255;
    const toLinear = (c: number) => c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
    return 0.2126 * toLinear(r) + 0.7152 * toLinear(g) + 0.0722 * toLinear(b);
  }

  function contrastRatio(hex1: string, hex2: string): number {
    const l1 = relativeLuminance(hex1);
    const l2 = relativeLuminance(hex2);
    const lighter = Math.max(l1, l2);
    const darker = Math.min(l1, l2);
    return (lighter + 0.05) / (darker + 0.05);
  }

  function autoPickColor() {
    if (!bgColor) return;
    const [bgHue] = hexToHsb(bgColor);
    // Pick SPACE_COLOR closest in hue to bgColor (monochromatic harmony)
    let bestColor = SPACE_COLORS[0];
    let bestDist = 999;
    for (const c of SPACE_COLORS) {
      const [cHue] = hexToHsb(c);
      const dist = Math.min(Math.abs(cHue - bgHue), 360 - Math.abs(cHue - bgHue));
      if (dist < bestDist) {
        bestDist = dist;
        bestColor = c;
      }
    }
    color = bestColor;
  }

  function mixColors(fg: string, bg: string, amount: number): string {
    const fr = parseInt(fg.slice(1, 3), 16);
    const fg_ = parseInt(fg.slice(3, 5), 16);
    const fb = parseInt(fg.slice(5, 7), 16);
    const br = parseInt(bg.slice(1, 3), 16);
    const bg_ = parseInt(bg.slice(3, 5), 16);
    const bb = parseInt(bg.slice(5, 7), 16);
    const r = Math.round(fr * amount + br * (1 - amount));
    const g = Math.round(fg_ * amount + bg_ * (1 - amount));
    const b = Math.round(fb * amount + bb * (1 - amount));
    return `#${r.toString(16).padStart(2,'0')}${g.toString(16).padStart(2,'0')}${b.toString(16).padStart(2,'0')}`;
  }

  // Pure hue color for picker background
  let pureHueHex = $derived(hsbToHex(pickerHue, 1, 1));

  function onPickerPointerDown(e: PointerEvent) {
    if (!bgColor) return;
    pickerDragging = true;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    updatePickerFromEvent(e);
  }

  function onPickerPointerMove(e: PointerEvent) {
    if (!pickerDragging) return;
    updatePickerFromEvent(e);
  }

  function onPickerPointerUp() {
    pickerDragging = false;
  }

  function updatePickerFromEvent(e: PointerEvent) {
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    pickerSat = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
    pickerBri = Math.max(0, Math.min(1, 1 - (e.clientY - rect.top) / rect.height));
    syncBgColorFromPicker();
  }
  let profileId = $state('default');
  let directory = $state('');
  let defaultWorkflowId = $state('');
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
          syncPickerFromBgColor(bgColor);
          grain = space.grain ?? 0;
          bgMode = space.bgMode ?? 'dark';
          profileId = space.profileId || 'default';
          directory = space.directory || '';
          defaultWorkflowId = space.defaultWorkflowId || '';
          spaceType = space.type ?? 'personal';
          shared = space.shared ?? false;
          selectedTeamId = space.teamId || teamStore.activeTeamId;
        }
      } else {
        editingId = null;
        name = '';
        color = SPACE_COLORS[spaceStore.spaces.length % SPACE_COLORS.length];
        bgColor = null;
        grain = 0;
        bgMode = 'dark';
        profileId = 'default';
        directory = '';
        defaultWorkflowId = '';
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
        grain: grain > 0 ? grain : undefined,
        bgMode: bgMode !== 'dark' ? bgMode : undefined,
        profileId: profileId === 'default' ? undefined : profileId,
        directory: trimmedDir,
        defaultWorkflowId: defaultWorkflowId || undefined,
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
      // Apply grain/gradient/sharing fields
      const extraPatch: Record<string, unknown> = {};
      if (grain > 0) extraPatch.grain = grain;
      if (bgMode !== 'dark') extraPatch.bgMode = bgMode;
      if (shared && selectedTeamId) {
        extraPatch.shared = shared;
        extraPatch.teamId = selectedTeamId;
      }
      if (Object.keys(extraPatch).length > 0) {
        spaceStore.update(space.id, extraPatch);
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

    <!-- Picker + vertical grain -->
    <div class="picker-row">
      <!-- HSB picker -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="space-preview"
        class:has-color={!!bgColor}
        style="--picker-hue: {pureHueHex}; --preview-grain: {grain}; --mix-base: {mixBaseVar}"
        onpointerdown={onPickerPointerDown}
        onpointermove={onPickerPointerMove}
        onpointerup={onPickerPointerUp}
      >
        {#if bgColor}
          <div class="picker-saturation"></div>
          <div class="picker-brightness"></div>
          <div
            class="picker-thumb"
            style="left: {pickerSat * 100}%; top: {(1 - pickerBri) * 100}%; background: {bgColor}"
          ></div>
        {:else}
          <div class="picker-empty">
            <span>Select a color below</span>
          </div>
        {/if}
        {#if grain > 0}
          <div class="preview-grain"></div>
        {/if}
      </div>

      <!-- Vertical grain slider with noise gradient -->
      <div class="grain-wrap" style="--grain-bg: {bgColor || 'var(--weplex-text-muted)'}">
        <div class="grain-track">
          <div class="grain-track-dots"></div>
        </div>
        <input
          type="range"
          class="grain-slider"
          min="0"
          max="1"
          step="0.05"
          bind:value={grain}
        />
      </div>
    </div>

    <!-- Controls row: none + mode toggle + hue -->
    <div class="slider-row">
      <button
        class="picker-none-btn"
        class:active={!bgColor}
        onclick={() => { bgColor = null; }}
        title="No background"
      >
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none">
          <circle cx="8" cy="8" r="6.5" stroke="currentColor" stroke-width="1" opacity="0.5" />
          <line x1="3" y1="13" x2="13" y2="3" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
        </svg>
      </button>
      <!-- Dark / Light toggle -->
      <button
        class="mode-toggle"
        onclick={() => { bgMode = bgMode === 'dark' ? 'light' : 'dark'; }}
        title={bgMode === 'dark' ? 'Dark mode' : 'Light mode'}
      >
        {#if bgMode === 'dark'}
          <svg width="14" height="14" viewBox="0 0 16 16" fill="none">
            <path d="M8 1a7 7 0 1 0 0 14A5 5 0 0 1 8 1z" fill="currentColor" />
          </svg>
        {:else}
          <svg width="14" height="14" viewBox="0 0 16 16" fill="none">
            <circle cx="8" cy="8" r="4" fill="currentColor" />
            <path d="M8 1v2M8 13v2M1 8h2M13 8h2M3.05 3.05l1.41 1.41M11.54 11.54l1.41 1.41M3.05 12.95l1.41-1.41M11.54 4.46l1.41-1.41" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" />
          </svg>
        {/if}
      </button>
      <input
        type="range"
        class="hue-slider"
        min="0"
        max="360"
        step="1"
        bind:value={pickerHue}
        oninput={() => { if (bgColor) syncBgColorFromPicker(); else { bgColor = hsbToHex(pickerHue, 0.7, 0.7); pickerSat = 0.7; pickerBri = 0.7; autoPickColor(); } }}
      />
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

    <span class="field-label">Default Workflow</span>
    <Select
      value={defaultWorkflowId}
      options={workflowStore.options}
      onchange={(v) => { defaultWorkflowId = v; }}
    />

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

  /* ── HSB Color Picker ── */

  .space-preview {
    position: relative;
    flex: 1;
    min-width: 0;
    height: 110px;
    border-radius: var(--weplex-radius-lg);
    background: var(--weplex-bg);
    overflow: hidden;
    border: 1px solid var(--weplex-border);
  }

  .space-preview.has-color {
    background: var(--mix-base);
    cursor: crosshair;
  }

  .picker-saturation {
    position: absolute;
    inset: 0;
    background: linear-gradient(to right,
      color-mix(in srgb, #fff 35%, var(--mix-base)),
      color-mix(in srgb, var(--picker-hue) 35%, var(--mix-base))
    );
  }

  .picker-brightness {
    position: absolute;
    inset: 0;
    background: linear-gradient(to bottom, transparent, #000);
  }

  .picker-thumb {
    position: absolute;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    border: 2px solid #fff;
    box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.3), 0 2px 6px rgba(0, 0, 0, 0.4);
    transform: translate(-50%, -50%);
    pointer-events: none;
    z-index: 1;
  }

  .picker-empty {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: var(--weplex-text-muted);
    font-size: var(--weplex-text-sm);
    opacity: 0.5;
  }

  .preview-grain {
    position: absolute;
    inset: 0;
    opacity: calc(var(--preview-grain) * 1.5);
    background-image: url("data:image/svg+xml,%3Csvg viewBox='0 0 200 200' xmlns='http://www.w3.org/2000/svg'%3E%3Cfilter id='n'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='0.75' numOctaves='4' stitchTiles='stitch'/%3E%3CfeColorMatrix type='saturate' values='0'/%3E%3C/filter%3E%3Crect width='100%25' height='100%25' filter='url(%23n)'/%3E%3C/svg%3E");
    background-repeat: repeat;
    background-size: 200px 200px;
    pointer-events: none;
    mix-blend-mode: soft-light;
  }

  /* ── Layout ── */

  .picker-row {
    display: flex;
    gap: 8px;
    margin-top: 4px;
  }

  .slider-row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 6px;
  }

  /* ── Grain (vertical) ── */

  .grain-wrap {
    position: relative;
    width: 24px;
    flex-shrink: 0;
    border-radius: 12px;
    overflow: hidden;
  }

  .grain-track {
    position: absolute;
    inset: 0;
    background: color-mix(in srgb, var(--grain-bg) 30%, var(--weplex-surface));
    border: 1px solid var(--weplex-border);
    border-radius: 12px;
  }

  .grain-track-dots {
    position: absolute;
    inset: 0;
    /* Multi-layer dot pattern for noise effect */
    background:
      radial-gradient(circle, rgba(255,255,255,0.5) 0.5px, transparent 0.5px),
      radial-gradient(circle, rgba(255,255,255,0.35) 0.5px, transparent 0.5px),
      radial-gradient(circle, rgba(255,255,255,0.2) 0.7px, transparent 0.7px);
    background-size: 4px 4px, 7px 5px, 3px 7px;
    background-position: 0 0, 2px 3px, 1px 1px;
    /* Visible at bottom, clean at top */
    mask-image: linear-gradient(to top, transparent 5%, black 95%);
    -webkit-mask-image: linear-gradient(to top, transparent 5%, black 95%);
  }

  .grain-slider {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    margin: 0;
    cursor: pointer;
    -webkit-appearance: none;
    appearance: none;
    background: transparent;
    writing-mode: vertical-lr;
    direction: rtl;
  }

  .grain-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 20px;
    height: 10px;
    border-radius: 5px;
    background: #fff;
    border: 1px solid rgba(0, 0, 0, 0.2);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.3);
    cursor: pointer;
  }

  .picker-none-btn {
    flex-shrink: 0;
    width: 24px;
    height: 24px;
    border-radius: 50%;
    border: 1px solid var(--weplex-border);
    background: var(--weplex-bg);
    color: var(--weplex-text-muted);
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    transition: all var(--weplex-duration-fast) var(--weplex-easing);
  }

  .picker-none-btn:hover {
    border-color: var(--weplex-text-muted);
  }

  .picker-none-btn.active {
    border-color: var(--weplex-accent);
  }

  .mode-toggle {
    flex-shrink: 0;
    width: 24px;
    height: 24px;
    border-radius: 50%;
    border: 1px solid var(--weplex-border);
    background: var(--weplex-bg);
    color: var(--weplex-text-muted);
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    transition: all var(--weplex-duration-fast) var(--weplex-easing);
  }

  .mode-toggle:hover {
    border-color: var(--weplex-text-muted);
    color: var(--weplex-text);
  }

  .hue-slider {
    flex: 1;
    min-width: 0;
    height: 16px;
    -webkit-appearance: none;
    appearance: none;
    border-radius: 8px;
    cursor: pointer;
    outline: none;
    margin: 0;
    background: linear-gradient(to right,
      hsl(0, 100%, 50%), hsl(60, 100%, 50%), hsl(120, 100%, 50%),
      hsl(180, 100%, 50%), hsl(240, 100%, 50%), hsl(300, 100%, 50%),
      hsl(360, 100%, 50%)
    );
  }

  .hue-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: #fff;
    border: 2px solid rgba(0, 0, 0, 0.15);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.3);
    cursor: pointer;
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

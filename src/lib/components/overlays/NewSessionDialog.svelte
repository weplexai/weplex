<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { sessionStore } from '../../stores/sessionStore';
  import { spaceStore } from '../../stores/spaceStore';
  import { profileStore } from '../../stores/profileStore';
  import { settingsStore } from '../../stores/settingsStore.svelte';
  import { uiStore } from '../../stores/uiStore';
  import SessionIcon from '../SessionIcon.svelte';
  import { Button, Modal, Input, Select } from '../ui';

  // Resolve default directory: Space.directory > AppSettings.defaultDirectory > '~'
  function getDefaultDirectory(): string {
    const space = spaceStore.spaces.find((s) => s.id === spaceStore.activeSpaceId);
    if (space?.directory) return space.directory;
    const settingsDir = settingsStore.settings.defaultDirectory;
    if (settingsDir && settingsDir !== '~') return settingsDir;
    return '~';
  }

  let directory = $state(getDefaultDirectory());
  let command = $state('');
  let selectedIcon = $state('');
  let selectedSpace = $state(spaceStore.activeSpaceId);
  let pinned = $state(false);
  let inputEl = $state<HTMLInputElement>();
  let iconTab = $state<'emoji' | 'icon'>('emoji');
  let selectedProfile = $state<string | undefined>(undefined); // undefined = inherit from space
  let showSpaceDropdown = $state(false);
  let showProfileDropdown = $state(false);
  let currentSpaceName = $derived(
    spaceStore.spaces.find((s) => s.id === selectedSpace)?.name ?? 'Default',
  );
  let currentProfileName = $derived(
    selectedProfile
      ? (profileStore.getById(selectedProfile)?.name ?? '\u2014')
      : 'Inherit from Space',
  );

  // Directory autocomplete
  let suggestions = $state<string[]>([]);
  let selectedSuggestion = $state(-1);
  let debounceTimer: ReturnType<typeof setTimeout>;

  async function fetchSuggestions(value: string) {
    if (!value) {
      suggestions = [];
      return;
    }
    try {
      suggestions = await invoke<string[]>('list_dirs', { partial: value });
      selectedSuggestion = -1;
    } catch {
      suggestions = [];
    }
  }

  function onDirInput() {
    clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => fetchSuggestions(directory), 100);
  }

  function acceptSuggestion(s: string) {
    directory = s + '/';
    selectedSuggestion = -1;
    inputEl?.focus();
    clearTimeout(debounceTimer);
    fetchSuggestions(directory);
  }

  function onDirKeydown(e: KeyboardEvent) {
    if (suggestions.length === 0) return;

    if (e.key === 'Tab') {
      e.preventDefault();
      const idx = selectedSuggestion >= 0 ? selectedSuggestion : 0;
      if (suggestions[idx]) acceptSuggestion(suggestions[idx]);
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      selectedSuggestion = Math.min(selectedSuggestion + 1, suggestions.length - 1);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      selectedSuggestion = Math.max(selectedSuggestion - 1, -1);
    } else if (e.key === 'Enter' && selectedSuggestion >= 0) {
      e.preventDefault();
      e.stopPropagation();
      acceptSuggestion(suggestions[selectedSuggestion]);
    } else if (e.key === 'Escape') {
      e.stopPropagation();
      suggestions = [];
      selectedSuggestion = -1;
    }
  }

  const emojis =
    '\u26A1 \uD83E\uDD16 \uD83D\uDD27 \uD83D\uDE80 \uD83D\uDCBB \uD83E\uDDEA \uD83D\uDCE6 \uD83C\uDF10 \uD83D\uDD12 \uD83C\uDFAF \uD83D\uDCE1 \uD83D\uDC1B \u2615 \uD83D\uDD25 \uD83D\uDCA1 \uD83C\uDFAE \uD83C\uDFA8 \uD83C\uDFB5 \uD83D\uDD11 \uD83D\uDC8E \uD83E\uDDF2 \uD83D\uDCCC \uD83D\uDD14 \uD83D\uDCAC \u2764\uFE0F \uD83D\uDC9C \uD83D\uDC99 \uD83D\uDC9A \uD83D\uDC9B \uD83E\uDDE1 \u2B50 \uD83C\uDF19 \u2600\uFE0F \uD83C\uDF08 \uD83C\uDF40 \u2744\uFE0F \uD83D\uDE0E \uD83E\uDD13 \uD83D\uDC7B \uD83D\uDC80 \uD83D\uDC7D \uD83E\uDD8A \uD83D\uDC31 \uD83D\uDC36 \uD83D\uDC19 \uD83E\uDD84 \uD83D\uDC0D \uD83D\uDC33'.split(
      ' ',
    );

  const lucideIconNames = [
    'terminal',
    'code',
    'bug',
    'rocket',
    'bot',
    'wrench',
    'zap',
    'server',
    'database',
    'globe',
    'lock',
    'shield',
    'cpu',
    'cloud',
    'layers',
    'git-branch',
    'package',
    'box',
    'key',
    'eye',
    'search',
    'monitor',
    'network',
    'wifi',
    'star',
    'heart',
    'bookmark',
    'flag',
    'bell',
    'command',
    'coffee',
    'flame',
    'lightbulb',
    'sparkles',
    'brain',
    'atom',
    'moon',
    'sun',
    'palette',
    'music',
    'gamepad2',
    'radio',
    'home',
    'users',
    'file-text',
    'link',
    'send',
    'fingerprint',
  ];

  const presets = [
    { label: 'Terminal', cmd: '', icon: '' },
    { label: 'claude', cmd: 'claude', icon: '⚡' },
    { label: 'claude --chrome', cmd: 'claude --chrome', icon: '⚡' },
    { label: 'ssh', cmd: 'ssh ', icon: '🌐' },
  ];

  function create() {
    const cwd = directory.length > 1 ? directory.replace(/\/+$/, '') : directory;
    const session = sessionStore.create({
      command: command || undefined,
      cwd,
      spaceId: selectedSpace,
      profileId: selectedProfile,
      pinned,
    });
    if (selectedIcon) {
      sessionStore.update(session.id, { icon: selectedIcon });
    }
    uiStore.closeOverlay();
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
      create();
    }
  }

  $effect(() => {
    inputEl?.focus();
  });
</script>

<Modal onclose={() => uiStore.closeOverlay()} position="top" label="New Session" class="dialog">
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div onkeydown={handleKeydown}>
    <h2 class="dialog-title">New Session</h2>

    <!-- Mode switcher -->
      <!-- ═══ Session form ═══ -->
      <label class="field-label" for="ns-dir">Directory</label>
      <div class="dir-autocomplete">
        <input
          id="ns-dir"
          bind:this={inputEl}
          class="field-input"
          type="text"
          bind:value={directory}
          placeholder="~/projects/my-app"
          autocomplete="off"
          oninput={onDirInput}
          onkeydown={onDirKeydown}
          onblur={() => {
            suggestions = [];
            selectedSuggestion = -1;
          }}
        />
        {#if suggestions.length > 0}
          <div class="suggestions-dropdown">
            {#each suggestions as s, i}
              <button
                class="suggestion"
                class:selected={i === selectedSuggestion}
                onmousedown={(e) => {
                  e.preventDefault();
                  acceptSuggestion(s);
                }}>{s}</button
              >
            {/each}
          </div>
        {/if}
      </div>

      <label class="field-label" for="ns-cmd">Command</label>
      <Input
        id="ns-cmd"
        mono
        bind:value={command}
        placeholder="Leave empty for default shell"
      />

      <div class="presets">
        {#each presets as preset}
          <button
            class="preset"
            class:active={command === preset.cmd}
            onclick={() => {
              command = preset.cmd;
              selectedIcon = preset.icon;
            }}
          >
            {preset.label}
          </button>
        {/each}
      </div>

      <span class="field-label">Icon</span>
      <div class="icon-picker-container">
        <div class="picker-tabs">
          <button
            class="picker-tab"
            class:active={iconTab === 'emoji'}
            onclick={() => (iconTab = 'emoji')}>Emoji</button
          >
          <button
            class="picker-tab"
            class:active={iconTab === 'icon'}
            onclick={() => (iconTab = 'icon')}>Icon</button
          >
          {#if selectedIcon}
            <button class="picker-clear" onclick={() => (selectedIcon = '')}>Clear</button>
          {/if}
        </div>
        <div class="icon-grid">
          {#if iconTab === 'emoji'}
            {#each emojis as emoji}
              <button
                class="icon-btn"
                class:active={selectedIcon === emoji}
                onclick={() => (selectedIcon = emoji)}>{emoji}</button
              >
            {/each}
          {:else}
            {#each lucideIconNames as name}
              <button
                class="icon-btn"
                class:active={selectedIcon === `lucide:${name}`}
                onclick={() => (selectedIcon = `lucide:${name}`)}
              >
                <SessionIcon icon={`lucide:${name}`} size={16} />
              </button>
            {/each}
          {/if}
        </div>
      </div>

      <div class="options">
        <div class="option">
          <span class="field-label">Space</span>
          <Select
            value={selectedSpace}
            options={spaceStore.spaces.map((s) => ({ value: s.id, label: s.name }))}
            dropup
            onchange={(v) => {
              selectedSpace = v;
              const space = spaceStore.spaces.find((s) => s.id === v);
              const spaceDir = space?.directory;
              if (spaceDir) {
                directory = spaceDir;
              } else {
                const settingsDir = settingsStore.settings.defaultDirectory;
                directory = settingsDir && settingsDir !== '~' ? settingsDir : '~';
              }
            }}
          />
        </div>

        {#if profileStore.profiles.length > 1}
          <div class="option">
            <span class="field-label">Profile</span>
            <Select
              value={selectedProfile ?? ''}
              options={[
                { value: '', label: 'Inherit from Space' },
                ...profileStore.profiles.map((p) => ({ value: p.id, label: p.name })),
              ]}
              dropup
              onchange={(v) => { selectedProfile = v || undefined; }}
            />
          </div>
        {/if}

        <label class="toggle-label">
          <button
            class="toggle"
            class:active={pinned}
            onclick={() => (pinned = !pinned)}
            type="button"
          >
            <span class="toggle-thumb"></span>
          </button>
          Pin to sidebar
        </label>
      </div>

      <div class="dialog-actions">
        <Button variant="secondary" onclick={() => uiStore.closeOverlay()}>Cancel</Button>
        <Button variant="primary" onclick={create}>Create</Button>
      </div>
  </div>
</Modal>

<style>
  :global(.dialog) {
    width: 460px;
    max-height: 80vh;
    overflow-y: auto;
    align-self: flex-start;
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
    margin-top: 12px;
  }

  .field-input {
    width: 100%;
    padding: 8px 10px;
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-md);
    background: var(--weplex-bg);
    color: var(--weplex-text);
    font-size: var(--weplex-text-base);
    font-family: var(--weplex-font-mono);
    outline: none;
    transition: border-color var(--weplex-duration-fast) var(--weplex-easing);
  }

  .field-input:focus {
    border-color: var(--weplex-accent);
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


  .presets {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 10px;
  }

  .preset {
    padding: 4px 10px;
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-full);
    background: transparent;
    color: var(--weplex-text-secondary);
    font-size: var(--weplex-text-xs);
    font-family: var(--weplex-font-mono);
    transition: all var(--weplex-duration-fast) var(--weplex-easing);
  }

  .preset:hover {
    border-color: var(--weplex-accent);
    color: var(--weplex-accent);
  }

  .preset.active {
    border-color: var(--weplex-accent);
    background: color-mix(in srgb, var(--weplex-accent) 10%, transparent);
    color: var(--weplex-accent);
  }

  .icon-picker-container {
    margin-top: 4px;
  }

  .picker-tabs {
    display: flex;
    align-items: center;
    gap: 2px;
    margin-bottom: 8px;
  }

  .picker-tab {
    padding: 4px 12px;
    border: none;
    border-radius: var(--weplex-radius-full);
    background: transparent;
    color: var(--weplex-text-secondary);
    font-size: var(--weplex-text-xs);
    cursor: pointer;
    transition: all var(--weplex-duration-fast) var(--weplex-easing);
  }

  .picker-tab:hover {
    color: var(--weplex-text);
  }

  .picker-tab.active {
    background: var(--weplex-accent);
    color: white;
  }

  .picker-clear {
    margin-left: auto;
    padding: 4px 8px;
    border: none;
    background: transparent;
    color: var(--weplex-text-muted);
    font-size: var(--weplex-text-xs);
    cursor: pointer;
  }

  .picker-clear:hover {
    color: var(--weplex-text);
  }

  .icon-grid {
    display: flex;
    flex-wrap: wrap;
    gap: 2px;
    max-height: 114px;
    overflow-y: auto;
  }

  .icon-btn {
    width: 36px;
    height: 36px;
    border: 1px solid transparent;
    border-radius: var(--weplex-radius-md);
    background: transparent;
    font-size: 18px;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    color: var(--weplex-text-secondary);
    transition: all var(--weplex-duration-fast) var(--weplex-easing);
  }

  .icon-btn:hover {
    background: var(--weplex-surface-hover);
  }

  .icon-btn.active {
    border-color: var(--weplex-accent);
    background: color-mix(in srgb, var(--weplex-accent) 10%, transparent);
    color: var(--weplex-accent);
  }

  .options {
    display: flex;
    align-items: flex-end;
    gap: 16px;
    margin-top: 16px;
  }

  .toggle-label {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: var(--weplex-text-sm);
    color: var(--weplex-text-secondary);
    cursor: pointer;
    min-height: 31px;
  }

  .toggle {
    position: relative;
    width: 32px;
    height: 18px;
    border-radius: 9px;
    border: 1px solid var(--weplex-border);
    background: var(--weplex-bg);
    cursor: pointer;
    padding: 0;
    flex-shrink: 0;
    transition: all var(--weplex-duration-fast) var(--weplex-easing);
  }

  .toggle.active {
    background: var(--weplex-accent);
    border-color: var(--weplex-accent);
  }

  .toggle-thumb {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--weplex-text-muted);
    transition: all var(--weplex-duration-fast) var(--weplex-easing);
  }

  .toggle.active .toggle-thumb {
    left: 16px;
    background: white;
  }

  .dialog-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 20px;
  }

</style>

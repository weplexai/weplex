<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { sessionStore } from '../../stores/sessionStore';
  import { spaceStore } from '../../stores/spaceStore';
  import { profileStore } from '../../stores/profileStore';
  import { settingsStore } from '../../stores/settingsStore.svelte';
  import { uiStore } from '../../stores/uiStore';
  import { pipelineRunStore } from '../../stores/pipelineRunStore.svelte';
  import type { PipelineConfig } from './types';
  import SessionIcon from '../SessionIcon.svelte';
  import { Workflow } from 'lucide-svelte';

  // Resolve default directory: Space.directory > AppSettings.defaultDirectory > '~'
  function getDefaultDirectory(): string {
    const space = spaceStore.spaces.find((s) => s.id === spaceStore.activeSpaceId);
    if (space?.directory) return space.directory;
    const settingsDir = settingsStore.settings.defaultDirectory;
    if (settingsDir && settingsDir !== '~') return settingsDir;
    return '~';
  }

  // Mode: session (terminal/agent) or pipeline
  let mode = $state<'session' | 'pipeline'>('session');

  // Pipeline state
  let pipelines = $state<PipelineConfig[]>([]);
  let selectedPipeline = $state<PipelineConfig | null>(null);
  let taskDescription = $state('');
  let launching = $state(false);
  let launchError = $state<string | null>(null);

  onMount(async () => {
    try {
      pipelines = await invoke<PipelineConfig[]>('list_pipelines');
    } catch {}
  });

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

  async function launchPipeline() {
    if (!selectedPipeline || !taskDescription.trim()) {
      launchError = 'Select a pipeline and describe the task';
      return;
    }
    launching = true;
    launchError = null;
    try {
      const cwd = directory.length > 1 ? directory.replace(/\/+$/, '') : directory;
      // Get profile env vars — explicit selection > space profile > default
      // Must include CLAUDE_CONFIG_DIR from profile.configDir (same as TerminalView)
      const profileId =
        selectedProfile ??
        spaceStore.spaces.find((s) => s.id === selectedSpace)?.profileId ??
        'default';
      const profile = profileStore.getById(profileId) || profileStore.defaultProfile;
      const envVars: Record<string, string> = {};
      if (profile && !profile.isDefault) {
        Object.assign(envVars, profile.envVars);
        if (profile.configDir) {
          envVars['CLAUDE_CONFIG_DIR'] = profile.configDir;
        }
      }
      await pipelineRunStore.startRun(
        selectedPipeline.file_path,
        taskDescription.trim(),
        cwd,
        envVars,
      );
      uiStore.closeOverlay();
    } catch (e: unknown) {
      launchError = e instanceof Error ? e.message : String(e);
    } finally {
      launching = false;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
      if (mode === 'pipeline') launchPipeline();
      else create();
    }
    if (e.key === 'Escape') {
      uiStore.closeOverlay();
    }
  }

  $effect(() => {
    inputEl?.focus();
  });
</script>

<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions a11y_no_noninteractive_element_interactions -->
<div class="overlay-backdrop" role="presentation" onclick={() => uiStore.closeOverlay()}>
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions a11y_no_noninteractive_element_interactions a11y_interactive_supports_focus -->
  <div
    class="dialog"
    role="dialog"
    tabindex="-1"
    aria-label="New Session"
    onclick={(e) => e.stopPropagation()}
    onkeydown={handleKeydown}
  >
    <h2 class="dialog-title">New Session</h2>

    <!-- Mode switcher -->
    <div class="mode-switcher">
      <button class="mode-btn" class:active={mode === 'session'} onclick={() => (mode = 'session')}
        >Session</button
      >
      <button
        class="mode-btn"
        class:active={mode === 'pipeline'}
        onclick={() => (mode = 'pipeline')}
      >
        <Workflow size={12} /> Pipeline
      </button>
    </div>

    {#if mode === 'session'}
      <!-- ═══ Session form (existing) ═══ -->
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
      <input
        id="ns-cmd"
        class="field-input"
        type="text"
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
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="custom-select"
            onfocusout={(e) => {
              if (!(e.currentTarget as HTMLElement).contains(e.relatedTarget as Node))
                showSpaceDropdown = false;
            }}
          >
            <button class="select-trigger" onclick={() => (showSpaceDropdown = !showSpaceDropdown)}>
              <span>{currentSpaceName}</span>
              <span class="select-chevron">{showSpaceDropdown ? '\u25B4' : '\u25BE'}</span>
            </button>
            {#if showSpaceDropdown}
              <div class="select-dropdown">
                {#each spaceStore.spaces as space (space.id)}
                  <button
                    class="select-option"
                    class:active={selectedSpace === space.id}
                    onclick={() => {
                      selectedSpace = space.id;
                      showSpaceDropdown = false;
                      // Update directory to match new space's default
                      const spaceDir = space.directory;
                      if (spaceDir) {
                        directory = spaceDir;
                      } else {
                        const settingsDir = settingsStore.settings.defaultDirectory;
                        directory = settingsDir && settingsDir !== '~' ? settingsDir : '~';
                      }
                    }}>{space.name}</button
                  >
                {/each}
              </div>
            {/if}
          </div>
        </div>

        {#if profileStore.profiles.length > 1}
          <div class="option">
            <span class="field-label">Profile</span>
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div
              class="custom-select"
              onfocusout={(e) => {
                if (!(e.currentTarget as HTMLElement).contains(e.relatedTarget as Node))
                  showProfileDropdown = false;
              }}
            >
              <button
                class="select-trigger"
                onclick={() => (showProfileDropdown = !showProfileDropdown)}
              >
                <span>{currentProfileName}</span>
                <span class="select-chevron">{showProfileDropdown ? '\u25B4' : '\u25BE'}</span>
              </button>
              {#if showProfileDropdown}
                <div class="select-dropdown">
                  <button
                    class="select-option"
                    class:active={selectedProfile === undefined}
                    onclick={() => {
                      selectedProfile = undefined;
                      showProfileDropdown = false;
                    }}>Inherit from Space</button
                  >
                  {#each profileStore.profiles as profile (profile.id)}
                    <button
                      class="select-option"
                      class:active={selectedProfile === profile.id}
                      onclick={() => {
                        selectedProfile = profile.id;
                        showProfileDropdown = false;
                      }}>{profile.name}</button
                    >
                  {/each}
                </div>
              {/if}
            </div>
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
        <button class="btn-cancel" onclick={() => uiStore.closeOverlay()}>Cancel</button>
        <button class="btn-create" onclick={create}>Create</button>
      </div>
    {:else}
      <!-- ═══ Pipeline form ═══ -->
      <label class="field-label" for="ns-pipe-dir">Directory</label>
      <div class="dir-autocomplete">
        <input
          id="ns-pipe-dir"
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

      <span class="field-label">Pipeline</span>
      <div class="pipeline-picker">
        {#each pipelines as p}
          <button
            class="pipeline-card"
            class:active={selectedPipeline?.name === p.name}
            onclick={() => (selectedPipeline = p)}
          >
            <span class="pipeline-card-icon"><Workflow size={13} /></span>
            <span class="pipeline-card-info">
              <span class="pipeline-card-name">{p.name}</span>
              <span class="pipeline-card-desc">{p.stages.length} stages</span>
            </span>
          </button>
        {/each}
        {#if pipelines.length === 0}
          <div class="pipeline-empty">
            No pipelines configured. Create one in Agents & Pipelines (⌘⇧A).
          </div>
        {/if}
      </div>

      <label class="field-label" for="ns-task">Task</label>
      <textarea
        id="ns-task"
        class="field-input task-textarea"
        bind:value={taskDescription}
        placeholder="Describe what needs to be done..."
        rows={3}
      ></textarea>

      {#if profileStore.profiles.length > 1}
        <div class="options">
          <div class="option">
            <span class="field-label">Profile</span>
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div
              class="custom-select"
              onfocusout={(e) => {
                if (!(e.currentTarget as HTMLElement).contains(e.relatedTarget as Node))
                  showProfileDropdown = false;
              }}
            >
              <button
                class="select-trigger"
                onclick={() => (showProfileDropdown = !showProfileDropdown)}
              >
                <span>{currentProfileName}</span>
                <span class="select-chevron">{showProfileDropdown ? '\u25B4' : '\u25BE'}</span>
              </button>
              {#if showProfileDropdown}
                <div class="select-dropdown">
                  <button
                    class="select-option"
                    class:active={selectedProfile === undefined}
                    onclick={() => {
                      selectedProfile = undefined;
                      showProfileDropdown = false;
                    }}>Inherit from Space</button
                  >
                  {#each profileStore.profiles as profile (profile.id)}
                    <button
                      class="select-option"
                      class:active={selectedProfile === profile.id}
                      onclick={() => {
                        selectedProfile = profile.id;
                        showProfileDropdown = false;
                      }}>{profile.name}</button
                    >
                  {/each}
                </div>
              {/if}
            </div>
          </div>
        </div>
      {/if}

      {#if launchError}
        <div class="launch-error">{launchError}</div>
      {/if}

      <div class="dialog-actions">
        <button class="btn-cancel" onclick={() => uiStore.closeOverlay()}>Cancel</button>
        <button
          class="btn-create btn-launch"
          onclick={launchPipeline}
          disabled={launching || !selectedPipeline || !taskDescription.trim()}
        >
          <Workflow size={13} />
          {launching ? 'Starting...' : 'Launch'}
        </button>
      </div>
    {/if}
  </div>
</div>

<style>
  .overlay-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.4);
    display: flex;
    justify-content: center;
    padding-top: 12vh;
    z-index: 100;
  }

  .dialog {
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

  .custom-select {
    position: relative;
  }

  .select-trigger {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-md);
    background: var(--weplex-bg);
    color: var(--weplex-text);
    font-size: var(--weplex-text-sm);
    font-family: var(--weplex-font-mono);
    cursor: pointer;
    transition: border-color var(--weplex-duration-fast) var(--weplex-easing);
    min-width: 100px;
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
    padding: 5px 8px;
    border: none;
    border-radius: var(--weplex-radius-sm);
    background: transparent;
    color: var(--weplex-text);
    font-size: var(--weplex-text-sm);
    font-family: var(--weplex-font-mono);
    text-align: left;
    cursor: pointer;
  }

  .select-option:hover {
    background: var(--weplex-surface-hover);
  }

  .select-option.active {
    color: var(--weplex-accent);
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
    max-height: 200px;
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

  .btn-launch {
    display: flex;
    align-items: center;
    gap: 5px;
  }

  /* ── Mode switcher ─────────────────────────────── */
  .mode-switcher {
    display: flex;
    gap: 2px;
    margin-bottom: 14px;
    background: var(--weplex-bg);
    border-radius: var(--weplex-radius-md);
    padding: 2px;
  }

  .mode-btn {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    padding: 6px 12px;
    border: none;
    border-radius: var(--weplex-radius-sm);
    background: transparent;
    color: var(--weplex-text-muted);
    font-size: var(--weplex-text-sm);
    font-weight: 500;
    cursor: pointer;
    transition: all var(--weplex-duration-fast) var(--weplex-easing);
  }

  .mode-btn:hover {
    color: var(--weplex-text);
  }

  .mode-btn.active {
    background: var(--weplex-surface);
    color: var(--weplex-text);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.15);
  }

  /* ── Pipeline picker ───────────────────────────── */
  .pipeline-picker {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-top: 4px;
  }

  .pipeline-card {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 12px;
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-md);
    background: transparent;
    cursor: pointer;
    text-align: left;
    transition: all var(--weplex-duration-fast) var(--weplex-easing);
  }

  .pipeline-card:hover {
    border-color: var(--weplex-accent);
    background: color-mix(in srgb, var(--weplex-accent) 4%, transparent);
  }

  .pipeline-card.active {
    border-color: var(--weplex-accent);
    background: color-mix(in srgb, var(--weplex-accent) 10%, transparent);
  }

  .pipeline-card-icon {
    color: var(--weplex-accent);
    display: flex;
    flex-shrink: 0;
  }

  .pipeline-card-info {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .pipeline-card-name {
    font-size: var(--weplex-text-sm);
    font-weight: 600;
    color: var(--weplex-text);
    font-family: var(--weplex-font-mono);
  }

  .pipeline-card.active .pipeline-card-name {
    color: var(--weplex-accent);
  }

  .pipeline-card-desc {
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
  }

  .pipeline-empty {
    padding: 12px;
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
    text-align: center;
  }

  .task-textarea {
    resize: vertical;
    min-height: 60px;
    line-height: 1.5;
  }

  .launch-error {
    margin-top: 8px;
    padding: 6px 10px;
    border-radius: var(--weplex-radius-sm);
    background: color-mix(in srgb, var(--weplex-error) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--weplex-error) 20%, transparent);
    color: var(--weplex-error);
    font-size: var(--weplex-text-xs);
  }
</style>

<script lang="ts">
  import { sessionStore } from '../../stores/sessionStore';
  import { uiStore } from '../../stores/uiStore';
  import { shortcuts, getShortcutHint } from '../../utils/shortcuts';

  let { mode = 'full' }: { mode?: 'full' | 'sessions' } = $props();

  let query = $state('');
  let selectedIndex = $state(0);
  let inputEl = $state<HTMLInputElement>();

  interface PaletteItem {
    id: string;
    label: string;
    hint?: string;
    category: string;
    action: () => void;
  }

  let items = $derived.by(() => {
    const result: PaletteItem[] = [];
    const q = query.toLowerCase();

    // Sessions
    for (const s of sessionStore.sessions) {
      const label = s.name;
      if (q && !label.toLowerCase().includes(q)) continue;
      result.push({
        id: `session-${s.id}`,
        label: `Switch to ${label}`,
        hint: s.type,
        category: 'Sessions',
        action: () => {
          sessionStore.activate(s.id);
          uiStore.closeOverlay();
        },
      });
    }

    if (mode === 'full') {
      // Actions
      const actions: PaletteItem[] = [
        {
          id: 'new-session',
          label: 'New session',
          hint: getShortcutHint('N'),
          category: 'Actions',
          action: () => {
            uiStore.closeOverlay();
            uiStore.openOverlay('new-session');
          },
        },
        {
          id: 'kill-session',
          label: 'Kill current session',
          hint: getShortcutHint('W'),
          category: 'Actions',
          action: () => {
            const id = sessionStore.activeSessionId;
            if (id !== null) sessionStore.kill(id);
            uiStore.closeOverlay();
          },
        },
        {
          id: 'toggle-sidebar',
          label: 'Toggle sidebar',
          hint: getShortcutHint('B'),
          category: 'Actions',
          action: () => {
            uiStore.toggleSidebar();
            uiStore.closeOverlay();
          },
        },
        {
          id: 'toggle-detail',
          label: 'Toggle detail panel',
          hint: getShortcutHint('I'),
          category: 'Actions',
          action: () => {
            uiStore.toggleDetailPanel();
            uiStore.closeOverlay();
          },
        },
        {
          id: 'agents-pipelines',
          label: 'Agents & Pipelines',
          hint: '⇧⌘A',
          category: 'Actions',
          action: () => {
            uiStore.closeOverlay();
            uiStore.openOverlay('agents');
          },
        },
      ];

      for (const a of actions) {
        if (q && !a.label.toLowerCase().includes(q)) continue;
        result.push(a);
      }

      // Settings
      if (!q || 'settings'.includes(q) || 'theme'.includes(q) || 'font'.includes(q)) {
        result.push({
          id: 'settings',
          label: 'Settings',
          hint: getShortcutHint(','),
          category: 'Settings',
          action: () => {
            uiStore.closeOverlay();
            uiStore.openOverlay('settings');
          },
        });
      }
    }

    return result;
  });

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      selectedIndex = Math.min(selectedIndex + 1, items.length - 1);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      selectedIndex = Math.max(selectedIndex - 1, 0);
    } else if (e.key === 'Enter' && items[selectedIndex]) {
      e.preventDefault();
      items[selectedIndex].action();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      uiStore.closeOverlay();
    }
  }

  $effect(() => {
    // Reset selection when query changes
    query;
    selectedIndex = 0;
  });

  $effect(() => {
    inputEl?.focus();
  });
</script>

<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions a11y_no_noninteractive_element_interactions -->
<div class="overlay-backdrop" role="presentation" onclick={() => uiStore.closeOverlay()}>
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions a11y_no_noninteractive_element_interactions a11y_interactive_supports_focus -->
  <div
    class="palette"
    role="dialog"
    tabindex="-1"
    aria-label="Command Palette"
    onclick={(e) => e.stopPropagation()}
  >
    <input
      bind:this={inputEl}
      class="palette-input"
      type="text"
      placeholder={mode === 'sessions' ? 'Search sessions...' : 'Type a command or session name...'}
      bind:value={query}
      onkeydown={handleKeydown}
    />

    <div class="palette-results">
      {#each items as item, i (item.id)}
        {#if i === 0 || items[i - 1]?.category !== item.category}
          <div class="result-category">{item.category}</div>
        {/if}
        <button
          class="result-item"
          class:selected={i === selectedIndex}
          onclick={item.action}
          onmouseenter={() => (selectedIndex = i)}
        >
          <span class="result-label">{item.label}</span>
          {#if item.hint}
            <span class="result-hint">{item.hint}</span>
          {/if}
        </button>
      {/each}

      {#if items.length === 0}
        <div class="no-results">No results found</div>
      {/if}
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

  .palette {
    width: 520px;
    max-height: 400px;
    background: var(--weplex-surface);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-xl);
    box-shadow: var(--weplex-shadow-overlay);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .palette-input {
    width: 100%;
    padding: 14px 16px;
    border: none;
    border-bottom: 1px solid var(--weplex-border);
    background: transparent;
    color: var(--weplex-text);
    font-size: var(--weplex-text-md);
    outline: none;
  }

  .palette-input::placeholder {
    color: var(--weplex-text-muted);
  }

  .palette-results {
    overflow-y: auto;
    padding: 4px;
  }

  .result-category {
    padding: 8px 12px 4px;
    font-size: 10px;
    font-weight: 600;
    color: var(--weplex-text-muted);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .result-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    padding: 8px 12px;
    border: none;
    border-radius: var(--weplex-radius-md);
    background: transparent;
    color: var(--weplex-text);
    font-size: var(--weplex-text-base);
    text-align: left;
    cursor: pointer;
    transition: background var(--weplex-duration-fast) var(--weplex-easing);
  }

  .result-item:hover,
  .result-item.selected {
    background: var(--weplex-surface-hover);
  }

  .result-hint {
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
    font-family: var(--weplex-font-mono);
  }

  .no-results {
    padding: 16px;
    text-align: center;
    font-size: var(--weplex-text-sm);
    color: var(--weplex-text-muted);
  }
</style>

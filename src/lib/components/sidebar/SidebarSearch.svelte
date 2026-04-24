<script lang="ts">
  import { sessionStore } from '../../stores/sessionStore';
  import { uiStore } from '../../stores/uiStore';
  import { getShortcutHint } from '../../utils/shortcuts';

  let query = $state('');
  let focused = $state(false);
  let selectedIndex = $state(0);
  let inputEl = $state<HTMLInputElement>();
  let wrapperEl = $state<HTMLDivElement>();

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
      if (q && !s.name.toLowerCase().includes(q)) continue;
      result.push({
        id: `session-${s.id}`,
        label: s.name,
        hint: s.type,
        category: 'Sessions',
        action: () => {
          sessionStore.activate(s.id);
          blur();
        },
      });
    }

    // Actions
    const actions: PaletteItem[] = [
      {
        id: 'new-session',
        label: 'New session',
        hint: getShortcutHint('N'),
        category: 'Actions',
        action: () => {
          blur();
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
          blur();
        },
      },
      {
        id: 'toggle-sidebar',
        label: 'Toggle sidebar',
        hint: getShortcutHint('B'),
        category: 'Actions',
        action: () => {
          uiStore.toggleSidebar();
          blur();
        },
      },
      {
        id: 'toggle-detail',
        label: 'Toggle detail panel',
        hint: getShortcutHint('I'),
        category: 'Actions',
        action: () => {
          uiStore.toggleDetailPanel();
          blur();
        },
      },
      {
        id: 'settings',
        label: 'Settings',
        hint: getShortcutHint(','),
        category: 'Settings',
        action: () => {
          blur();
          uiStore.enterHubMode('settings');
        },
      },
    ];

    if (import.meta.env.DEV) {
      actions.push({
        id: 'uikit',
        label: 'UI Kit',
        hint: '',
        category: 'Dev',
        action: () => {
          blur();
          uiStore.openOverlay('uikit');
        },
      });
    }

    for (const a of actions) {
      if (q && !a.label.toLowerCase().includes(q)) continue;
      result.push(a);
    }

    return result;
  });

  function blur() {
    query = '';
    focused = false;
    inputEl?.blur();
  }

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
      blur();
    }
  }

  function handleClickOutside(e: MouseEvent) {
    if (focused && wrapperEl && !wrapperEl.contains(e.target as Node)) {
      blur();
    }
  }

  $effect(() => {
    query;
    selectedIndex = 0;
  });

  // Listen for Cmd+K to focus
  $effect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault();
        inputEl?.focus();
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  });

  // Click outside
  $effect(() => {
    if (focused) {
      window.addEventListener('mousedown', handleClickOutside);
      return () => window.removeEventListener('mousedown', handleClickOutside);
    }
  });

  // Focus from overlay trigger
  $effect(() => {
    if (uiStore.activeOverlay === 'command-palette') {
      uiStore.closeOverlay();
      inputEl?.focus();
    }
  });
</script>

<div class="search-wrapper" bind:this={wrapperEl}>
  <input
    bind:this={inputEl}
    class="search-input"
    type="text"
    placeholder="Search or command..."
    bind:value={query}
    onfocus={() => (focused = true)}
    onkeydown={handleKeydown}
  />
  {#if !focused}
    <span class="search-hint">{getShortcutHint('K')}</span>
  {/if}

  {#if focused}
    <div class="search-results">
      {#each items as item, i (item.id)}
        {#if i === 0 || items[i - 1]?.category !== item.category}
          <div class="result-category">{item.category}</div>
        {/if}
        <button
          class="result-item"
          class:selected={i === selectedIndex}
          onmousedown={(e) => {
            e.preventDefault();
            item.action();
          }}
          onmouseenter={() => (selectedIndex = i)}
        >
          <span class="result-label">{item.label}</span>
          {#if item.hint}
            <span class="result-hint">{item.hint}</span>
          {/if}
        </button>
      {/each}

      {#if items.length === 0}
        <div class="no-results">No results</div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .search-wrapper {
    position: relative;
    padding: 0 8px 4px;
    flex-shrink: 0;
  }

  .search-input {
    width: 100%;
    padding: 7px 10px;
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-md);
    background: var(--weplex-surface);
    color: var(--weplex-text);
    font-size: var(--weplex-text-sm);
    outline: none;
    transition: border-color var(--weplex-duration-fast) var(--weplex-easing);
  }

  .search-input:focus {
    border-color: var(--weplex-accent);
  }

  .search-input::placeholder {
    color: var(--weplex-text-muted);
  }

  .search-hint {
    position: absolute;
    right: 16px;
    /* Wrapper has asymmetric padding (0 top, 4 bottom), so the input's
       vertical center sits 2px above the wrapper's geometric center.
       Offset the hint accordingly so it aligns with the placeholder. */
    top: calc(50% - 2px);
    transform: translateY(-50%);
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
    pointer-events: none;
    font-family: var(--weplex-font-mono);
  }

  .search-results {
    position: absolute;
    left: 8px;
    right: 8px;
    top: 100%;
    margin-top: 4px;
    max-height: calc(100vh - 120px);
    overflow-y: auto;
    background: var(--weplex-surface);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-lg);
    box-shadow: var(--weplex-shadow-md);
    padding: 4px;
    z-index: 50;
  }

  .result-category {
    padding: 6px 10px 3px;
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
    padding: 6px 10px;
    border: none;
    border-radius: var(--weplex-radius-sm);
    background: transparent;
    color: var(--weplex-text);
    font-size: var(--weplex-text-sm);
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
    padding: 12px;
    text-align: center;
    font-size: var(--weplex-text-sm);
    color: var(--weplex-text-muted);
  }
</style>

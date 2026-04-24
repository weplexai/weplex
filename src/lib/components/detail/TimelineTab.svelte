<script lang="ts">
  import type { NoteEntry } from '../../types';
  import { invoke } from '@tauri-apps/api/core';
  import { timeAgo } from '../../utils/time';

  let { sessionId }: { sessionId: number } = $props();

  let notes = $state<NoteEntry[]>([]);
  let query = $state('');
  let pollTimer: ReturnType<typeof setInterval> | null = null;

  async function fetchNotes() {
    try {
      const data = await invoke<{ notes?: NoteEntry[] } | null>('get_session_summary', {
        sessionId: String(sessionId),
      });
      notes = data?.notes ?? [];
    } catch {
      notes = [];
    }
  }

  $effect(() => {
    const _sid = sessionId; // track
    notes = [];
    fetchNotes();
    if (pollTimer) clearInterval(pollTimer);
    pollTimer = setInterval(fetchNotes, 10_000);
    return () => {
      if (pollTimer) {
        clearInterval(pollTimer);
        pollTimer = null;
      }
    };
  });

  function matches(note: NoteEntry, q: string): boolean {
    if (!q) return true;
    const needle = q.toLowerCase();
    if (note.text.toLowerCase().includes(needle)) return true;
    if (note.filesChanged?.some((f) => f.toLowerCase().includes(needle))) return true;
    if (note.decisions?.some((d) => d.toLowerCase().includes(needle))) return true;
    return false;
  }

  function absoluteTime(ms: number): string {
    const d = new Date(ms);
    // Local, short: 14:32  · Apr 23
    const time = d.toLocaleTimeString(undefined, {
      hour: '2-digit',
      minute: '2-digit',
      hour12: false,
    });
    const date = d.toLocaleDateString(undefined, {
      month: 'short',
      day: 'numeric',
    });
    return `${time} · ${date}`;
  }

  let filtered = $derived(notes.filter((n) => matches(n, query)).slice().reverse());
</script>

<div class="timeline">
  <div class="timeline-head">
    <input
      class="search"
      type="search"
      placeholder="Search activity…"
      bind:value={query}
      aria-label="Search this session's activity"
    />
    <span class="count">
      {#if query}
        {filtered.length} / {notes.length}
      {:else}
        {notes.length} {notes.length === 1 ? 'entry' : 'entries'}
      {/if}
    </span>
  </div>

  {#if notes.length === 0}
    <div class="empty">
      <p class="empty-title">No activity yet.</p>
      <p class="empty-hint">
        Ask your agent to call <code>weplex_log_activity</code> with a short summary
        of what it did. Entries are private to this session on this machine.
      </p>
    </div>
  {:else if filtered.length === 0}
    <div class="empty">
      <p class="empty-title">Nothing matches "{query}".</p>
    </div>
  {:else}
    <ol class="entries">
      {#each filtered as note, i (note.at + '-' + i)}
        <li class="entry">
          <div class="entry-row">
            <time class="entry-time" datetime={new Date(note.at * 1000).toISOString()}>
              {absoluteTime(note.at * 1000)}
            </time>
            <span class="entry-relative">{timeAgo(note.at * 1000)}</span>
          </div>
          <p class="entry-text">{note.text}</p>
          {#if note.filesChanged && note.filesChanged.length > 0}
            <div class="entry-chips">
              {#each note.filesChanged as f}
                <span class="chip chip-file" title={f}>{f.split('/').pop() || f}</span>
              {/each}
            </div>
          {/if}
          {#if note.decisions && note.decisions.length > 0}
            <div class="entry-chips">
              {#each note.decisions as d}
                <span class="chip chip-decision">{d}</span>
              {/each}
            </div>
          {/if}
        </li>
      {/each}
    </ol>
  {/if}
</div>

<style>
  .timeline {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 12px 4px 24px;
  }

  .timeline-head {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .search {
    flex: 1;
    background: var(--weplex-panel);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-sm);
    padding: 6px 10px;
    font-size: var(--weplex-text-sm);
    color: var(--weplex-text);
    font-family: inherit;
    min-width: 0;
  }

  .search::placeholder {
    color: var(--weplex-text-muted);
  }

  .search:focus {
    outline: none;
    border-color: var(--weplex-accent);
  }

  .count {
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
    white-space: nowrap;
  }

  .empty {
    padding: 24px 8px;
    text-align: center;
  }

  .empty-title {
    font-size: var(--weplex-text-sm);
    color: var(--weplex-text);
    margin: 0 0 4px;
  }

  .empty-hint {
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
    margin: 0;
    line-height: 1.5;
  }

  .empty-hint code {
    font-family: var(--weplex-font-mono);
    background: var(--weplex-panel);
    padding: 1px 4px;
    border-radius: 3px;
  }

  .entries {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .entry {
    border-left: 2px solid var(--weplex-border);
    padding: 0 0 0 10px;
  }

  .entry-row {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 8px;
    margin-bottom: 4px;
  }

  .entry-time {
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
    font-family: var(--weplex-font-mono);
  }

  .entry-relative {
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
    opacity: 0.7;
  }

  .entry-text {
    font-size: var(--weplex-text-sm);
    color: var(--weplex-text);
    margin: 0 0 6px;
    line-height: 1.4;
  }

  .entry-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-top: 4px;
  }

  .chip {
    font-size: 10px;
    padding: 1px 6px;
    border-radius: 3px;
    font-family: var(--weplex-font-mono);
    max-width: 200px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .chip-file {
    background: var(--weplex-panel);
    color: var(--weplex-text-muted);
    border: 1px solid var(--weplex-border);
  }

  .chip-decision {
    background: color-mix(in srgb, var(--weplex-accent) 10%, transparent);
    color: var(--weplex-text);
    border: 1px solid color-mix(in srgb, var(--weplex-accent) 30%, transparent);
  }
</style>

<script lang="ts">
  import type { NoteEntry } from '../../types';
  import { invoke } from '@tauri-apps/api/core';
  import { timeAgo } from '../../utils/time';


  let { sessionId }: { sessionId: number } = $props();

  let notes = $state<NoteEntry[]>([]);
  let pollTimer: ReturnType<typeof setInterval> | null = null;

  async function fetchNotes() {
    try {
      const data = await invoke<{ notes?: NoteEntry[] } | null>('get_session_summary', {
        sessionId: String(sessionId),
      });
      if (data?.notes) {
        notes = data.notes;
      }
    } catch {
      // Summary file may not exist yet
    }
  }

  // Re-fetch when sessionId changes and set up polling
  $effect(() => {
    const _sid = sessionId; // track dependency
    notes = [];
    fetchNotes();

    if (pollTimer) clearInterval(pollTimer);
    pollTimer = setInterval(fetchNotes, 10_000);

    // Cleanup interval when effect re-runs or component unmounts
    return () => {
      if (pollTimer) {
        clearInterval(pollTimer);
        pollTimer = null;
      }
    };
  });
</script>

{#if notes.length > 0}
  <section class="section">
    <h3 class="section-title">ACTIVITY</h3>
    <div class="activity-list">
      {#each notes.toReversed() as note}
        <div class="activity-entry">
          <div class="activity-header">
            <span class="activity-text">{note.text}</span>
            <span class="activity-time">{timeAgo(note.at * 1000)}</span>
          </div>
          {#if note.filesChanged && note.filesChanged.length > 0}
            <div class="activity-files">
              {note.filesChanged.map((f) => f.split('/').pop() || f).join(', ')}
            </div>
          {/if}
        </div>
      {/each}
    </div>
  </section>
{/if}

<style>
  .section {
    margin-bottom: 16px;
  }

  .section-title {
    font-size: 10px;
    font-weight: 600;
    color: var(--weplex-text-muted);
    letter-spacing: 0.06em;
    margin-bottom: 8px;
  }

  .activity-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .activity-entry {
    padding: 0;
  }

  .activity-header {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 8px;
  }

  .activity-text {
    font-size: var(--weplex-text-sm);
    color: var(--weplex-text);
    font-weight: 500;
    flex: 1;
    min-width: 0;
  }

  .activity-time {
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
    white-space: nowrap;
    flex-shrink: 0;
  }

  .activity-files {
    font-size: var(--weplex-text-xs);
    font-family: var(--weplex-font-mono);
    color: var(--weplex-text-muted);
    margin-top: 2px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>

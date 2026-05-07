<script lang="ts">
  import type { Session, NoteEntry } from '../../types';
  import { invoke } from '@tauri-apps/api/core';
  import { timeAgo } from '../../utils/time';
  import { resolveProfileEnvId } from '../../utils/profile';

  // Svelte action: move the element into <body> on mount, remove on destroy.
  // Prevents position: fixed from being captured by an ancestor containing
  // block (the sidebar's .slider-track has `will-change: transform`, which
  // creates one — so without this portal the preview clips inside the
  // sidebar instead of overlaying the viewport.
  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return {
      destroy() {
        node.remove();
      },
    };
  }

  let {
    session,
    anchorEl,
    onmouseenter,
    onmouseleave,
  }: {
    session: Session;
    anchorEl: HTMLElement;
    onmouseenter?: () => void;
    onmouseleave?: () => void;
  } = $props();

  let notes = $state<NoteEntry[]>([]);
  let loaded = $state(false);
  let pos = $state<{ top: number; left: number; width: number }>({
    top: 0,
    left: 0,
    width: 280,
  });

  async function fetchNotes() {
    try {
      const data = await invoke<{ notes?: NoteEntry[] } | null>('get_session_summary', {
        sessionId: String(session.id),
        profileId: resolveProfileEnvId(session),
      });
      notes = (data?.notes ?? []).slice(-3).reverse();
    } catch {
      notes = [];
    } finally {
      loaded = true;
    }
  }

  function reposition() {
    const rect = anchorEl.getBoundingClientRect();
    const PREFERRED_WIDTH = 280;
    const MARGIN = 8;
    const PAD = 8;
    const viewportWidth = window.innerWidth;
    const viewportHeight = window.innerHeight;

    // Shrink the preview if the window is narrower than 280 + padding.
    const width = Math.min(PREFERRED_WIDTH, viewportWidth - 2 * PAD);

    // First choice: to the right of the anchor.
    const fitsRight = rect.right + MARGIN + width <= viewportWidth - PAD;
    // Second choice: to the left of the anchor.
    const fitsLeft = rect.left - MARGIN - width >= PAD;

    let left: number;
    let top: number;

    if (fitsRight) {
      left = rect.right + MARGIN;
      top = rect.top;
    } else if (fitsLeft) {
      left = rect.left - width - MARGIN;
      top = rect.top;
    } else {
      // Too narrow for either side — drop below the anchor, full width,
      // centred horizontally within the viewport.
      left = Math.max(PAD, Math.round((viewportWidth - width) / 2));
      top = rect.bottom + MARGIN;
    }

    // Final clamp so nothing escapes the viewport regardless of which
    // branch ran above.
    left = Math.max(PAD, Math.min(left, viewportWidth - width - PAD));
    const maxTop = viewportHeight - 120;
    if (top > maxTop) top = Math.max(PAD, maxTop);

    pos = { top, left, width };
  }

  $effect(() => {
    fetchNotes();
    reposition();
    const onResize = () => reposition();
    window.addEventListener('resize', onResize);
    window.addEventListener('scroll', onResize, true);
    return () => {
      window.removeEventListener('resize', onResize);
      window.removeEventListener('scroll', onResize, true);
    };
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  use:portal
  class="preview"
  style:top="{pos.top}px"
  style:left="{pos.left}px"
  style:width="{pos.width}px"
  role="tooltip"
  {onmouseenter}
  {onmouseleave}
>
  <div class="preview-head">
    <span class="preview-title">{session.name}</span>
    {#if session.agentType}
      <span class="preview-agent">{session.agentType}</span>
    {/if}
  </div>

  {#if !loaded}
    <div class="preview-empty">Loading…</div>
  {:else if notes.length === 0}
    <div class="preview-empty">
      No activity yet in this session.
    </div>
  {:else}
    <ul class="preview-list">
      {#each notes as note, i (note.at + '-' + i)}
        <li class="preview-entry">
          <div class="preview-entry-head">
            <span class="preview-entry-time">{timeAgo(note.at * 1000)}</span>
          </div>
          <p class="preview-entry-text">{note.text}</p>
        </li>
      {/each}
    </ul>
    {#if notes.length > 0}
      <div class="preview-foot">Timeline tab for full history</div>
    {/if}
  {/if}
</div>

<style>
  .preview {
    position: fixed;
    z-index: 1000;
    max-width: calc(100vw - 16px);
    background: var(--weplex-bg);
    border: 1px solid var(--weplex-border);
    border-radius: 8px;
    box-shadow:
      0 0 0 1px rgba(0, 0, 0, 0.3),
      0 8px 24px rgba(0, 0, 0, 0.35);
    padding: 10px 12px;
    pointer-events: auto;
    font-family: inherit;
  }

  .preview-head {
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding-bottom: 6px;
    border-bottom: 1px solid var(--weplex-border);
    margin-bottom: 8px;
  }

  .preview-title {
    font-size: var(--weplex-text-sm);
    font-weight: 600;
    color: var(--weplex-text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex: 1;
    min-width: 0;
  }

  .preview-agent {
    font-size: 10px;
    color: var(--weplex-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    font-family: var(--weplex-font-mono);
  }

  .preview-empty {
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
    padding: 4px 0;
  }

  .preview-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .preview-entry {
    padding: 0;
  }

  .preview-entry-head {
    display: flex;
    margin-bottom: 2px;
  }

  .preview-entry-time {
    font-size: 10px;
    color: var(--weplex-text-muted);
    font-family: var(--weplex-font-mono);
  }

  .preview-entry-text {
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text);
    margin: 0;
    line-height: 1.4;
    overflow: hidden;
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
  }

  .preview-foot {
    margin-top: 8px;
    padding-top: 6px;
    border-top: 1px solid var(--weplex-border);
    font-size: 10px;
    color: var(--weplex-text-muted);
    text-align: center;
  }
</style>

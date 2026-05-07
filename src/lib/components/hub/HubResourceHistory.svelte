<script lang="ts">
  import { History, RotateCcw, AlertTriangle } from 'lucide-svelte';
  import { Button } from '../ui';
  import type { LockfileEntry, LockfileHistoryEntry } from '../../types/lockfile';
  import { lockfileStore } from '../../stores/lockfileStore.svelte';

  interface Props {
    profileConfigDir: string;
    resourceId: string;
    current: LockfileEntry | null;
    history: LockfileHistoryEntry[];
  }

  let { profileConfigDir, resourceId, current, history }: Props = $props();

  // Confirm dialog state — reuses the resource detail's overlay surface.
  let confirm = $state<{
    sha256: string;
    label: string;
  } | null>(null);
  let restoring = $state(false);
  let toast = $state<{ type: 'success' | 'error'; text: string } | null>(null);

  let isEmpty = $derived(!current && history.length === 0);

  // Stable label for a history entry: prefer version, fall back to short sha.
  function labelFor(
    entry: { version: string | null; sha256: string },
  ): string {
    if (entry.version && entry.version.trim().length > 0) return entry.version;
    return `sha:${entry.sha256.slice(0, 8)}`;
  }

  /** Relative date — keeps the row compact ("2 days ago"). */
  function relativeDate(iso: string): string {
    const t = Date.parse(iso);
    if (Number.isNaN(t)) return iso;
    const now = Date.now();
    const diffMs = now - t;
    const sec = Math.round(diffMs / 1000);
    if (sec < 5) return 'just now';
    if (sec < 60) return `${sec}s ago`;
    const min = Math.round(sec / 60);
    if (min < 60) return `${min} minute${min === 1 ? '' : 's'} ago`;
    const hr = Math.round(min / 60);
    if (hr < 24) return `${hr} hour${hr === 1 ? '' : 's'} ago`;
    const day = Math.round(hr / 24);
    if (day < 30) return `${day} day${day === 1 ? '' : 's'} ago`;
    const mon = Math.round(day / 30);
    if (mon < 12) return `${mon} month${mon === 1 ? '' : 's'} ago`;
    const yr = Math.round(mon / 12);
    return `${yr} year${yr === 1 ? '' : 's'} ago`;
  }

  function showToast(type: 'success' | 'error', text: string) {
    toast = { type, text };
    setTimeout(() => {
      toast = null;
    }, 3000);
  }

  function askRestore(entry: LockfileHistoryEntry) {
    confirm = {
      sha256: entry.sha256,
      label: `${labelFor(entry)} (${relativeDate(entry.installedAt)})`,
    };
  }

  async function doRestore() {
    if (!confirm || restoring) return;
    restoring = true;
    const target = confirm;
    try {
      const report = await lockfileStore.restore(
        profileConfigDir,
        resourceId,
        target.sha256,
      );
      confirm = null;
      if (report.noOp) {
        showToast('success', 'Already on that version');
      } else {
        showToast('success', `Restored ${target.label}`);
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      showToast('error', `Restore failed: ${msg}`);
      confirm = null;
    } finally {
      restoring = false;
    }
  }
</script>

<section class="history-pane">
  {#if isEmpty}
    <div class="history-empty">
      <History size={28} strokeWidth={1.2} />
      <p>No history recorded for this resource yet.</p>
      <p class="history-hint">
        New installs and edits will appear here automatically.
      </p>
    </div>
  {:else}
    {#if current?.drifted}
      <div class="drift-banner">
        <AlertTriangle size={14} />
        <div>
          <strong>Modified externally.</strong>
          <span>
            The on-disk file no longer matches the lockfile. Restore a
            recorded version below to roll back, or save your edits via
            Weplex to update the lockfile.
          </span>
        </div>
      </div>
    {/if}

    {#if current}
      <div class="history-row history-row-current">
        <div class="row-main">
          <span class="row-label">{labelFor(current)}</span>
          <span class="badge badge-current">current</span>
          <span class="badge badge-source badge-source-{current.source}">
            {current.source}
          </span>
        </div>
        <span class="row-meta" title={current.installedAt}>
          installed {relativeDate(current.installedAt)} by {current.installedBy}
        </span>
      </div>
    {/if}

    {#if history.length > 0}
      <h3 class="history-title">Previous versions</h3>
      <ul class="history-list">
        {#each history as entry (entry.sha256 + entry.installedAt)}
          <li class="history-row">
            <div class="row-main">
              <span class="row-label">{labelFor(entry)}</span>
              <span class="badge badge-source badge-source-{entry.source}">
                {entry.source}
              </span>
            </div>
            <span class="row-meta" title={entry.installedAt}>
              {relativeDate(entry.installedAt)}
            </span>
            <Button
              variant="secondary"
              size="sm"
              disabled={restoring}
              onclick={() => askRestore(entry)}
            >
              <RotateCcw size={12} /> Restore
            </Button>
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
</section>

{#if confirm}
  <div
    class="restore-overlay"
    role="presentation"
    onclick={(e) => {
      if (e.target === e.currentTarget && !restoring) confirm = null;
    }}
    onkeydown={(e) => {
      if (e.key === 'Escape' && !restoring) confirm = null;
    }}
  >
    <div class="restore-dialog" role="dialog" aria-label="Confirm restore">
      <h3>Restore previous version?</h3>
      <p>
        Replace the current version with <strong>{confirm.label}</strong>?
        The current version will be moved to history and remains restorable.
      </p>
      <div class="restore-actions">
        <Button
          variant="secondary"
          disabled={restoring}
          onclick={() => (confirm = null)}
        >
          Cancel
        </Button>
        <Button variant="primary" disabled={restoring} onclick={doRestore}>
          {restoring ? 'Restoring...' : 'Restore'}
        </Button>
      </div>
    </div>
  </div>
{/if}

{#if toast}
  <div class="history-toast history-toast-{toast.type}">{toast.text}</div>
{/if}

<style>
  .history-pane {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .history-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 32px 16px;
    color: var(--weplex-text-muted);
    text-align: center;
  }
  .history-empty p {
    margin: 0;
    font-size: 13px;
  }
  .history-empty .history-hint {
    font-size: 11px;
    opacity: 0.75;
  }

  .drift-banner {
    display: flex;
    gap: 8px;
    padding: 10px 12px;
    border-radius: var(--weplex-radius-md);
    background: color-mix(in srgb, var(--weplex-warning, #f59e0b) 12%, transparent);
    border: 1px solid
      color-mix(in srgb, var(--weplex-warning, #f59e0b) 35%, transparent);
    color: var(--weplex-warning, #f59e0b);
    font-size: 12px;
    line-height: 1.45;
  }
  .drift-banner strong {
    display: block;
    font-weight: 700;
    margin-bottom: 2px;
  }
  .drift-banner span {
    color: var(--weplex-text-secondary);
  }

  .history-title {
    font-size: 11px;
    font-weight: 700;
    color: var(--weplex-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    margin: 6px 0 0;
  }

  .history-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .history-row {
    display: grid;
    grid-template-columns: 1fr auto auto;
    align-items: center;
    gap: 10px;
    padding: 8px 12px;
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-md);
    background: var(--weplex-surface);
  }

  .history-row-current {
    border-color: color-mix(in srgb, var(--weplex-accent) 35%, transparent);
    background: color-mix(in srgb, var(--weplex-accent) 6%, transparent);
    grid-template-columns: 1fr auto;
  }

  .row-main {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .row-label {
    font-family: var(--weplex-font-mono);
    font-size: 12px;
    font-weight: 600;
    color: var(--weplex-text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .badge {
    font-size: 9px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    padding: 2px 6px;
    border-radius: var(--weplex-radius-sm);
    background: var(--weplex-surface-hover);
    color: var(--weplex-text-muted);
  }
  .badge-current {
    background: color-mix(in srgb, var(--weplex-accent) 18%, transparent);
    color: var(--weplex-accent);
  }
  .badge-source-builtin {
    background: color-mix(in srgb, var(--weplex-text-muted) 16%, transparent);
  }
  .badge-source-user {
    background: color-mix(in srgb, var(--weplex-active, #10b981) 16%, transparent);
    color: var(--weplex-active, #10b981);
  }
  .badge-source-marketplace {
    background: color-mix(in srgb, var(--weplex-accent) 16%, transparent);
    color: var(--weplex-accent);
  }
  .badge-source-imported {
    background: color-mix(in srgb, var(--weplex-warning, #f59e0b) 16%, transparent);
    color: var(--weplex-warning, #f59e0b);
  }

  .row-meta {
    font-size: 11px;
    color: var(--weplex-text-muted);
    white-space: nowrap;
  }

  .restore-overlay {
    position: fixed;
    inset: 0;
    z-index: 9000;
    background: rgba(0, 0, 0, 0.45);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .restore-dialog {
    width: 380px;
    padding: 20px;
    background: var(--weplex-surface);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-xl);
    box-shadow: var(--weplex-shadow-overlay);
  }
  .restore-dialog h3 {
    margin: 0 0 8px;
    font-size: 14px;
    font-weight: 700;
    color: var(--weplex-text);
  }
  .restore-dialog p {
    margin: 0 0 16px;
    font-size: 13px;
    color: var(--weplex-text-secondary);
    line-height: 1.5;
  }
  .restore-actions {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
  }

  .history-toast {
    position: fixed;
    bottom: 24px;
    left: 50%;
    transform: translateX(-50%);
    padding: 8px 20px;
    border-radius: var(--weplex-radius-full, 999px);
    font-size: 12px;
    font-weight: 500;
    z-index: 9999;
    pointer-events: none;
  }
  .history-toast-success {
    background: var(--weplex-active, #10b981);
    color: white;
  }
  .history-toast-error {
    background: var(--weplex-error, #ef4444);
    color: white;
  }
</style>

<script lang="ts">
  import { Modal, Button } from '../ui';
  import { open } from '@tauri-apps/plugin-dialog';
  import { Upload, AlertTriangle, FileArchive } from 'lucide-svelte';
  import {
    inspectArchive,
    importProfile,
  } from '../../services/profileArchive';
  import type {
    ArchiveInspection,
    ConflictPolicy,
    ImportReport,
  } from '../../types/lockfile';

  interface Props {
    targetConfigDir: string;
    open: boolean;
    onclose: () => void;
  }

  let { targetConfigDir, open: isOpen, onclose }: Props = $props();

  // Two-stage flow: pick → review.
  type Stage = 'pick' | 'review' | 'importing' | 'done';
  let stage = $state<Stage>('pick');

  let archivePath = $state<string | null>(null);
  let inspection = $state<ArchiveInspection | null>(null);
  let lastReport = $state<ImportReport | null>(null);

  let inspecting = $state(false);
  let importing = $state(false);
  let error = $state<string | null>(null);

  /** Pretty file name from a path. */
  function basename(p: string): string {
    const i = p.lastIndexOf('/');
    return i >= 0 ? p.slice(i + 1) : p;
  }

  async function pickArchive() {
    error = null;
    try {
      const picked = await open({
        title: 'Select Weplex profile archive',
        multiple: false,
        directory: false,
        filters: [
          { name: 'Weplex profile archive', extensions: ['tar.gz', 'gz'] },
        ],
      });
      if (!picked || Array.isArray(picked)) {
        // Cancelled.
        return;
      }
      archivePath = picked;
      inspecting = true;
      try {
        inspection = await inspectArchive(picked, targetConfigDir);
        stage = 'review';
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        error = `Could not read archive: ${msg}`;
        archivePath = null;
        inspection = null;
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      error = `File picker failed: ${msg}`;
    } finally {
      inspecting = false;
    }
  }

  async function applyImport(policy: ConflictPolicy) {
    if (!archivePath || importing) return;
    importing = true;
    stage = 'importing';
    error = null;
    try {
      const report = await importProfile(targetConfigDir, archivePath, policy);
      lastReport = report;
      stage = 'done';
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      error = `Import failed: ${msg}`;
      // Step back to review so the user can retry or cancel.
      stage = 'review';
    } finally {
      importing = false;
    }
  }

  function close() {
    if (importing) return;
    onclose();
  }
</script>

{#if isOpen}
  <Modal
    onclose={close}
    position="center"
    label="Import profile archive"
    class="archive-dialog"
  >
    {#if stage === 'pick'}
      <header class="archive-header">
        <div class="archive-icon"><FileArchive size={18} /></div>
        <h2>Import profile archive</h2>
      </header>
      <p class="archive-body">
        Choose a <code>.weplex.profile.tar.gz</code> file to merge into this profile.
        Existing resources can be overwritten or kept on a per-archive basis.
      </p>
      {#if error}
        <p class="archive-error"><AlertTriangle size={13} /> {error}</p>
      {/if}
      <div class="archive-actions">
        <Button variant="secondary" onclick={close}>Cancel</Button>
        <Button variant="primary" disabled={inspecting} onclick={pickArchive}>
          <Upload size={13} />
          {inspecting ? 'Reading...' : 'Choose archive'}
        </Button>
      </div>

    {:else if stage === 'review' && inspection && archivePath}
      <header class="archive-header">
        <div class="archive-icon"><FileArchive size={18} /></div>
        <h2>Review archive contents</h2>
      </header>
      <p class="archive-body">
        About to import <strong>{inspection.resourceCount}</strong>
        resource{inspection.resourceCount === 1 ? '' : 's'} from
        <code>{basename(archivePath)}</code>.
      </p>

      {#if inspection.conflicts.length > 0}
        <div class="conflict-banner">
          <AlertTriangle size={14} />
          <div>
            <strong>
              {inspection.conflicts.length}
              conflict{inspection.conflicts.length === 1 ? '' : 's'}
              with this profile.
            </strong>
            <span>
              Choose how to resolve them. Skipped conflicts keep the
              existing on-disk version; overwritten conflicts move the
              current version to history before applying the archive.
            </span>
          </div>
        </div>
        <ul class="conflict-list">
          {#each inspection.conflicts as c (c.resourceId)}
            <li>
              <span class="conflict-id">{c.resourceId}</span>
              <span class="conflict-shas">
                <code title="Current version">
                  {c.existingSha256.slice(0, 8)}
                </code>
                →
                <code title="Archive version">
                  {c.incomingSha256.slice(0, 8)}
                </code>
              </span>
            </li>
          {/each}
        </ul>
      {:else}
        <p class="archive-hint">No conflicts — clean import.</p>
      {/if}

      {#if error}
        <p class="archive-error"><AlertTriangle size={13} /> {error}</p>
      {/if}

      <div class="archive-actions">
        <Button variant="secondary" disabled={importing} onclick={close}>
          Cancel
        </Button>
        {#if inspection.conflicts.length > 0}
          <Button
            variant="danger"
            disabled={importing}
            onclick={() => applyImport('overwriteAll')}
          >
            Overwrite all
          </Button>
          <Button
            variant="primary"
            disabled={importing}
            onclick={() => applyImport('skipAll')}
          >
            Skip conflicts
          </Button>
        {:else}
          <Button
            variant="primary"
            disabled={importing}
            onclick={() => applyImport('skipAll')}
          >
            Import
          </Button>
        {/if}
      </div>

    {:else if stage === 'importing'}
      <header class="archive-header">
        <div class="archive-icon"><FileArchive size={18} /></div>
        <h2>Importing archive...</h2>
      </header>
      <p class="archive-body">Applying changes — this should be quick.</p>

    {:else if stage === 'done' && lastReport}
      <header class="archive-header">
        <div class="archive-icon"><FileArchive size={18} /></div>
        <h2>Import complete</h2>
      </header>
      <ul class="result-list">
        <li><strong>{lastReport.installed}</strong> installed</li>
        <li><strong>{lastReport.overwritten}</strong> overwritten</li>
        <li><strong>{lastReport.skipped}</strong> skipped</li>
      </ul>
      <div class="archive-actions">
        <Button variant="primary" onclick={close}>Done</Button>
      </div>
    {/if}
  </Modal>
{/if}

<style>
  :global(.archive-dialog) {
    width: 460px;
    max-width: 92vw;
    padding: 22px;
    background: var(--weplex-surface);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-xl);
    box-shadow: var(--weplex-shadow-overlay);
  }

  .archive-header {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 14px;
  }
  .archive-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border-radius: var(--weplex-radius-md);
    background: var(--weplex-surface-hover);
    color: var(--weplex-accent);
    flex-shrink: 0;
  }
  .archive-header h2 {
    margin: 0;
    font-size: 14px;
    font-weight: 700;
    color: var(--weplex-text);
  }

  .archive-body {
    font-size: 13px;
    color: var(--weplex-text-secondary);
    line-height: 1.55;
    margin: 0 0 14px;
  }
  .archive-body code,
  .conflict-shas code,
  .conflict-id {
    font-family: var(--weplex-font-mono);
    font-size: 11px;
    background: var(--weplex-surface-hover);
    padding: 1px 5px;
    border-radius: 3px;
  }
  .archive-hint {
    font-size: 12px;
    color: var(--weplex-text-muted);
    margin: 0 0 12px;
  }

  .archive-error {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0 0 12px;
    padding: 8px 10px;
    border-radius: var(--weplex-radius-sm);
    background: color-mix(in srgb, var(--weplex-error) 12%, transparent);
    border: 1px solid color-mix(in srgb, var(--weplex-error) 30%, transparent);
    color: var(--weplex-error);
    font-size: 12px;
  }

  .conflict-banner {
    display: flex;
    gap: 8px;
    padding: 10px 12px;
    margin-bottom: 10px;
    border-radius: var(--weplex-radius-md);
    background: color-mix(in srgb, var(--weplex-warning, #f59e0b) 12%, transparent);
    border: 1px solid
      color-mix(in srgb, var(--weplex-warning, #f59e0b) 35%, transparent);
    color: var(--weplex-warning, #f59e0b);
    font-size: 12px;
    line-height: 1.45;
  }
  .conflict-banner strong { display: block; font-weight: 700; margin-bottom: 2px; }
  .conflict-banner span { color: var(--weplex-text-secondary); }

  .conflict-list {
    list-style: none;
    margin: 0 0 14px;
    padding: 0;
    max-height: 180px;
    overflow-y: auto;
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-md);
  }
  .conflict-list li {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    padding: 6px 10px;
    border-bottom: 1px solid var(--weplex-border);
    font-size: 12px;
  }
  .conflict-list li:last-child { border-bottom: none; }
  .conflict-shas { color: var(--weplex-text-muted); }

  .result-list {
    list-style: none;
    margin: 0 0 16px;
    padding: 0;
    display: flex;
    gap: 16px;
  }
  .result-list li {
    flex: 1;
    text-align: center;
    padding: 10px 8px;
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-md);
    font-size: 11px;
    color: var(--weplex-text-muted);
  }
  .result-list strong {
    display: block;
    font-size: 18px;
    color: var(--weplex-text);
    margin-bottom: 2px;
  }

  .archive-actions {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
    flex-wrap: wrap;
  }
</style>

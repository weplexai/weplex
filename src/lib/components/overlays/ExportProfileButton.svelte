<script lang="ts">
  import { Download } from 'lucide-svelte';
  import { save } from '@tauri-apps/plugin-dialog';
  import { exportProfile } from '../../services/profileArchive';

  interface Props {
    profileConfigDir: string;
    profileName: string;
  }

  let { profileConfigDir, profileName }: Props = $props();

  let busy = $state(false);
  let toast = $state<{ type: 'success' | 'error'; text: string } | null>(null);

  function showToast(type: 'success' | 'error', text: string, ms = 4000) {
    toast = { type, text };
    setTimeout(() => {
      toast = null;
    }, ms);
  }

  /** Default file name derived from the profile — slug-safe and clear. */
  function defaultFilename(): string {
    const slug = profileName
      .toLowerCase()
      .replace(/[^a-z0-9-_]+/g, '-')
      .replace(/^-+|-+$/g, '')
      || 'profile';
    return `${slug}.weplex.profile.tar.gz`;
  }

  function formatBytes(n: number): string {
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
    if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
    return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
  }

  async function onClick() {
    if (busy) return;
    busy = true;
    try {
      const dest = await save({
        title: 'Export Weplex profile',
        defaultPath: defaultFilename(),
        filters: [
          { name: 'Weplex profile archive', extensions: ['tar.gz'] },
        ],
      });
      if (!dest) {
        // User cancelled the save dialog — silent.
        return;
      }
      const report = await exportProfile(profileConfigDir, dest);
      showToast(
        'success',
        `Exported ${report.resourceCount} resource${report.resourceCount === 1 ? '' : 's'} (${formatBytes(report.bytes)})`,
      );
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      showToast('error', `Export failed: ${msg}`);
    } finally {
      busy = false;
    }
  }
</script>

<button
  class="resources-action-btn"
  onclick={onClick}
  disabled={busy}
  title="Bundle this profile's resources, history, and cache into a portable archive"
>
  <Download size={13} />
  <span>{busy ? 'Exporting...' : 'Export archive'}</span>
</button>

{#if toast}
  <div class="export-toast export-toast-{toast.type}">{toast.text}</div>
{/if}

<style>
  .resources-action-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 7px 10px;
    border: 1px dashed var(--weplex-border);
    border-radius: var(--weplex-radius-md);
    background: transparent;
    color: var(--weplex-text-muted);
    font-size: 12px;
    cursor: pointer;
    transition: all var(--weplex-duration-fast);
  }
  .resources-action-btn:hover {
    border-color: var(--weplex-accent);
    color: var(--weplex-accent);
    border-style: solid;
  }
  .resources-action-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .export-toast {
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
  .export-toast-success {
    background: var(--weplex-active, #10b981);
    color: white;
  }
  .export-toast-error {
    background: var(--weplex-error, #ef4444);
    color: white;
  }
</style>

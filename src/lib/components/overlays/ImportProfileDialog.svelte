<script lang="ts">
  import { Modal, Button } from '../ui';
  import { Copy, ArrowRight } from 'lucide-svelte';

  interface Props {
    profileName: string;
    counts: { agents: number; rules: number; skills: number };
    onconfirm: (sync: boolean) => void;
    onclose: () => void;
  }

  let { profileName, counts, onconfirm, onclose }: Props = $props();

  let parts = $derived.by(() => {
    const p: string[] = [];
    if (counts.agents > 0) p.push(`${counts.agents} agent${counts.agents > 1 ? 's' : ''}`);
    if (counts.rules > 0) p.push(`${counts.rules} rule${counts.rules > 1 ? 's' : ''}`);
    if (counts.skills > 0) p.push(`${counts.skills} skill${counts.skills > 1 ? 's' : ''}`);
    return p;
  });

  let summary = $derived(
    parts.length === 0
      ? null
      : parts.length === 1
        ? parts[0]
        : parts.length === 2
          ? `${parts[0]} and ${parts[1]}`
          : `${parts[0]}, ${parts[1]}, and ${parts[2]}`,
  );
</script>

<Modal onclose={onclose} position="center" label="Import profile" class="import-dialog">
  <div class="import-header">
    <div class="import-icon">
      <ArrowRight size={18} strokeWidth={2} />
    </div>
    <h2>Import Profile: <strong>{profileName}</strong></h2>
  </div>

  {#if summary}
    <p class="import-body">
      You have {summary} in your shared library.
    </p>
    <p class="import-question">
      Copy them to <strong>{profileName}</strong>?
    </p>
    <p class="import-hint">You can always do this later from Resources.</p>

    <div class="import-actions">
      <Button variant="secondary" onclick={() => onconfirm(false)}>
        No, import only
      </Button>
      <Button variant="primary" onclick={() => onconfirm(true)}>
        <Copy size={13} /> Yes, copy
      </Button>
    </div>
  {:else}
    <p class="import-body">
      No shared resources yet. The profile will be added to Weplex.
    </p>

    <div class="import-actions">
      <Button variant="primary" onclick={() => onconfirm(false)}>
        Import
      </Button>
    </div>
  {/if}
</Modal>

<style>
  :global(.import-dialog) {
    width: 380px;
    padding: 24px;
    background: var(--weplex-surface);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-xl);
    box-shadow: var(--weplex-shadow-overlay);
  }

  .import-header {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 16px;
  }

  .import-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 36px;
    height: 36px;
    border-radius: var(--weplex-radius-lg);
    background: var(--weplex-accent-subtle);
    color: var(--weplex-accent);
    flex-shrink: 0;
  }

  .import-header h2 {
    font-size: var(--weplex-text-md);
    font-weight: 500;
    color: var(--weplex-text);
    font-family: var(--weplex-font-mono);
    margin: 0;
  }
  .import-header h2 strong {
    font-weight: 700;
  }

  .import-body {
    font-size: var(--weplex-text-base);
    color: var(--weplex-text-secondary);
    margin: 0 0 8px;
    line-height: 1.6;
  }

  .import-question {
    font-size: var(--weplex-text-base);
    color: var(--weplex-text);
    margin: 0 0 6px;
  }
  .import-question strong {
    font-weight: 600;
    font-family: var(--weplex-font-mono);
  }

  .import-hint {
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
    margin: 0 0 20px;
  }

  .import-actions {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
  }
</style>

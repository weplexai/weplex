<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { commandStore, type Command } from '../../stores/commandStore.svelte';
  import { featureFlags } from '../../stores/featureFlagsStore.svelte';
  import type { Session } from '../../types';

  let { session }: { session: Session } = $props();

  let commands = $derived(commandStore.getPanelCommands());
  let isBusy = $derived(session.status === 'thinking' || session.status === 'active');
  let executing = $state(false);

  onMount(() => {
    commandStore.load(session.cwd);
  });

  async function execute(cmd: Command) {
    if (isBusy || executing) return;
    executing = true;
    const text = commandStore.resolveForPty(cmd, session);
    if (!text) { executing = false; return; }
    try {
      await invoke('write_pty', { sessionId: session.id, data: text + '\n' });
    } catch (e) {
      console.error('[weplex] Failed to send command:', e);
    } finally {
      setTimeout(() => { executing = false; }, 500);
    }
  }
</script>

{#if featureFlags.commands}
  <!-- User Commands -->
  {#if commands.user.length > 0}
    <section class="section">
      <h3 class="section-title">Commands</h3>
      <div class="cmd-list">
        {#each commands.user as cmd (cmd.name)}
          <button class="cmd-btn" class:disabled={isBusy || executing} onclick={() => execute(cmd)}>
            <span class="cmd-icon" style="--cmd-color: var(--weplex-{commandStore.safeIconColor(cmd)})">{cmd.icon}</span>
            <span class="cmd-name">{cmd.name}</span>
            <span class="cmd-slash">/{cmd.name}</span>
          </button>
        {/each}
      </div>
    </section>
  {/if}

  <!-- Project Commands -->
  {#if commands.project.length > 0}
    <div class="divider"></div>
    <section class="section">
      <h3 class="section-title">Project Commands</h3>
      <div class="cmd-list">
        {#each commands.project as cmd (cmd.name)}
          <button class="cmd-btn" class:disabled={isBusy || executing} onclick={() => execute(cmd)}>
            <span class="cmd-icon" style="--cmd-color: var(--weplex-{commandStore.safeIconColor(cmd)})">{cmd.icon}</span>
            <span class="cmd-name">{cmd.name}</span>
            <span class="cmd-slash">/{cmd.name}</span>
          </button>
        {/each}
      </div>
    </section>
  {/if}

  {#if commands.user.length === 0 && commands.project.length === 0}
    <section class="section">
      <h3 class="section-title">Commands</h3>
      <p class="empty-hint">No commands found. Create them in Hub or add .md files to ~/.claude/commands/</p>
    </section>
  {/if}
{/if}

<style>
  .section { margin-bottom: 16px; }

  .section-title {
    font-size: 10px;
    font-weight: 600;
    color: var(--weplex-text-muted);
    letter-spacing: 0.06em;
    margin-bottom: 8px;
    text-transform: uppercase;
  }

  .cmd-list { display: flex; flex-direction: column; gap: 4px; }

  .cmd-btn {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 7px 10px;
    background: var(--weplex-surface);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-md);
    color: var(--weplex-text);
    font-size: var(--weplex-text-sm);
    font-family: var(--weplex-font-ui);
    cursor: pointer;
    transition: all var(--weplex-duration-fast);
    text-align: left;
  }

  .cmd-btn:hover:not(.disabled) {
    border-color: var(--weplex-accent);
    background: var(--weplex-surface-hover);
  }

  .cmd-btn:active:not(.disabled) { background: var(--weplex-accent-subtle); }

  .cmd-btn.disabled { opacity: 0.4; cursor: not-allowed; }
  .cmd-btn.disabled:hover { border-color: var(--weplex-border); background: var(--weplex-surface); }

  .cmd-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    border-radius: 4px;
    background: color-mix(in srgb, var(--cmd-color) 15%, transparent);
    color: var(--cmd-color);
    font-size: 10px;
    font-weight: 700;
    font-family: var(--weplex-font-mono);
    flex-shrink: 0;
  }

  .cmd-name { flex: 1; font-weight: 500; }

  .cmd-slash {
    font-size: 10px;
    font-family: var(--weplex-font-mono);
    color: var(--weplex-text-muted);
    opacity: 0;
    transition: opacity 0.15s;
  }

  .cmd-btn:hover .cmd-slash { opacity: 1; }

  .divider {
    height: 1px;
    background: var(--weplex-border);
    margin: 4px 0 12px;
  }

  .empty-hint {
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
    line-height: 1.5;
  }
</style>

<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { Terminal } from '@xterm/xterm';
  import { FitAddon } from '@xterm/addon-fit';
  import { sessionStore } from '../../stores/sessionStore';
  import { settingsStore } from '../../stores/settingsStore';
  import { pipelineWsService } from '../../services/pipelineWsService';

  let {
    spaceId,
    sessionName,
    ownerName,
    sessionId,
  }: {
    spaceId: string;
    sessionName: string;
    ownerName: string;
    sessionId: number;
  } = $props();

  let container: HTMLDivElement;
  let term: Terminal;
  let fitAddon: FitAddon;
  let resizeObserver: ResizeObserver;
  let destroyed = false;
  let unsubStream: (() => void) | null = null;
  let unsubScrollback: (() => void) | null = null;
  let unsubEnded: (() => void) | null = null;
  let ended = $state(false);

  let isVisible = $derived(sessionId === sessionStore.activeSessionId);

  onMount(() => {
    term = new Terminal({
      fontFamily: settingsStore.settings.fontFamily,
      fontSize: settingsStore.settings.fontSize,
      cursorStyle: 'bar',
      cursorBlink: false,
      disableStdin: true, // Read-only
      theme: {
        background: '#0a0a0f',
        foreground: '#e4e4ef',
        cursor: 'transparent', // Hidden cursor
        selectionBackground: '#fc5e4440',
      },
      scrollback: 1000,
      allowProposedApi: true,
    });

    fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    term.open(container);

    // Fit after first paint
    requestAnimationFrame(() => {
      if (!destroyed) fitAddon.fit();
    });

    // Auto-resize
    resizeObserver = new ResizeObserver(() => {
      if (!destroyed) {
        requestAnimationFrame(() => fitAddon.fit());
      }
    });
    resizeObserver.observe(container);

    // Join as spectator
    pipelineWsService.spectateJoin(spaceId, sessionName);

    // Listen for scrollback (sent on join — raw chunks preserving ANSI)
    unsubScrollback = pipelineWsService.onSpectateScrollback((data: any) => {
      if (data.spaceId === spaceId && data.sessionName === sessionName) {
        // Write raw chunks sequentially to preserve terminal state
        const chunks = data.chunks || data.lines || [];
        for (const chunk of chunks) {
          term.write(chunk);
        }
      }
    });

    // Listen for live PTY stream
    unsubStream = pipelineWsService.onPtyStream((data) => {
      if (data.spaceId === spaceId && data.sessionName === sessionName) {
        term.write(data.chunk);
      }
    });

    // Listen for session ended
    unsubEnded = pipelineWsService.onSpectateEnded((data) => {
      if (data.spaceId === spaceId && data.sessionName === sessionName) {
        ended = true;
        term.writeln('\r\n\x1b[90m--- Session ended ---\x1b[0m');
      }
    });
  });

  onDestroy(() => {
    destroyed = true;
    pipelineWsService.spectateLeave(spaceId, sessionName);
    unsubStream?.();
    unsubScrollback?.();
    unsubEnded?.();
    resizeObserver?.disconnect();
    term?.dispose();
  });
</script>

<div
  class="spectator-view"
  class:visible={isVisible}
>
  <div class="spectator-bar">
    <span class="spectator-icon">👁</span>
    <span class="spectator-label">
      Spectating {ownerName}'s session
      {#if ended}
        <span class="ended-badge">ended</span>
      {/if}
    </span>
    <span class="spectator-mode">read-only</span>
  </div>
  <div class="terminal-container" bind:this={container}></div>
</div>

<style>
  .spectator-view {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    display: none;
    flex-direction: column;
  }

  .spectator-view.visible {
    display: flex;
  }

  .spectator-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 12px;
    background: var(--weplex-surface);
    border-bottom: 1px solid var(--weplex-border);
    font-size: var(--weplex-text-xs);
    flex-shrink: 0;
  }

  .spectator-icon {
    font-size: 12px;
  }

  .spectator-label {
    color: var(--weplex-text-muted);
  }

  .spectator-mode {
    margin-left: auto;
    color: var(--weplex-accent);
    font-weight: 500;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .ended-badge {
    color: var(--weplex-error);
    margin-left: 4px;
  }

  .terminal-container {
    flex: 1;
    overflow: hidden;
  }
</style>

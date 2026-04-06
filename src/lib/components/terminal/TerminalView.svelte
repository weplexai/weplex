<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { Terminal } from '@xterm/xterm';
  import { FitAddon } from '@xterm/addon-fit';
  import { WebLinksAddon } from '@xterm/addon-web-links';
  import '@xterm/xterm/css/xterm.css';
  import { sessionStore } from '../../stores/sessionStore';
  import { settingsStore } from '../../stores/settingsStore';
  import { spaceStore } from '../../stores/spaceStore';
  import { profileStore } from '../../stores/profileStore';
  import { splitStore } from '../../stores/splitStore';
  import { pipelineInjectStore } from '../../stores/pipelineInjectStore.svelte';
  import { contextInjectionStore } from '../../stores/contextInjectionStore.svelte';
  import { terminalRegistry } from '../../stores/terminalRegistry';

  let { sessionId }: { sessionId: number } = $props();
  let isActive = $derived(sessionStore.activeSessionId === sessionId);
  let outerEl: HTMLDivElement;

  let containerEl: HTMLDivElement;
  let term: Terminal;
  let fitAddon: FitAddon;
  let resizeObserver: ResizeObserver;
  let ptyWriter: ((data: string) => void) | null = null;
  let unlisten: (() => void) | null = null;
  let unlistenDrop: (() => void) | null = null;
  let disposables: { dispose(): void }[] = [];
  let pendingTimers: ReturnType<typeof setTimeout>[] = [];
  let destroyed = false;
  let waitingTimer: ReturnType<typeof setTimeout> | null = null;
  let idleTimer: ReturnType<typeof setTimeout> | null = null;
  let statePollTimer: ReturnType<typeof setTimeout> | null = null;
  let isDragOver = $state(false);
  // Set in connectPty once session type is known
  let isAgentMode = false;

  // Reusable TextDecoder for converting raw PTY bytes to string (pattern matching only).
  // stream:true keeps state across calls so multibyte sequences split across
  // chunks are decoded correctly — the same guarantee xterm.js gives internally.
  const textDecoder = new TextDecoder('utf-8', { fatal: false });

  function b64ToBytes(b64: string): Uint8Array {
    const binary = atob(b64);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
    return bytes;
  }

  let scheduleTransition: (() => void) | null = null;
  let lastUserInputAt = 0;

  onMount(() => {
    const settings = settingsStore.settings;

    term = new Terminal({
      cursorBlink: true,
      cursorStyle: 'bar',
      scrollback: 5000,
      fontSize: settings.fontSize,
      fontFamily: settings.fontFamily,
      theme: {
        background: '#0a0a0f',
        foreground: '#e8e8ed',
        cursor: 'transparent',
        cursorAccent: '#0a0a0f',
        selectionBackground: 'rgba(139, 92, 246, 0.3)',
        selectionForeground: '#e8e8ed',
        black: '#1a1a25',
        red: '#ef4444',
        green: '#10b981',
        yellow: '#f59e0b',
        blue: '#3b82f6',
        magenta: '#8b5cf6',
        cyan: '#06b6d4',
        white: '#e8e8ed',
        brightBlack: '#6b6b80',
        brightRed: '#f87171',
        brightGreen: '#34d399',
        brightYellow: '#fbbf24',
        brightBlue: '#60a5fa',
        brightMagenta: '#a78bfa',
        brightCyan: '#22d3ee',
        brightWhite: '#f9fafb',
      },
      allowProposedApi: true,
    });

    fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    term.loadAddon(new WebLinksAddon());

    // Let Cmd/Ctrl shortcuts pass through to our global handler
    term.attachCustomKeyEventHandler((e) => {
      const isMod = e.metaKey || e.ctrlKey;
      if (
        isMod &&
        !e.shiftKey &&
        ['b', 'k', 'p', 'n', 'w', 'i', ',', 'd', ']', '['].includes(e.key)
      ) {
        return false; // Don't handle — let it bubble to window
      }
      // Cmd+Shift shortcuts (split vertical, close pane, agents)
      if (isMod && e.shiftKey && ['d', 'w', 'a'].includes(e.key)) {
        return false;
      }
      return true; // Let xterm handle everything else
    });

    term.open(containerEl);
    // Only fit if container has real dimensions (skip when in hidden terminal-host)
    if (containerEl.offsetWidth > 0 && containerEl.offsetHeight > 0) {
      fitAddon.fit();
    }

    disposables.push(
      term.onData((data) => {
        if (ptyWriter) ptyWriter(data);
        // Agent: user input is the reliable signal that work was requested
        if (isAgentMode && scheduleTransition) {
          lastUserInputAt = Date.now();
          sessionStore.updateStatus(sessionId, 'active');
          scheduleTransition();
        }
      }),
    );

    connectPty();

    resizeObserver = new ResizeObserver(() => {
      if (containerEl.offsetWidth > 0 && containerEl.offsetHeight > 0) {
        fitAddon.fit();
      }
    });
    resizeObserver.observe(containerEl);

    // Drag & drop: paste file paths into PTY (like Terminal.app)
    setupDragDrop();

    // Register for portal moves (split panes grab this element)
    terminalRegistry.register(sessionId, outerEl);
  });

  // Re-fit and focus when this terminal becomes the active one
  $effect(() => {
    if (isActive && fitAddon && term) {
      requestAnimationFrame(() => {
        fitAddon.fit();
        term.focus();
      });
    }
  });

  // Reactively update terminal font when settings change
  $effect(() => {
    const { fontSize, fontFamily } = settingsStore.settings;
    if (term) {
      term.options.fontSize = fontSize;
      term.options.fontFamily = fontFamily;
      if (fitAddon) fitAddon.fit();
    }
  });

  onDestroy(() => {
    destroyed = true;
    pendingTimers.forEach(clearTimeout);
    pendingTimers = [];
    if (waitingTimer) clearTimeout(waitingTimer);
    if (idleTimer) clearTimeout(idleTimer);
    if (statePollTimer) clearTimeout(statePollTimer);
    disposables.forEach((d) => d.dispose());
    disposables = [];
    resizeObserver?.disconnect();
    unlisten?.();
    unlistenDrop?.();
    term?.dispose();
    terminalRegistry.unregister(sessionId);
  });

  function escapePath(p: string): string {
    // Escape characters that shells treat as special
    if (/[ '"\\(){}$!&|;<>?*#~`\[\]]/.test(p)) {
      // Use single quotes, escaping existing single quotes
      return "'" + p.replace(/'/g, "'\\''") + "'";
    }
    return p;
  }

  async function setupDragDrop() {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    const appWindow = getCurrentWindow();

    unlistenDrop = await appWindow.onDragDropEvent((event) => {
      if (!isActive) return;

      if (event.payload.type === 'enter') {
        isDragOver = true;
      } else if (event.payload.type === 'leave') {
        isDragOver = false;
      } else if (event.payload.type === 'drop') {
        isDragOver = false;
        const paths = event.payload.paths;
        if (paths.length > 0 && ptyWriter) {
          const escaped = paths.map(escapePath).join(' ');
          ptyWriter(escaped);
        }
      }
    });
  }

  interface ClaudeUsage {
    input_tokens: number;
    output_tokens: number;
    cache_read_tokens: number;
    cache_write_tokens: number;
    model: string | null;
    turns: number;
  }

  function startUsagePolling(invoke: Function, cwd: string, claudeId: string) {
    const poll = async () => {
      if (destroyed) return;
      try {
        const usage = (await invoke('get_claude_usage', {
          cwd,
          sessionId: claudeId,
        })) as ClaudeUsage | null;
        if (usage) {
          sessionStore.update(sessionId, {
            tokensIn: usage.input_tokens,
            tokensOut: usage.output_tokens,
            cacheReadTokens: usage.cache_read_tokens,
            cacheWriteTokens: usage.cache_write_tokens,
            turns: usage.turns,
            model: usage.model || undefined,
          });
        }
      } catch (e) {
        console.warn('[Weplex] usage poll error:', e);
      }
      if (!destroyed) {
        const t = setTimeout(poll, 30000);
        pendingTimers.push(t);
      }
    };
    const t = setTimeout(poll, 3000);
    pendingTimers.push(t);
  }

  async function connectPty() {
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const session = sessionStore.sessions.find((s) => s.id === sessionId);
      const isRestored = sessionStore.isRestored(sessionId);

      // For restored agent sessions, modify command to resume (only if session had real activity)
      let command = session?.command || null;
      if (isRestored && command) {
        // Only resume sessions that were actively running before restart
        const wasRunning =
          session?.previousStatus === 'active' ||
          session?.previousStatus === 'idle' ||
          session?.previousStatus === 'waiting';
        if (
          wasRunning &&
          session?.hasOutput &&
          session?.claudeSessionId &&
          command.includes('claude') &&
          !command.includes('--resume') &&
          !command.includes('--continue')
        ) {
          command = command + ' --resume ' + session.claudeSessionId;
        }
        sessionStore.clearRestored(sessionId);
      }

      const isClaude = session?.type === 'agent' && session?.agentType === 'claude' && session?.cwd;
      const isAgentSession = session?.type === 'agent';
      const launchTime = Date.now();

      // Expose session type to onData handler (runs before connectPty resolves)
      isAgentMode = isAgentSession;

      // Resolve profile: session override → space default → default profile
      const space = spaceStore.spaces.find((s) => s.id === session?.spaceId);
      const profileId = session?.profileId ?? space?.profileId ?? 'default';
      const profile = profileStore.getById(profileId) || profileStore.defaultProfile;
      let envVars: Record<string, string> | null = null;
      if (profile && !profile.isDefault) {
        envVars = { ...profile.envVars };
        if (profile.configDir) {
          envVars['CLAUDE_CONFIG_DIR'] = profile.configDir;
        }
      }

      // Merge extra env vars from session (e.g. MCP socket path for pipeline stages)
      if (session?.extraEnvVars) {
        envVars = { ...(envVars || {}), ...session.extraEnvVars };
      }

      // Always set WEPLEX_SESSION_ID so the MCP server can save session summaries
      envVars = { ...(envVars || {}), WEPLEX_SESSION_ID: String(sessionId) };

      // Inject Weplex context into CLAUDE.md before starting Claude sessions
      if (isClaude && session) {
        await contextInjectionStore.inject(session);
      }

      await invoke('create_pty', {
        sessionId,
        cols: term.cols > 2 ? term.cols : 80,
        rows: term.rows > 2 ? term.rows : 24,
        command,
        cwd: session?.cwd || null,
        envVars,
      });

      // Agent sessions start as 'idle' (green, ready for first prompt)
      // Terminal sessions start as 'active' (shell is booting up)
      sessionStore.updateStatus(sessionId, isAgentSession ? 'idle' : 'active');

      const { listen } = await import('@tauri-apps/api/event');
      const claudeResumeRe =
        /claude\s+--resume\s+([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})/;

      function resetStatusTimers() {
        if (waitingTimer) {
          clearTimeout(waitingTimer);
          waitingTimer = null;
        }
        if (idleTimer) {
          clearTimeout(idleTimer);
          idleTimer = null;
        }
        if (statePollTimer) {
          clearTimeout(statePollTimer);
          statePollTimer = null;
        }
      }

      // Patterns that signal the agent needs user action right now (menus, permissions)
      // Patterns that mean Claude is waiting for user action right now.
      // NOTE: case-sensitive (no 'i' flag) — "esc to interrupt" (lowercase) appears during
      // active work and must NOT match. "Esc to cancel" (capital E) is the menu indicator.
      // PTY strips spaces between words, so \s* handles both "Enter to select" and "Entertoselect".
      const agentNeedsInputRe =
        /Enter\s*to\s*select|↑.{0,3}↓|Tab\s*to\s*amend|Do you want to proceed|Esc\s*to\s*cancel|ctrl\+g.*edit|\[y\/n\]|\[Y\/n\]|\[yes\/no\]|\(y\)es\/\(n\)o|\? $/m;

      // For Claude: poll JSONL to detect when Claude finishes its turn (reliable).
      // For other agents: fall back to timeout.
      function scheduleStatusTransition() {
        resetStatusTimers();
        if (isClaude && session?.cwd && session?.claudeSessionId) {
          // Poll JSONL every 2s; stop when last role = "assistant" (Claude done)
          const pollState = async () => {
            if (destroyed) return;
            // Read claudeSessionId fresh from store — it may arrive after polling starts
            const claudeId = sessionStore.sessions.find((s) => s.id === sessionId)?.claudeSessionId;
            if (!claudeId) {
              statePollTimer = setTimeout(pollState, 2000);
              return;
            }
            try {
              const state = (await invoke('get_claude_state', {
                cwd: session!.cwd,
                sessionId: claudeId,
              })) as string | null;
              if (state === 'idle') {
                // Guard against race: JSONL may not yet reflect the user's latest input
                if (Date.now() - lastUserInputAt < 3000) {
                  statePollTimer = setTimeout(pollState, 2000);
                  return;
                }
                sessionStore.updateStatus(sessionId, 'idle');
                return; // done polling
              }
            } catch {
              /* ignore, retry */
            }
            statePollTimer = setTimeout(pollState, 2000);
          };
          statePollTimer = setTimeout(pollState, 2000);
        } else {
          // Non-Claude agents: timeout fallback
          waitingTimer = setTimeout(
            () => {
              if (!destroyed) sessionStore.updateStatus(sessionId, 'idle');
            },
            isAgentSession ? 4000 : 15000,
          );
        }
      }

      // Expose to the onData handler (runs in onMount scope, outside connectPty)
      scheduleTransition = scheduleStatusTransition;

      let outputMarked = false;
      let cursorRestored = false;
      let claudeIdCaptured = !!session?.claudeSessionId;
      // Rolling buffer for pattern detection across split chunks (agent sessions only)
      let outputTail = '';
      const TAIL_MAX = 500;

      const unlistenFn = await listen<string>(`pty-output-${sessionId}`, (event) => {
        // Decode base64 payload → raw bytes, then write directly to xterm.js.
        // xterm.js receives a Uint8Array and runs its own stateful UTF-8 decoder,
        // which correctly handles multibyte sequences split across PTY reads and
        // natively respects Synchronized Output Mode (\x1b[?2026h/l). This
        // mirrors exactly how VS Code feeds PTY data to xterm.js.
        const bytes = b64ToBytes(event.payload);

        // Restore cursor color on first output
        if (!cursorRestored) {
          cursorRestored = true;
          term.options.theme = { ...term.options.theme, cursor: '#8b5cf6' };
        }
        term.write(bytes);

        // Decode to string only for pattern/session-ID matching.
        const text = textDecoder.decode(bytes, { stream: true });

        // Terminal: any output = active. Agent: only user input sets active (see onData).
        if (!isAgentSession) {
          sessionStore.updateStatus(sessionId, 'active');
          scheduleStatusTransition();
        } else {
          // Agent: detect when it needs user action (question / menu / permission).
          // Use a rolling buffer so patterns split across chunks are still caught.
          const stripped = text.replace(/\x1b\[[0-9;]*[a-zA-Z]/g, '');
          outputTail = (outputTail + stripped).slice(-TAIL_MAX);
          if (agentNeedsInputRe.test(outputTail)) {
            outputTail = ''; // consume — don't re-trigger on next chunk
            resetStatusTimers();
            sessionStore.updateStatus(sessionId, 'waiting');
          }
        }

        // Mark session as having received output (for smart restore)
        if (!outputMarked) {
          outputMarked = true;
          sessionStore.update(sessionId, { hasOutput: true });
        }

        // Capture Claude session ID from "claude --resume <uuid>" output
        if (!claudeIdCaptured && isClaude) {
          const match = text.match(claudeResumeRe);
          if (match) {
            claudeIdCaptured = true;
            sessionStore.update(sessionId, { claudeSessionId: match[1] });
            startUsagePolling(invoke, session!.cwd!, match[1]);
          }
        }
      });
      unlisten = unlistenFn;

      ptyWriter = async (data: string) => {
        await invoke('write_pty', { sessionId, data });
      };

      // Detect the NEW Claude session ID by file creation time
      // Claude takes 10-40s to create its session file, so we poll with retries
      if (isClaude && !claudeIdCaptured) {
        const pollForSessionId = async (attempt: number) => {
          if (destroyed || claudeIdCaptured || attempt > 12) return;
          try {
            // Collect IDs already claimed by other sessions to avoid collisions
            const excludeIds = sessionStore.sessions
              .filter((s) => s.id !== sessionId && s.claudeSessionId)
              .map((s) => s.claudeSessionId!);
            const newId = await invoke<string | null>('get_new_claude_session', {
              cwd: session!.cwd,
              afterEpochMs: launchTime,
              excludeIds,
            });
            if (newId) {
              claudeIdCaptured = true;
              sessionStore.update(sessionId, { claudeSessionId: newId });
              startUsagePolling(invoke, session!.cwd!, newId);
              return;
            }
          } catch {
            /* retry on next attempt */
          }
          const t = setTimeout(() => pollForSessionId(attempt + 1), 5000);
          pendingTimers.push(t);
        };
        const t = setTimeout(() => pollForSessionId(0), 3000);
        pendingTimers.push(t);
      }

      // If session already has a Claude session ID (restored), start polling immediately
      if (isClaude && claudeIdCaptured && session!.claudeSessionId) {
        startUsagePolling(invoke, session!.cwd!, session!.claudeSessionId);
      }

      disposables.push(
        term.onResize(async ({ cols, rows }) => {
          try {
            await invoke('resize_pty', { sessionId, cols, rows });
          } catch {
            /* session may be dead */
          }
        }),
      );

      // Re-fit if container is visible (portal may not have moved us yet)
      if (!destroyed && fitAddon && containerEl.offsetWidth > 0) {
        fitAddon.fit();
      }

      // Pipeline injection: if a pipeline was launched, send instructions after Claude is ready.
      // Poll terminal buffer for prompt indicator instead of using a fixed delay.
      const pipelineInstructions = pipelineInjectStore.consume(sessionId);
      if (pipelineInstructions) {
        let injected = false;
        const checkReady = () => {
          if (destroyed || injected || !ptyWriter) return;
          const buf = term.buffer.active;
          for (let i = Math.max(0, buf.cursorY - 5); i <= buf.cursorY; i++) {
            const line = buf.getLine(i)?.translateToString(true) || '';
            if (line.includes('❯') || line.includes('>') || line.includes('$')) {
              injected = true;
              ptyWriter(pipelineInstructions + '\n');
              return;
            }
          }
        };
        let attempts = 0;
        const pollTimer = setInterval(() => {
          attempts++;
          checkReady();
          if (injected || attempts >= 30) clearInterval(pollTimer);
        }, 500);
        pendingTimers.push(pollTimer as unknown as ReturnType<typeof setTimeout>);
      }
    } catch (err) {
      console.error('PTY connect failed:', err);
      sessionStore.updateStatus(sessionId, 'error');
      term.writeln('\x1b[31m  Failed to connect to PTY backend.\x1b[0m');
      term.writeln(`\x1b[90m  ${err}\x1b[0m`);
    }
  }
</script>

<div
  bind:this={outerEl}
  class="terminal-outer"
  class:drag-over={isDragOver}
  ondragover={(e) => e.preventDefault()}
  ondrop={(e) => e.preventDefault()}
>
  <div class="terminal-inner" bind:this={containerEl}></div>
</div>

<style>
  .terminal-outer {
    width: 100%;
    height: 100%;
    padding: 8px 12px;
    background: var(--weplex-bg);
  }

  .terminal-inner {
    width: 100%;
    height: 100%;
    line-height: normal;
  }

  .terminal-inner :global(.xterm) {
    height: 100%;
  }

  .terminal-inner :global(.xterm .xterm-viewport),
  .terminal-inner :global(.xterm .xterm-scrollable-element) {
    background-color: transparent !important;
  }

  .terminal-outer.drag-over {
    outline: 2px solid var(--weplex-accent);
    outline-offset: -2px;
    border-radius: 4px;
  }
</style>

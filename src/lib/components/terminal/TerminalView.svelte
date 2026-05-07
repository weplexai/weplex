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
  import { contextInjectionStore } from '../../stores/contextInjectionStore.svelte';
  import { wsService } from '../../services/wsService';
  import { notifyWaitingForInput, notifyError, trackSessionActivity, redactSecrets } from '../../services/notificationService';
  import { terminalRegistry } from '../../stores/terminalRegistry';
  import { commandStore } from '../../stores/commandStore.svelte';
  import { resolveProfileEnvId } from '../../utils/profile';
  import { schedule as scheduleCompile, cancel as cancelCompile } from '../../utils/compileScheduler';

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
  let unlistenHook: (() => void) | null = null;

  // Spectating state (owner side)
  let isSharedSession = false;
  let spectateActive = false;
  let spectateSpaceId = '';
  let spectateName = '';
  const textDecoder4relay = new TextDecoder();
  let disposables: { dispose(): void }[] = [];
  let pendingTimers: ReturnType<typeof setTimeout>[] = [];
  let destroyed = false;
  let waitingTimer: ReturnType<typeof setTimeout> | null = null;
  let idleTimer: ReturnType<typeof setTimeout> | null = null;
  let statePollTimer: ReturnType<typeof setTimeout> | null = null;
  let isDragOver = $state(false);
  // Set in connectPty once session type is known
  let isAgentMode = false;
  // Profile dir we scheduled a cross-agent compile for; cancelled in onDestroy.
  let scheduledCompileFor: string | null = null;

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

  // True once create_pty has been invoked for this session in this component
  // lifetime. Set BEFORE the async invoke so concurrent reactive triggers can't
  // re-enter. On invoke failure the catch resets it so the next wake-trigger
  // can retry — without that the session is permanently stranded by a transient
  // error (e.g. cwd vanished, binary missing).
  let ptySpawned = false;

  // Strict UUID v4 shape — claudeSessionId is concatenated into the shell stdin
  // (`claude --resume <id>`), and the persisted weplex_sessions.json is
  // user-writable. A tampered file with shell metacharacters would otherwise
  // execute as the user. Same regex as the live capture at claudeResumeRe.
  const CLAUDE_UUID_RE =
    /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

  onMount(() => {
    const settings = settingsStore.settings;

    const isLight = settings.theme === 'light';
    const darkTermTheme = {
      background: '#0a0a0f',
      foreground: '#e8e8ed',
      cursor: 'transparent',
      cursorAccent: '#0a0a0f',
      selectionBackground: 'rgba(252, 94, 68, 0.25)',
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
    };
    const lightTermTheme = {
      background: '#fafafa',
      foreground: '#1a1a1f',
      cursor: 'transparent',
      cursorAccent: '#fafafa',
      selectionBackground: 'rgba(224, 72, 48, 0.2)',
      selectionForeground: '#1a1a1f',
      black: '#1a1a1f',
      red: '#dc2626',
      green: '#059669',
      yellow: '#d97706',
      blue: '#2563eb',
      magenta: '#7c3aed',
      cyan: '#0891b2',
      white: '#f5f5f5',
      brightBlack: '#9898a8',
      brightRed: '#ef4444',
      brightGreen: '#10b981',
      brightYellow: '#f59e0b',
      brightBlue: '#3b82f6',
      brightMagenta: '#8b5cf6',
      brightCyan: '#06b6d4',
      brightWhite: '#ffffff',
    };

    term = new Terminal({
      cursorBlink: true,
      cursorStyle: 'bar',
      scrollback: 5000,
      fontSize: settings.fontSize,
      fontFamily: settings.fontFamily,
      theme: isLight ? lightTermTheme : darkTermTheme,
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
        // ── Slash command interception (agent sessions only) ──
        // During slash mode: characters are echoed locally to xterm (not sent to PTY).
        if (ptyWriter) ptyWriter(data);
        // Agent: user input → set "thinking" (agent will process)
        // When PTY output arrives, status changes to "active"
        if (isAgentMode && scheduleTransition) {
          lastUserInputAt = Date.now();
          // Enter key or multi-char paste = likely a prompt submission
          if (data.includes('\r') || data.includes('\n') || data.length > 5) {
            // Skip terminal escape responses (e.g. DA response \x1b[?1;2c)
            const isEscapeResponse = data.includes('\x1b[') || /^\[\?[0-9;]*[\x40-\x7e]/.test(data);
            if (!isEscapeResponse) {
              sessionStore.updateStatus(sessionId, 'thinking');
            }
          }
          scheduleTransition();
        }
      }),
    );

    // PTY spawn is gated on wake — see the effect below. We wake immediately
    // for sessions that aren't restored from disk (i.e. created in this
    // session: `sessionStore.create()` doesn't add them to hibernated).
    // Restored sessions stay hibernated until activated or boot-time wake-up.

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
    // Allow SplitContainer to force a redraw after reparenting — xterm Canvas
    // does not auto-redraw on detach/reattach when slot size is unchanged.
    terminalRegistry.setRefresh(sessionId, () => {
      if (!term || !fitAddon || !containerEl) return;
      if (containerEl.offsetWidth === 0 || containerEl.offsetHeight === 0) return;
      try {
        fitAddon.fit();
      } catch { /* fit can throw if proposeDimensions returns null mid-layout */ }
      term.refresh(0, term.rows - 1);
    });
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

  // Lazy PTY spawn. Restored sessions sit in `hibernatedSessionIds` until they
  // are activated (sessionStore.activate) or woken at boot for the active
  // space's split layout (App.svelte). Newly-created sessions are not added to
  // the set so they spawn immediately. The ptySpawned latch makes this fire
  // exactly once per component instance.
  $effect(() => {
    if (ptySpawned) return;
    if (!term) return; // wait for onMount to set up xterm
    if (sessionStore.isHibernated(sessionId)) return;
    ptySpawned = true;
    connectPty();
  });

  // Reactively update terminal font and theme when settings change
  $effect(() => {
    const { fontSize, fontFamily, theme } = settingsStore.settings;
    if (term) {
      term.options.fontSize = fontSize;
      term.options.fontFamily = fontFamily;
      // Update terminal theme on the fly
      const isLightNow = theme === 'light';
      term.options.theme = isLightNow
        ? {
            background: '#fafafa', foreground: '#1a1a1f', cursor: '#e04830',
            cursorAccent: '#fafafa', selectionBackground: 'rgba(224, 72, 48, 0.2)',
            black: '#1a1a1f', red: '#dc2626', green: '#059669', yellow: '#d97706',
            blue: '#2563eb', magenta: '#7c3aed', cyan: '#0891b2', white: '#f5f5f5',
            brightBlack: '#9898a8', brightRed: '#ef4444', brightGreen: '#10b981',
            brightYellow: '#f59e0b', brightBlue: '#3b82f6', brightMagenta: '#8b5cf6',
            brightCyan: '#06b6d4', brightWhite: '#ffffff',
          }
        : {
            background: '#0a0a0f', foreground: '#e8e8ed', cursor: '#fc5e44',
            cursorAccent: '#0a0a0f', selectionBackground: 'rgba(252, 94, 68, 0.25)',
            black: '#1a1a25', red: '#ef4444', green: '#10b981', yellow: '#f59e0b',
            blue: '#3b82f6', magenta: '#8b5cf6', cyan: '#06b6d4', white: '#e8e8ed',
            brightBlack: '#6b6b80', brightRed: '#f87171', brightGreen: '#34d399',
            brightYellow: '#fbbf24', brightBlue: '#60a5fa', brightMagenta: '#a78bfa',
            brightCyan: '#22d3ee', brightWhite: '#f9fafb',
          };
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
    unlistenHook?.();
    if (spectateActive) {
      wsService.spectateStop(spectateSpaceId, spectateName);
      spectateActive = false;
    }
    // Unregister BEFORE dispose so any in-flight refresh callbacks (e.g. rAF
    // scheduled by SplitContainer during reparent) cannot reach a disposed term.
    terminalRegistry.unregister(sessionId);
    term?.dispose();
    if (scheduledCompileFor) {
      cancelCompile(scheduledCompileFor);
      scheduledCompileFor = null;
    }
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

  /** Fetch git branch and status for a session's cwd, update session store. */
  async function fetchGitInfo(sid: number, cwd: string) {
    if (destroyed) return;
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const [branch, gitFiles] = await Promise.all([
        invoke<string | null>('get_git_branch', { cwd }),
        invoke<{ path: string; status: string }[]>('get_git_status', { cwd }),
      ]);
      if (destroyed) return;
      // Always update — clear stale data when git info is empty
      sessionStore.update(sid, {
        branch: branch ?? undefined,
        gitFiles: gitFiles && gitFiles.length > 0 ? gitFiles : undefined,
      });
    } catch {
      // Git not available or not a repo — silently skip
    }
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
          CLAUDE_UUID_RE.test(session.claudeSessionId) &&
          command.includes('claude') &&
          !command.includes('--resume') &&
          !command.includes('--continue')
        ) {
          command = command + ' --resume ' + session.claudeSessionId;
        } else if (session?.claudeSessionId && !CLAUDE_UUID_RE.test(session.claudeSessionId)) {
          console.warn(
            `[Weplex] Refusing to resume session ${sessionId}: claudeSessionId is not a valid UUID`,
          );
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

      // Merge extra env vars from session
      if (session?.extraEnvVars) {
        envVars = { ...(envVars || {}), ...session.extraEnvVars };
      }

      // Always set WEPLEX_SESSION_ID so the MCP server can save session summaries
      envVars = { ...(envVars || {}), WEPLEX_SESSION_ID: String(sessionId) };

      // WEPLEX_PROFILE_ID picks the per-profile Keychain key used to encrypt
      // notes. Single source of truth in utils/profile.ts so writer (here)
      // and readers (Timeline / Hover / SpaceChat) cannot drift apart.
      if (session) {
        envVars = { ...envVars, WEPLEX_PROFILE_ID: resolveProfileEnvId(session) };
      }

      // Inject Weplex context into CLAUDE.md before starting Claude sessions
      if (isClaude && session) {
        await contextInjectionStore.inject(session);
      }

      // Load commands for slash autocomplete
      if (isAgentMode && session?.cwd) {
        commandStore.load(session.cwd);
      }

      // Cross-agent compile + scan: lazy, per non-Claude session start.
      // Skipped for Claude (reads bodies natively) and the default profile
      // (no configDir → no manifests). Failures fall through — session
      // start is never gated. Toast surfaces in HubResources.
      if (
        isAgentSession &&
        session?.agentType !== 'claude' &&
        profile?.configDir
      ) {
        try {
          scheduledCompileFor = profile.configDir;
          // Fire-and-forget: don't block PTY spawn on the debounced compile.
          scheduleCompile(profile.configDir, {
            projectRoot: session?.cwd ?? null,
            deepScan: settingsStore.settings.agentshieldDeepScan,
          });
        } catch (e) {
          console.warn('[weplex] cross-agent compile failed (non-fatal):', e);
        }
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

      // Start spectating offer for shared/team sessions
      if (session && space && (space.shared || space.type === 'team') && space.serverId) {
        isSharedSession = true;
        spectateSpaceId = space.serverId;
        spectateName = session.name;
        spectateActive = true;
        wsService.spectateOffer(spectateSpaceId, spectateName);

        // Listen for spectator count updates — store on session for UI display
        const unsubCount = wsService.onSpectateCount((data) => {
          if (data.spaceId === spectateSpaceId && data.sessionName === spectateName) {
            sessionStore.update(sessionId, { spectatorCount: data.count });
          }
        });
        // Add cleanup via disposables (survives git hook setup that reassigns unlistenHook)
        disposables.push({ dispose: unsubCount });
      }

      // Fetch git info for the session's working directory
      if (session?.cwd) {
        fetchGitInfo(sessionId, session.cwd);

        // Clean up previous hook listener on reconnection
        unlistenHook?.();
        unlistenHook = null;

        // Listen for hook events to refresh git status after file modifications
        const sessionCwd = session.cwd;
        const { listen: listenEvent } = await import('@tauri-apps/api/event');
        unlistenHook = (await listenEvent<{ event_type: string; session_id: number; tool_name?: string }>('hook-event', (evt) => {
          if (evt.payload.session_id !== sessionId) return;
          if (evt.payload.event_type !== 'post_tool_use') return;
          const tool = evt.payload.tool_name;
          if (!tool || !['Write', 'Edit', 'MultiEdit', 'Bash'].includes(tool)) return;
          // Debounce: refresh git status 2s after last file-modifying tool use
          const t = setTimeout(() => fetchGitInfo(sessionId, sessionCwd), 2000);
          pendingTimers.push(t);
        })) as unknown as () => void;
      }

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

      // Agent error patterns — only truly fatal errors, not normal tool output
      const agentErrorRe =
        /panic:|Traceback \(most recent|ENOENT.*no such file|EACCES.*permission denied|Process exited with code [1-9]|API error.*status [45]\d\d|quota exceeded|rate limit exceeded/im;

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
          term.options.theme = { ...term.options.theme, cursor: '#fc5e44' };
        }
        term.write(bytes);

        // Stream to relay for spectators (shared/team sessions only)
        if (isSharedSession && spectateActive) {
          const text4relay = textDecoder4relay.decode(bytes, { stream: true });
          wsService.sendPtyStream(spectateSpaceId, spectateName, text4relay);
        }

        // Decode to string only for pattern/session-ID matching.
        const text = textDecoder.decode(bytes, { stream: true });

        // Track activity for stuck detection
        trackSessionActivity(sessionId);

        // Terminal: any output = active. Agent: thinking → active on first output.
        if (!isAgentSession) {
          sessionStore.updateStatus(sessionId, 'active');
          scheduleStatusTransition();
        } else {
          // Agent output received — if was thinking, now active
          const currentStatus = sessionStore.sessions.find((s) => s.id === sessionId)?.status;
          if (currentStatus === 'thinking') {
            sessionStore.updateStatus(sessionId, 'active');
          }
          // Agent: detect when it needs user action (question / menu / permission).
          // Use a rolling buffer so patterns split across chunks are still caught.
          const stripped = text.replace(/\x1b\[[0-9;]*[a-zA-Z]/g, '');
          outputTail = (outputTail + stripped).slice(-TAIL_MAX);
          if (agentNeedsInputRe.test(outputTail)) {
            outputTail = ''; // consume — don't re-trigger on next chunk
            resetStatusTimers();
            sessionStore.updateStatus(sessionId, 'waiting');
            notifyWaitingForInput(session?.name || `session-${sessionId}`);
          }

          // Detect agent errors in output
          const errorMatch = outputTail.match(agentErrorRe);
          if (errorMatch) {
            const errorMsg = redactSecrets(errorMatch[0].trim().slice(0, 80));
            outputTail = ''; // consume — don't re-trigger
            sessionStore.update(sessionId, { status: 'error', lastError: errorMsg, lastActivity: Date.now() });
            notifyError(session?.name || `session-${sessionId}`, errorMsg);
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

      // Claude session ID is captured via SessionStart hook (reliable, no polling).
      // Start usage polling when claudeSessionId becomes available (may arrive later via hook).
      if (isClaude) {
        let usagePollingStarted = !!session!.claudeSessionId;
        if (usagePollingStarted) {
          startUsagePolling(invoke, session!.cwd!, session!.claudeSessionId!);
        }
        // Watch for claudeSessionId arriving via SessionStart hook
        const checkInterval = setInterval(() => {
          if (destroyed) { clearInterval(checkInterval); return; }
          const s = sessionStore.sessions.find((s) => s.id === sessionId);
          if (!usagePollingStarted && s?.claudeSessionId) {
            usagePollingStarted = true;
            claudeIdCaptured = true;
            startUsagePolling(invoke, session!.cwd!, s.claudeSessionId);
            clearInterval(checkInterval);
          }
        }, 1000);
        pendingTimers.push(checkInterval as unknown as ReturnType<typeof setTimeout>);
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

    } catch (err) {
      console.error('PTY connect failed:', err);
      sessionStore.updateStatus(sessionId, 'error');
      term.writeln('\x1b[31m  Failed to connect to PTY backend.\x1b[0m');
      term.writeln(`\x1b[90m  ${err}\x1b[0m`);
      // Recovery: drop the spawn latch AND re-hibernate so the gating effect
      // doesn't immediately loop on the same failure. Clicking the session
      // (sessionStore.activate → wakeUp) will retry exactly once.
      ptySpawned = false;
      sessionStore.rehibernate(sessionId);
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

/**
 * Command Store — reads Claude commands from .claude/commands/*.md files on disk.
 * Weplex-specific metadata (icon, color, adapters) stored in ~/.weplex/command-meta.json.
 *
 * For Claude sessions: sends "/command-name" to PTY (Claude resolves natively).
 * For other agents: resolves adapter text and sends to PTY.
 */

import { invoke } from '@tauri-apps/api/core';
import type { Session } from '../types';

/** Strip control characters that could be dangerous in PTY.
 *  Includes ESC (0x1B) to prevent terminal escape sequence injection. */
function sanitizePtyText(text: string): string {
  return text.replace(/[\x00-\x08\x0B-\x0C\x0E-\x1F\x1B]/g, '');
}

const VALID_ICON_COLORS = new Set([
  'accent', 'warning', 'active', 'error', 'info', 'pink',
  'model-opus', 'model-sonnet', 'model-haiku',
  'text', 'text-secondary', 'text-muted',
]);

/** Discriminator for command vs pipeline. Mirrors Rust enum value. */
export type CommandType = 'command' | 'pipeline';

/** Raw command file from Rust backend. Rust struct uses
 *  `#[serde(rename_all = "camelCase")]` so JSON keys are camelCase. */
interface CommandFile {
  name: string;
  filePath: string;
  scope: string;
  description: string;
  argumentHint: string;
  allowedTools: string[];
  model: string;
  body: string;
  /** "command" (default) or "pipeline". Backend defaults missing values to "command". */
  commandType: CommandType;
}

/** Weplex display metadata overlay. */
interface CommandMeta {
  icon?: string;
  iconColor?: string;
  showInPanel?: boolean;
  adapters?: Record<string, string>;
}

/** Merged command for UI display. */
export interface Command {
  name: string;
  filePath: string;
  scope: 'user' | 'project';
  description: string;
  argumentHint: string;
  allowedTools: string[];
  model: string;
  body: string;
  commandType: CommandType;
  // Weplex display
  icon: string;
  iconColor: string;
  showInPanel: boolean;
  adapters: Record<string, string>;
}

const META_STORE_KEY = 'weplex_command_meta';

function isValidMeta(v: unknown): v is CommandMeta {
  if (!v || typeof v !== 'object') return false;
  const m = v as Record<string, unknown>;
  if (m.icon !== undefined && typeof m.icon !== 'string') return false;
  if (m.iconColor !== undefined && typeof m.iconColor !== 'string') return false;
  if (m.showInPanel !== undefined && typeof m.showInPanel !== 'boolean') return false;
  if (m.adapters !== undefined && (typeof m.adapters !== 'object' || m.adapters === null)) return false;
  return true;
}

function loadMeta(): Record<string, CommandMeta> {
  try {
    const raw = localStorage.getItem(META_STORE_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw);
    if (!parsed || typeof parsed !== 'object') return {};
    // Validate each entry
    const result: Record<string, CommandMeta> = {};
    for (const [key, val] of Object.entries(parsed)) {
      if (typeof key === 'string' && isValidMeta(val)) {
        result[key] = val;
      }
    }
    return result;
  } catch {
    return {};
  }
}

function saveMeta(meta: Record<string, CommandMeta>) {
  localStorage.setItem(META_STORE_KEY, JSON.stringify(meta));
}

/** Coerce an unknown command_type value to the CommandType union.
 *  Backend whitelist already enforces this, but we defensively normalize
 *  for forward-compat. */
function normalizeCommandType(v: unknown): CommandType {
  return v === 'pipeline' ? 'pipeline' : 'command';
}

/** Merge raw command file with Weplex display metadata. */
function mergeCommand(file: CommandFile, meta: CommandMeta | undefined): Command {
  return {
    name: file.name,
    filePath: file.filePath,
    scope: file.scope as 'user' | 'project',
    description: file.description,
    argumentHint: file.argumentHint,
    allowedTools: file.allowedTools,
    model: file.model,
    body: file.body,
    commandType: normalizeCommandType(file.commandType),
    icon: meta?.icon || file.name.charAt(0).toUpperCase(),
    iconColor: meta?.iconColor || 'text-muted',
    showInPanel: meta?.showInPanel ?? true,
    adapters: meta?.adapters || {},
  };
}

/** Argument shape for save(). Single object so callers don't have to track
 *  positional ordering, and adding fields later is non-breaking. */
export interface SaveCommandArgs {
  name: string;
  scope: 'user' | 'project';
  cwd: string | undefined;
  description: string;
  argumentHint: string;
  allowedTools: string[];
  model: string;
  body: string;
  commandType: CommandType;
  meta: CommandMeta;
}

class CommandStore {
  commands = $state<Command[]>([]);
  loading = $state(false);
  private meta: Record<string, CommandMeta> = loadMeta();
  private defaultsEnsured = false;
  private lastCwd?: string;

  constructor() {
    // Reload commands when window regains focus (user may have edited .md files externally)
    if (typeof window !== 'undefined') {
      window.addEventListener('focus', () => {
        if (this.lastCwd !== undefined) this.load(this.lastCwd);
      });
    }
  }

  /** Load commands from disk. Call on init and after changes. */
  async load(cwd?: string): Promise<void> {
    this.lastCwd = cwd;
    this.loading = true;
    try {
      // Ensure default command files exist on first load
      if (!this.defaultsEnsured) {
        await invoke('ensure_default_commands');
        this.defaultsEnsured = true;
      }
      const files = await invoke<CommandFile[]>('list_commands', { cwd: cwd || null });
      this.commands = files.map((f) => mergeCommand(f, this.meta[f.name]));
    } catch (e) {
      console.error('[weplex] Failed to load commands:', e);
    } finally {
      this.loading = false;
    }
  }

  /** Validate and return safe iconColor CSS variable suffix. */
  safeIconColor(cmd: Command): string {
    return VALID_ICON_COLORS.has(cmd.iconColor) ? cmd.iconColor : 'text-muted';
  }

  /** User-scope, regular commands only (excludes pipelines). */
  get userCommands(): Command[] {
    return this.commands.filter((c) => c.scope === 'user' && c.commandType === 'command');
  }

  /** Project-scope, regular commands only (excludes pipelines). */
  get projectCommands(): Command[] {
    return this.commands.filter((c) => c.scope === 'project' && c.commandType === 'command');
  }

  /** User-scope pipelines. */
  get userPipelines(): Command[] {
    return this.commands.filter((c) => c.scope === 'user' && c.commandType === 'pipeline');
  }

  /** Project-scope pipelines. */
  get projectPipelines(): Command[] {
    return this.commands.filter((c) => c.scope === 'project' && c.commandType === 'pipeline');
  }

  getByName(name: string, scope?: 'user' | 'project'): Command | undefined {
    if (scope) return this.commands.find((c) => c.name === name && c.scope === scope);
    // Project commands take priority over user commands (same as Claude resolution)
    return this.commands.find((c) => c.name === name && c.scope === 'project')
      || this.commands.find((c) => c.name === name && c.scope === 'user');
  }

  /** Get commands visible in DetailPanel, split by scope and type. */
  getPanelCommands(): {
    userCommands: Command[];
    userPipelines: Command[];
    projectCommands: Command[];
    projectPipelines: Command[];
  } {
    return {
      userCommands: this.commands.filter(
        (c) => c.scope === 'user' && c.commandType === 'command' && c.showInPanel,
      ),
      userPipelines: this.commands.filter(
        (c) => c.scope === 'user' && c.commandType === 'pipeline' && c.showInPanel,
      ),
      projectCommands: this.commands.filter(
        (c) => c.scope === 'project' && c.commandType === 'command' && c.showInPanel,
      ),
      projectPipelines: this.commands.filter(
        (c) => c.scope === 'project' && c.commandType === 'pipeline' && c.showInPanel,
      ),
    };
  }

  /** Save a command to disk (creates/updates .md file). */
  async save(args: SaveCommandArgs): Promise<string | null> {
    try {
      await invoke('save_command', {
        name: args.name,
        scope: args.scope,
        cwd: args.cwd || null,
        description: args.description,
        argumentHint: args.argumentHint,
        allowedTools: args.allowedTools,
        model: args.model,
        body: args.body,
        commandType: args.commandType,
      });
      // Save Weplex metadata
      this.meta[args.name] = args.meta;
      saveMeta(this.meta);
      await this.load(args.cwd);
      return null;
    } catch (e) {
      return e instanceof Error ? e.message : String(e);
    }
  }

  /** Delete a command file from disk and clean up metadata. */
  async remove(name: string, filePath: string, cwd?: string): Promise<string | null> {
    try {
      await invoke('delete_command', { path: filePath });
      // Clean up orphaned metadata
      delete this.meta[name];
      saveMeta(this.meta);
      await this.load(cwd);
      return null;
    } catch (e) {
      return e instanceof Error ? e.message : String(e);
    }
  }

  /** Update Weplex display metadata for a command. */
  updateMeta(name: string, meta: Partial<CommandMeta>): void {
    this.meta[name] = { ...this.meta[name], ...meta };
    saveMeta(this.meta);
    // Update in-memory
    this.commands = this.commands.map((c) =>
      c.name === name ? mergeCommand(
        {
          name: c.name,
          filePath: c.filePath,
          scope: c.scope,
          description: c.description,
          argumentHint: c.argumentHint,
          allowedTools: c.allowedTools,
          model: c.model,
          body: c.body,
          commandType: c.commandType,
        },
        this.meta[name],
      ) : c,
    );
  }

  /**
   * Resolve what to send to PTY for a command execution.
   * Claude: sends "/name" (Claude resolves natively).
   * Others: sends adapter text or command body.
   */
  resolveForPty(cmd: Command, session: Session): string {
    if (session.agentType === 'claude') {
      return `/${cmd.name}`;
    }
    // Non-Claude: check adapters, fall back to body
    const agentType = session.agentType || 'unknown';
    const raw = cmd.adapters[agentType] || cmd.adapters.default || cmd.body;
    return sanitizePtyText(raw);
  }

  /** Get all commands for slash autocomplete. */
  getSlashCommands(): Command[] {
    return [...this.commands].sort((a, b) => a.name.localeCompare(b.name));
  }

  /** Resolve a slash input (e.g. "/review") for PTY. */
  resolveSlash(input: string, session: Session): string | null {
    const name = input.replace(/^\/\//, '').trim();
    const cmd = this.commands.find((c) => c.name === name);
    if (!cmd) return null;
    return this.resolveForPty(cmd, session);
  }
}

export const commandStore = new CommandStore();

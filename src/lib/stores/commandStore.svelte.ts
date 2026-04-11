/**
 * Command Store — reads Claude commands from .claude/commands/*.md files on disk.
 * Weplex-specific metadata (icon, color, adapters) stored in ~/.weplex/command-meta.json.
 *
 * For Claude sessions: sends "/command-name" to PTY (Claude resolves natively).
 * For other agents: resolves adapter text and sends to PTY.
 */

import { invoke } from '@tauri-apps/api/core';
import type { Session } from '../types';

/** Strip control characters that could be dangerous in PTY. */
function sanitizePtyText(text: string): string {
  return text.replace(/[\x00-\x08\x0B-\x0C\x0E-\x1F]/g, '');
}

const VALID_ICON_COLORS = new Set([
  'accent', 'warning', 'active', 'error', 'info', 'pink',
  'model-opus', 'model-sonnet', 'model-haiku',
  'text', 'text-secondary', 'text-muted',
]);

/** Raw command file from Rust backend. */
interface CommandFile {
  name: string;
  file_path: string;
  scope: string;
  description: string;
  argument_hint: string;
  allowed_tools: string[];
  model: string;
  body: string;
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
  // Weplex display
  icon: string;
  iconColor: string;
  showInPanel: boolean;
  adapters: Record<string, string>;
}

const META_STORE_KEY = 'weplex_command_meta';

function loadMeta(): Record<string, CommandMeta> {
  try {
    const raw = localStorage.getItem(META_STORE_KEY);
    return raw ? JSON.parse(raw) : {};
  } catch {
    return {};
  }
}

function saveMeta(meta: Record<string, CommandMeta>) {
  localStorage.setItem(META_STORE_KEY, JSON.stringify(meta));
}

/** Merge raw command file with Weplex display metadata. */
function mergeCommand(file: CommandFile, meta: CommandMeta | undefined): Command {
  return {
    name: file.name,
    filePath: file.file_path,
    scope: file.scope as 'user' | 'project',
    description: file.description,
    argumentHint: file.argument_hint,
    allowedTools: file.allowed_tools,
    model: file.model,
    body: file.body,
    icon: meta?.icon || file.name.charAt(0).toUpperCase(),
    iconColor: meta?.iconColor || 'text-muted',
    showInPanel: meta?.showInPanel ?? true,
    adapters: meta?.adapters || {},
  };
}

class CommandStore {
  commands = $state<Command[]>([]);
  loading = $state(false);
  private meta: Record<string, CommandMeta> = loadMeta();

  /** Load commands from disk. Call on init and after changes. */
  async load(cwd?: string): Promise<void> {
    this.loading = true;
    try {
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

  get userCommands(): Command[] {
    return this.commands.filter((c) => c.scope === 'user');
  }

  get projectCommands(): Command[] {
    return this.commands.filter((c) => c.scope === 'project');
  }

  getByName(name: string, scope?: 'user' | 'project'): Command | undefined {
    if (scope) return this.commands.find((c) => c.name === name && c.scope === scope);
    // Project commands take priority over user commands (same as Claude resolution)
    return this.commands.find((c) => c.name === name && c.scope === 'project')
      || this.commands.find((c) => c.name === name && c.scope === 'user');
  }

  /** Get commands visible in DetailPanel. */
  getPanelCommands(): { user: Command[]; project: Command[] } {
    return {
      user: this.commands.filter((c) => c.scope === 'user' && c.showInPanel),
      project: this.commands.filter((c) => c.scope === 'project' && c.showInPanel),
    };
  }

  /** Save a command to disk (creates/updates .md file). */
  async save(
    name: string,
    scope: 'user' | 'project',
    cwd: string | undefined,
    description: string,
    argumentHint: string,
    allowedTools: string[],
    model: string,
    body: string,
    meta: CommandMeta,
  ): Promise<string | null> {
    try {
      await invoke('save_command', {
        name, scope, cwd: cwd || null,
        description, argumentHint: argumentHint, allowedTools, model, body,
      });
      // Save Weplex metadata
      this.meta[name] = meta;
      saveMeta(this.meta);
      await this.load(cwd);
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
        { name: c.name, file_path: c.filePath, scope: c.scope, description: c.description, argument_hint: c.argumentHint, allowed_tools: c.allowedTools, model: c.model, body: c.body },
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
    const name = input.replace(/^\//, '').trim();
    const cmd = this.commands.find((c) => c.name === name);
    if (!cmd) return null;
    return this.resolveForPty(cmd, session);
  }
}

export const commandStore = new CommandStore();

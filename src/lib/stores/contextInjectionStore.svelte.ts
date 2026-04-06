/**
 * Context Injection Store — injects Weplex workspace context into CLAUDE.md
 * before each Claude Code session starts. Gives Claude passive awareness of
 * the current space, other sessions, and cost without extra prompting.
 */

import { invoke } from '@tauri-apps/api/core';
import { sessionStore } from './sessionStore.svelte';
import { spaceStore } from './spaceStore.svelte';
import { profileStore } from './profileStore.svelte';
import type { Session } from '../types';

// ── Context block builder ────────────────────────────────────────────────────

function buildContextBlock(session: Session): string {
  const space = spaceStore.spaces.find((s) => s.id === session.spaceId);
  const spaceName = space?.name || 'Default';

  // Gather other sessions in the same space
  const otherSessions = sessionStore.sessions
    .filter((s) => s.spaceId === session.spaceId && s.id !== session.id && s.status !== 'new')
    .map((s) => {
      const status = s.status === 'active' ? 'active' : s.status === 'waiting' ? 'waiting' : 'idle';
      const cwd = s.cwd ? ` — ${s.cwd}` : '';
      const agent = s.agentType ? ` (${s.agentType})` : '';
      return `    - ${s.name}${agent} [${status}]${cwd}`;
    });

  // Profile info
  const profileId = session.profileId || space?.profileId;
  const profile = profileId ? profileStore.getById(profileId) : profileStore.defaultProfile;
  const profileName = profile?.name || 'Default';

  // Cost across all sessions in this space
  const spaceSessions = sessionStore.sessions.filter((s) => s.spaceId === session.spaceId);
  const spaceCost = spaceSessions.reduce((sum, s) => sum + (s.cost || 0), 0);

  // Build the block
  const lines: string[] = [
    '<!-- Injected by Weplex — do not edit this block -->',
    '## Weplex Workspace Context',
    `- Space: ${spaceName} | Session: ${session.name} | Profile: ${profileName}`,
  ];

  if (otherSessions.length > 0) {
    lines.push('- Active sessions in this space:');
    lines.push(...otherSessions);
  }

  if (spaceCost > 0) {
    lines.push(`- Space cost: $${spaceCost.toFixed(2)}`);
  }

  lines.push('<!-- End Weplex block -->');

  return lines.join('\n');
}

// ── Public API ───────────────────────────────────────────────────────────────

export const contextInjectionStore = {
  /**
   * Inject Weplex context into the project's CLAUDE.md before session starts.
   * Only injects for Claude agent sessions with a known cwd.
   */
  async inject(session: Session): Promise<void> {
    if (session.agentType !== 'claude' || !session.cwd) return;

    const contextBlock = buildContextBlock(session);

    try {
      await invoke('inject_claude_md', {
        cwd: session.cwd,
        contextBlock,
      });
    } catch (e) {
      console.error('[weplex] Failed to inject CLAUDE.md context:', e);
    }
  },

  /**
   * Remove Weplex injection from CLAUDE.md when session ends.
   */
  async remove(cwd: string): Promise<void> {
    try {
      await invoke('remove_claude_md_injection', { cwd });
    } catch (e) {
      console.error('[weplex] Failed to remove CLAUDE.md injection:', e);
    }
  },
};

import type { TeamInfo } from '../types';
import { teamService } from '../services/teamService';
import { collabPipelineStore } from './collabPipelineStore.svelte';

// ── State ──────────────────────────────────────────────────────────────────

let team = $state<TeamInfo | null>(null);
let loading = $state(false);
let error = $state<string | null>(null);

// ── Store ──────────────────────────────────────────────────────────────────

export const teamStore = {
  get team() {
    return team;
  },
  get loading() {
    return loading;
  },
  get error() {
    return error;
  },

  /** Fetch current team on app start. Silently returns null if no team. */
  async init(): Promise<void> {
    loading = true;
    error = null;
    try {
      team = await teamService.getMyTeam();
    } catch (e) {
      // Not critical — user may simply not have a team yet
      console.warn('[Weplex] Team fetch failed:', e);
      team = null;
    } finally {
      loading = false;
    }
  },

  async createTeam(name: string): Promise<void> {
    loading = true;
    error = null;
    try {
      team = await teamService.createTeam(name);
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to create team';
      throw e;
    } finally {
      loading = false;
    }
  },

  async joinTeam(inviteCode: string): Promise<void> {
    loading = true;
    error = null;
    try {
      team = await teamService.joinTeam(inviteCode);
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to join team';
      throw e;
    } finally {
      loading = false;
    }
  },

  async leaveTeam(): Promise<void> {
    loading = true;
    error = null;
    try {
      await teamService.leaveTeam();
      team = null;
      collabPipelineStore.reset();
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to leave team';
      throw e;
    } finally {
      loading = false;
    }
  },

  async regenerateCode(): Promise<void> {
    error = null;
    try {
      const result = await teamService.regenerateCode();
      if (team) {
        team = { ...team, inviteCode: result.inviteCode };
      }
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to regenerate code';
      throw e;
    }
  },

  async removeMember(memberId: string): Promise<void> {
    error = null;
    try {
      await teamService.removeMember(memberId);
      // Update local state — remove member from the list
      if (team) {
        team = {
          ...team,
          members: team.members.filter((m) => m.id !== memberId),
        };
      }
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to remove member';
      throw e;
    }
  },

  /** Clear state (on logout). */
  reset(): void {
    team = null;
    loading = false;
    error = null;
  },

  clearError(): void {
    error = null;
  },
};

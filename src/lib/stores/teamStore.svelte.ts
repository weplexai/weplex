import type { TeamInfo } from '../types';
import { teamService } from '../services/teamService';
import { pipelineWsService } from '../services/pipelineWsService';
import { collabPipelineStore } from './collabPipelineStore.svelte';
import { spaceStore } from './spaceStore.svelte';

// ── State ──────────────────────────────────────────────────────────────────

let teams = $state<TeamInfo[]>([]);
let activeTeamId = $state<string | null>(null);
let loading = $state(false);
let error = $state<string | null>(null);

// ── WS listener cleanup handles ──────────────────────────────────────────

let unsubMemberJoined: (() => void) | null = null;
let unsubMemberLeft: (() => void) | null = null;
let unsubTeamUpdated: (() => void) | null = null;
let unsubTeamDeleted: (() => void) | null = null;

function cleanupListeners(): void {
  unsubMemberJoined?.();
  unsubMemberLeft?.();
  unsubTeamUpdated?.();
  unsubTeamDeleted?.();
  unsubMemberJoined = null;
  unsubMemberLeft = null;
  unsubTeamUpdated = null;
  unsubTeamDeleted = null;
}

// ── Helpers ────────────────────────────────────────────────────────────────

/** Select next available team or null. */
function selectNextTeam(excludeId: string): void {
  const remaining = teams.filter((t) => t.id !== excludeId);
  activeTeamId = remaining.length > 0 ? remaining[0].id : null;
}

/** Update a single team in the list. */
function updateTeamInList(updated: TeamInfo): void {
  teams = teams.map((t) => (t.id === updated.id ? updated : t));
}

// ── Store ──────────────────────────────────────────────────────────────────

export const teamStore = {
  get teams() {
    return teams;
  },
  get activeTeamId() {
    return activeTeamId;
  },
  get activeTeam(): TeamInfo | undefined {
    return teams.find((t) => t.id === activeTeamId);
  },
  get loading() {
    return loading;
  },
  get error() {
    return error;
  },

  /** Fetch all teams on app start. Auto-select first if no active team. */
  async init(): Promise<void> {
    loading = true;
    error = null;
    try {
      teams = await teamService.getMyTeams();
      // Auto-select first team if none selected or current selection is stale
      if (teams.length > 0 && (!activeTeamId || !teams.find((t) => t.id === activeTeamId))) {
        activeTeamId = teams[0].id;
      } else if (teams.length === 0) {
        activeTeamId = null;
      }

      // Join WS rooms for all teams
      for (const team of teams) {
        pipelineWsService.joinTeamRoom(team.id);
      }

      // Subscribe to real-time team events
      cleanupListeners();

      unsubMemberJoined = pipelineWsService.onTeamMemberJoined(({ teamId, member }) => {
        const team = teams.find((t) => t.id === teamId);
        if (team && !team.members.find((m) => m.userId === member.userId)) {
          team.members = [...team.members, member];
          teams = [...teams]; // trigger reactivity
        }
      });

      unsubMemberLeft = pipelineWsService.onTeamMemberLeft(({ teamId, userId }) => {
        const team = teams.find((t) => t.id === teamId);
        if (team) {
          team.members = team.members.filter((m) => m.userId !== userId);
          teams = [...teams];
        }
      });

      unsubTeamUpdated = pipelineWsService.onTeamUpdated(({ teamId, ...updates }) => {
        const team = teams.find((t) => t.id === teamId);
        if (team) {
          Object.assign(team, updates);
          teams = [...teams];
        }
      });

      unsubTeamDeleted = pipelineWsService.onTeamDeleted(({ teamId }) => {
        pipelineWsService.leaveTeamRoom(teamId);
        teams = teams.filter((t) => t.id !== teamId);
        if (activeTeamId === teamId) {
          activeTeamId = teams[0]?.id ?? null;
          collabPipelineStore.reset();
        }
      });

      // Subscribe to real-time space events (spaces arrive via team rooms)
      spaceStore.subscribeToSpaceEvents();
    } catch (e) {
      // Not critical — user may simply not have teams yet
      console.warn('[Weplex] Teams fetch failed:', e);
      teams = [];
      activeTeamId = null;
    } finally {
      loading = false;
    }
  },

  async createTeam(name: string): Promise<void> {
    loading = true;
    error = null;
    try {
      const newTeam = await teamService.createTeam(name);
      teams = [...teams, newTeam];
      activeTeamId = newTeam.id;
      // Join WS room for the new team
      pipelineWsService.joinTeamRoom(newTeam.id);
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
      const joined = await teamService.joinTeam(inviteCode);
      teams = [...teams, joined];
      activeTeamId = joined.id;
      // Join WS room for the new team
      pipelineWsService.joinTeamRoom(joined.id);
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to join team';
      throw e;
    } finally {
      loading = false;
    }
  },

  async leaveTeam(teamId: string): Promise<void> {
    loading = true;
    error = null;
    try {
      await teamService.leaveTeam(teamId);
      pipelineWsService.leaveTeamRoom(teamId);
      teams = teams.filter((t) => t.id !== teamId);
      if (activeTeamId === teamId) {
        selectNextTeam(teamId);
        collabPipelineStore.reset();
      }
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to leave team';
      throw e;
    } finally {
      loading = false;
    }
  },

  async deleteTeam(teamId: string): Promise<void> {
    loading = true;
    error = null;
    try {
      await teamService.deleteTeam(teamId);
      pipelineWsService.leaveTeamRoom(teamId);
      teams = teams.filter((t) => t.id !== teamId);
      if (activeTeamId === teamId) {
        selectNextTeam(teamId);
        collabPipelineStore.reset();
      }
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to delete team';
      throw e;
    } finally {
      loading = false;
    }
  },

  /** Switch active team context. Resets collaborative pipeline state. */
  setActiveTeam(teamId: string): void {
    if (activeTeamId === teamId) return;
    activeTeamId = teamId;
    // Reset and re-init collab pipelines for the new team
    collabPipelineStore.reset();
    collabPipelineStore.init().catch((e) =>
      console.warn('[Weplex] Collab pipeline re-init failed on team switch:', e),
    );
  },

  async regenerateCode(teamId: string): Promise<void> {
    error = null;
    try {
      const result = await teamService.regenerateCode(teamId);
      const target = teams.find((t) => t.id === teamId);
      if (target) {
        updateTeamInList({ ...target, inviteCode: result.inviteCode });
      }
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to regenerate code';
      throw e;
    }
  },

  async removeMember(teamId: string, memberId: string): Promise<void> {
    error = null;
    try {
      await teamService.removeMember(teamId, memberId);
      // Update local state — remove member from the list
      const target = teams.find((t) => t.id === teamId);
      if (target) {
        updateTeamInList({
          ...target,
          members: target.members.filter((m) => m.id !== memberId),
        });
      }
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to remove member';
      throw e;
    }
  },

  /** Clear all state (on logout). */
  reset(): void {
    cleanupListeners();
    spaceStore.unsubscribeFromSpaceEvents();
    teams = [];
    activeTeamId = null;
    loading = false;
    error = null;
  },

  clearError(): void {
    error = null;
  },
};

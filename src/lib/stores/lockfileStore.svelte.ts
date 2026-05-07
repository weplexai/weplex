// Per-profile lockfile cache. Mirrors the backend's `.weplex.lock.yaml`
// for the Hub UI: resource history, drift indicator, restore action.
//
// Phase 3 ownership: the lockfile is the single source of truth for what
// is installed in a profile (with provenance + version history). The
// `compileScheduler` and `guardStore` continue to own their respective
// concerns; this store just answers "what's installed and where did it
// come from?". After mutating actions (restore, marketplace install, any
// resource create/copy/delete), callers should `refresh()` here AND
// schedule a compile + guard refresh — this store does both for `restore`.

import { invoke } from '@tauri-apps/api/core';
import type {
  Lockfile,
  LockfileEntry,
  LockfileHistoryEntry,
  MutationReport,
} from '../types/lockfile';
import { schedule as scheduleCompile } from '../utils/compileScheduler';
import { settingsStore } from './settingsStore.svelte';
import { guardStore } from './guardStore.svelte';

// ─── Types ──────────────────────────────────────────────────────────────

export interface ProfileLockfileState {
  lockfile: Lockfile | null;
  loading: boolean;
  error: string | null;
}

// ─── State ──────────────────────────────────────────────────────────────

// Keyed by profile.configDir.
let byProfile = $state<Record<string, ProfileLockfileState>>({});

// ─── Helpers ────────────────────────────────────────────────────────────

function ensureEntry(profileConfigDir: string): ProfileLockfileState {
  let entry = byProfile[profileConfigDir];
  if (!entry) {
    entry = { lockfile: null, loading: false, error: null };
    byProfile = { ...byProfile, [profileConfigDir]: entry };
  }
  return entry;
}

function patch(
  profileConfigDir: string,
  update: Partial<ProfileLockfileState>,
): void {
  const current = ensureEntry(profileConfigDir);
  byProfile = {
    ...byProfile,
    [profileConfigDir]: { ...current, ...update },
  };
}

// ─── Store ──────────────────────────────────────────────────────────────

export const lockfileStore = {
  get byProfile() {
    return byProfile;
  },

  /**
   * Reload the lockfile for `profileConfigDir`. Errors are captured on
   * the per-profile entry — never thrown.
   */
  async refresh(profileConfigDir: string): Promise<void> {
    if (!profileConfigDir) return;
    patch(profileConfigDir, { loading: true, error: null });
    try {
      const lockfile = await invoke<Lockfile>('read_lockfile', {
        profileConfigDir,
      });
      patch(profileConfigDir, { lockfile, loading: false, error: null });
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      patch(profileConfigDir, { loading: false, error: msg });
      console.warn('[weplex] lockfile refresh failed:', msg);
    }
  },

  /** Current lockfile entry for a resource id, or null. */
  entryForResource(
    profileConfigDir: string,
    resourceId: string,
  ): LockfileEntry | null {
    const lockfile = byProfile[profileConfigDir]?.lockfile;
    if (!lockfile) return null;
    return lockfile.resources.find((r) => r.id === resourceId) ?? null;
  },

  /** History entries for a resource id (newest first by backend convention). */
  historyFor(
    profileConfigDir: string,
    resourceId: string,
  ): LockfileHistoryEntry[] {
    const lockfile = byProfile[profileConfigDir]?.lockfile;
    if (!lockfile) return [];
    return lockfile.history[resourceId] ?? [];
  },

  /** True if the on-disk body diverges from the lockfile-recorded sha. */
  isDrifted(profileConfigDir: string, resourceId: string): boolean {
    return this.entryForResource(profileConfigDir, resourceId)?.drifted ?? false;
  },

  /**
   * Restore a previous version of a resource from the cache. After
   * success: refresh self, schedule a compile + guard scan so the
   * external-agent target files and the guard verdict catch up.
   */
  async restore(
    profileConfigDir: string,
    resourceId: string,
    targetSha256: string,
  ): Promise<MutationReport> {
    const report = await invoke<MutationReport>('restore_resource_version', {
      profileConfigDir,
      resourceId,
      targetSha256,
    });
    await this.refresh(profileConfigDir);
    // Best-effort: don't block the restore on these.
    scheduleCompile(profileConfigDir, {
      deepScan: settingsStore.settings.agentshieldDeepScan,
    });
    guardStore
      .refresh(profileConfigDir, settingsStore.settings.agentshieldDeepScan)
      .catch(() => {
        // guardStore.refresh swallows errors itself; this catch only
        // exists to make it `void` for callers that ignore the promise.
      });
    return report;
  },
};

import { invoke } from '@tauri-apps/api/core';
import type {
  GuardFinding,
  GuardVerdict,
  OverrideDecision,
  ResourceVerdict,
  ScanReport,
} from '../types/guard';

// ─── Types ──────────────────────────────────────────────────────────────

export interface ProfileGuardState {
  report: ScanReport | null;
  loading: boolean;
  error: string | null;
}

// ─── State ──────────────────────────────────────────────────────────────

// Keyed by profile.configDir.
let byProfile = $state<Record<string, ProfileGuardState>>({});

// Per-profile lookup keyed by resourcePath. Two profiles can symlink to
// the same body file (or, for profile-relative paths under HOME, share
// the same canonical path) — keeping the maps separate keeps each
// profile's verdict honest. Lookups that pass a profile context use
// `verdictForInProfile`; the legacy flat `verdictFor` path is kept for
// callers that don't have a profile in scope and now picks
// deterministically (alphabetical by profile dir) with a one-shot
// console warning on the first cross-profile collision.
let byResourcePathPerProfile = $derived.by(() => {
  const m: Record<string, Record<string, ResourceVerdict>> = {};
  for (const [profileDir, ps] of Object.entries(byProfile)) {
    if (!ps.report) continue;
    const inner: Record<string, ResourceVerdict> = {};
    for (const r of ps.report.resources) {
      inner[r.resourcePath] = r;
    }
    m[profileDir] = inner;
  }
  return m;
});

// Flat lookup retained for legacy callers without profile context.
// Walks profiles in alphabetical order so the "winner" on a collision
// is deterministic across renders. The collision flag (computed inside
// the derived) is read by `verdictFor` to emit a one-shot warning at
// call time — keeping the derived itself side-effect-free.
let byResourcePath = $derived.by(() => {
  const m: Record<string, ResourceVerdict> = {};
  let hasCollision = false;
  const profileDirs = Object.keys(byResourcePathPerProfile).sort();
  for (const profileDir of profileDirs) {
    const inner = byResourcePathPerProfile[profileDir];
    for (const [path, verdict] of Object.entries(inner)) {
      if (m[path] && m[path].bodySha256 !== verdict.bodySha256) {
        hasCollision = true;
        // First writer wins — do not overwrite.
        continue;
      }
      m[path] = verdict;
    }
  }
  return { map: m, hasCollision };
});

let crossProfileCollisionWarned = false;

function maybeWarnCollision(): void {
  if (crossProfileCollisionWarned) return;
  if (!byResourcePath.hasCollision) return;
  crossProfileCollisionWarned = true;
  console.warn(
    '[weplex] guardStore: resourcePath collision across profiles — ' +
      'first verdict wins (alphabetical by profile dir). ' +
      'Use verdictForInProfile()/findingsForInProfile() for profile-specific lookups.',
  );
}

// ─── Helpers ────────────────────────────────────────────────────────────

function ensureEntry(profileConfigDir: string): ProfileGuardState {
  let entry = byProfile[profileConfigDir];
  if (!entry) {
    entry = { report: null, loading: false, error: null };
    byProfile = { ...byProfile, [profileConfigDir]: entry };
  }
  return entry;
}

function patch(profileConfigDir: string, update: Partial<ProfileGuardState>): void {
  const current = ensureEntry(profileConfigDir);
  byProfile = {
    ...byProfile,
    [profileConfigDir]: { ...current, ...update },
  };
}

// ─── Store ──────────────────────────────────────────────────────────────

export const guardStore = {
  get byProfile() {
    return byProfile;
  },
  get byResourcePath() {
    return byResourcePath.map;
  },

  /**
   * Re-scan the given profile and cache the report. Errors are captured on
   * the per-profile entry — never thrown.
   */
  async refresh(profileConfigDir: string, deepScan: boolean): Promise<void> {
    if (!profileConfigDir) return;
    patch(profileConfigDir, { loading: true, error: null });
    try {
      const report = await invoke<ScanReport>('scan_profile', {
        profileConfigDir,
        projectRoot: null,
        deepScan,
      });
      patch(profileConfigDir, { report, loading: false, error: null });
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      patch(profileConfigDir, { loading: false, error: msg });
      console.warn('[weplex] guard refresh failed:', msg);
    }
  },

  /**
   * Profile-scoped verdict lookup — pass this when you have the profile
   * dir in scope. Avoids cross-profile collisions when two profiles
   * symlink the same body path. Returns 'green' for unscanned resources.
   */
  verdictForInProfile(profileConfigDir: string, resourcePath: string): GuardVerdict {
    return byResourcePathPerProfile[profileConfigDir]?.[resourcePath]?.verdict ?? 'green';
  },

  /**
   * Profile-scoped findings lookup. See `verdictForInProfile`.
   */
  findingsForInProfile(
    profileConfigDir: string,
    resourcePath: string,
  ): ResourceVerdict | null {
    return byResourcePathPerProfile[profileConfigDir]?.[resourcePath] ?? null;
  },

  /**
   * Legacy flat lookup. Use `verdictForInProfile` when a profile is in
   * scope — this version picks deterministically on collision but
   * warns once per session if two profiles disagree on the same path.
   * Returns 'green' for unscanned resources.
   */
  verdictFor(resourcePath: string): GuardVerdict {
    maybeWarnCollision();
    return byResourcePath.map[resourcePath]?.verdict ?? 'green';
  },

  /**
   * Legacy flat lookup. See `verdictFor`.
   */
  findingsFor(resourcePath: string): ResourceVerdict | null {
    maybeWarnCollision();
    return byResourcePath.map[resourcePath] ?? null;
  },

  /**
   * Profile-level findings that don't map to a single resource (e.g.
   * profile-wide settings/permissions concerns from a deep scan).
   * Reserved for a future Hub indicator — call sites today are stubs.
   */
  profileFindingsFor(profileConfigDir: string): GuardFinding[] {
    return byProfile[profileConfigDir]?.report?.profileFindings ?? [];
  },

  /**
   * Persist an override decision for a finding and refresh the profile so
   * the UI reflects the new effective verdict.
   */
  async setOverride(
    profileConfigDir: string,
    decision: OverrideDecision,
  ): Promise<void> {
    await invoke('set_override_decision', { profileConfigDir, decision });
    // After setting an override, the verdict math changes — refresh.
    // Preserve the deepScan setting from the previous run if any.
    const prevDeep = byProfile[profileConfigDir]?.report?.deepScanRan ?? false;
    await this.refresh(profileConfigDir, prevDeep);
  },

  /**
   * Push a scan report into the cache without an extra invoke roundtrip —
   * called by the compileScheduler caller after a fresh scan.
   */
  ingestReport(profileConfigDir: string, report: ScanReport): void {
    patch(profileConfigDir, { report, loading: false, error: null });
  },
};

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

// Flat lookup keyed by `resourcePath` for badge rendering. Derived from
// every profile's report — last writer wins (resourcePath is canonical
// per body file so collisions across profiles are not expected).
let byResourcePath = $derived.by(() => {
  const m: Record<string, ResourceVerdict> = {};
  for (const ps of Object.values(byProfile)) {
    if (!ps.report) continue;
    for (const r of ps.report.resources) {
      m[r.resourcePath] = r;
    }
  }
  return m;
});

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
    return byResourcePath;
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
   * Lookup verdict for a resource by its body path. Returns 'green' for
   * unscanned resources so the happy-path Hub stays unmarked.
   */
  verdictFor(resourcePath: string): GuardVerdict {
    return byResourcePath[resourcePath]?.verdict ?? 'green';
  },

  findingsFor(resourcePath: string): ResourceVerdict | null {
    return byResourcePath[resourcePath] ?? null;
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

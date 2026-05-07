import { describe, it, expect, beforeEach, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import type { ResourceVerdict, ScanReport } from '../types/guard';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

const mockedInvoke = vi.mocked(invoke);

// Import AFTER the mock so the module's $derived setup uses the mocked
// invoke when refresh() runs.
import { guardStore } from './guardStore.svelte';

function buildVerdict(
  resourcePath: string,
  bodySha256: string,
  verdict: 'green' | 'yellow' | 'red' = 'yellow',
): ResourceVerdict {
  return {
    resourcePath,
    manifestPath: `${resourcePath}.weplex.yaml`,
    resourceId: resourcePath.split('/').pop() ?? 'r',
    kind: 'agent',
    bodySha256,
    verdict,
    findings: [],
    overriddenFindings: [],
  };
}

function buildReport(
  profileDir: string,
  resources: ResourceVerdict[],
): ScanReport {
  return {
    profileDir,
    resources,
    overall: 'yellow',
    deepScanRan: false,
    deepScanSkippedReason: null,
    profileFindings: [],
  };
}

describe('guardStore profile-scoped lookups', () => {
  beforeEach(() => {
    mockedInvoke.mockReset();
  });

  it('verdictForInProfile returns the correct profile verdict on collision', async () => {
    const profileA = '/home/u/.claude-a';
    const profileB = '/home/u/.claude-b';
    // Two profiles whose resources share the same canonical path (e.g.
    // a symlinked body) but differ in body SHA / verdict.
    const sharedPath = '/home/u/.shared/agents/x.md';

    mockedInvoke.mockImplementation(async (cmd, args) => {
      if (cmd !== 'scan_profile') return null;
      const argsObj = args as { profileConfigDir?: string };
      if (argsObj.profileConfigDir === profileA) {
        return buildReport(profileA, [
          buildVerdict(sharedPath, 'sha-a', 'red'),
        ]);
      }
      if (argsObj.profileConfigDir === profileB) {
        return buildReport(profileB, [
          buildVerdict(sharedPath, 'sha-b', 'yellow'),
        ]);
      }
      return null;
    });

    await guardStore.refresh(profileA, false);
    await guardStore.refresh(profileB, false);

    // Profile-scoped lookups return the profile's own verdict.
    expect(guardStore.verdictForInProfile(profileA, sharedPath)).toBe('red');
    expect(guardStore.verdictForInProfile(profileB, sharedPath)).toBe('yellow');

    // Profile-scoped findings honour the per-profile body SHA.
    expect(guardStore.findingsForInProfile(profileA, sharedPath)?.bodySha256).toBe(
      'sha-a',
    );
    expect(guardStore.findingsForInProfile(profileB, sharedPath)?.bodySha256).toBe(
      'sha-b',
    );
  });

  it('verdictForInProfile returns green for unscanned profile', () => {
    expect(
      guardStore.verdictForInProfile('/never/scanned', '/some/path'),
    ).toBe('green');
    expect(
      guardStore.findingsForInProfile('/never/scanned', '/some/path'),
    ).toBeNull();
  });

  it('legacy verdictFor picks deterministically and warns once on collision', async () => {
    const profileA = '/home/u/.profA';
    const profileZ = '/home/u/.profZ';
    const sharedPath = '/home/u/.shared/agents/y.md';

    mockedInvoke.mockImplementation(async (cmd, args) => {
      if (cmd !== 'scan_profile') return null;
      const argsObj = args as { profileConfigDir?: string };
      if (argsObj.profileConfigDir === profileA) {
        return buildReport(profileA, [
          buildVerdict(sharedPath, 'sha-A', 'red'),
        ]);
      }
      if (argsObj.profileConfigDir === profileZ) {
        return buildReport(profileZ, [
          buildVerdict(sharedPath, 'sha-Z', 'yellow'),
        ]);
      }
      return null;
    });

    await guardStore.refresh(profileA, false);
    await guardStore.refresh(profileZ, false);

    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    // Alphabetical winner: profileA. With sha-A != sha-Z, the second
    // profile's entry is ignored and a warning is logged once.
    const v = guardStore.verdictFor(sharedPath);
    expect(v).toBe('red');
    // Calling again must not double-warn.
    guardStore.verdictFor(sharedPath);
    expect(warn).toHaveBeenCalledTimes(1);
    warn.mockRestore();
  });
});

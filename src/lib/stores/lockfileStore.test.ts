import { describe, it, expect, beforeEach, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import type {
  Lockfile,
  LockfileEntry,
  LockfileHistoryEntry,
  MutationReport,
} from '../types/lockfile';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

// guardStore.refresh and compileScheduler.schedule call invoke too —
// keep the mocks decoupled so we only assert lockfile-relevant traffic.
vi.mock('../utils/compileScheduler', () => ({
  schedule: vi.fn(() => Promise.resolve(null)),
}));

const mockedInvoke = vi.mocked(invoke);

// Import AFTER the mocks so the module's setup uses them.
import { lockfileStore } from './lockfileStore.svelte';
import { schedule as scheduleCompile } from '../utils/compileScheduler';

const mockedSchedule = vi.mocked(scheduleCompile);

const PROFILE_A = '/home/u/.profile-a';

function buildEntry(
  id: string,
  sha256: string,
  overrides: Partial<LockfileEntry> = {},
): LockfileEntry {
  return {
    id,
    kind: 'agent',
    source: 'user',
    version: null,
    sha256,
    sidecarSha256: null,
    files: [`${id}.md`],
    installedAt: '2026-04-01T10:00:00Z',
    installedBy: 'tester',
    pack: null,
    drifted: false,
    ...overrides,
  };
}

function buildHistoryEntry(
  sha256: string,
  installedAt: string,
  overrides: Partial<LockfileHistoryEntry> = {},
): LockfileHistoryEntry {
  return {
    version: null,
    sha256,
    sidecarSha256: null,
    source: 'user',
    installedAt,
    cachePaths: [`.weplex/cache/${sha256}/agents/x.md`],
    ...overrides,
  };
}

function buildLockfile(
  resources: LockfileEntry[],
  history: Record<string, LockfileHistoryEntry[]> = {},
): Lockfile {
  return {
    version: 1,
    generatedBy: 'weplex',
    resources,
    history,
  };
}

describe('lockfileStore', () => {
  beforeEach(() => {
    mockedInvoke.mockReset();
    mockedSchedule.mockClear();
  });

  it('refresh loads the lockfile and exposes it via byProfile', async () => {
    const lock = buildLockfile([buildEntry('agents/x', 'sha-x')]);
    mockedInvoke.mockImplementation(async (cmd) => {
      if (cmd === 'read_lockfile') return lock;
      throw new Error(`unexpected ${cmd}`);
    });

    await lockfileStore.refresh(PROFILE_A);

    expect(mockedInvoke).toHaveBeenCalledWith('read_lockfile', {
      profileConfigDir: PROFILE_A,
    });
    expect(lockfileStore.byProfile[PROFILE_A]?.lockfile).toEqual(lock);
    expect(lockfileStore.byProfile[PROFILE_A]?.error).toBeNull();
    expect(lockfileStore.byProfile[PROFILE_A]?.loading).toBe(false);
  });

  it('refresh records error and keeps the prior lockfile when invoke throws', async () => {
    // Successful first load.
    const lock = buildLockfile([buildEntry('agents/x', 'sha-x')]);
    mockedInvoke.mockResolvedValueOnce(lock);
    await lockfileStore.refresh(PROFILE_A);
    expect(lockfileStore.byProfile[PROFILE_A]?.lockfile).toEqual(lock);

    // Failing second load.
    mockedInvoke.mockRejectedValueOnce(new Error('boom'));
    await lockfileStore.refresh(PROFILE_A);
    expect(lockfileStore.byProfile[PROFILE_A]?.error).toBe('boom');
    // Previous lockfile remains — we don't blank out the cache on error.
    expect(lockfileStore.byProfile[PROFILE_A]?.lockfile).toEqual(lock);
  });

  it('refresh is a no-op on empty profile dir', async () => {
    await lockfileStore.refresh('');
    expect(mockedInvoke).not.toHaveBeenCalled();
  });

  it('entryForResource returns the entry or null when missing', async () => {
    const entry = buildEntry('agents/x', 'sha-x');
    const lock = buildLockfile([entry]);
    mockedInvoke.mockResolvedValueOnce(lock);
    await lockfileStore.refresh(PROFILE_A);

    expect(lockfileStore.entryForResource(PROFILE_A, 'agents/x')).toEqual(entry);
    expect(lockfileStore.entryForResource(PROFILE_A, 'agents/missing')).toBeNull();
    // Profile that hasn't been refreshed yet.
    expect(lockfileStore.entryForResource('/never', 'agents/x')).toBeNull();
  });

  it('historyFor returns the recorded history (empty when missing)', async () => {
    const hist = [
      buildHistoryEntry('sha-old', '2026-03-01T00:00:00Z'),
      buildHistoryEntry('sha-older', '2026-02-01T00:00:00Z'),
    ];
    const lock = buildLockfile([buildEntry('agents/x', 'sha-x')], {
      'agents/x': hist,
    });
    mockedInvoke.mockResolvedValueOnce(lock);
    await lockfileStore.refresh(PROFILE_A);

    expect(lockfileStore.historyFor(PROFILE_A, 'agents/x')).toEqual(hist);
    expect(lockfileStore.historyFor(PROFILE_A, 'agents/none')).toEqual([]);
  });

  it('isDrifted reflects the lockfile entry drift flag', async () => {
    const lock = buildLockfile([
      buildEntry('agents/clean', 'sha-clean', { drifted: false }),
      buildEntry('agents/dirty', 'sha-dirty', { drifted: true }),
    ]);
    mockedInvoke.mockResolvedValueOnce(lock);
    await lockfileStore.refresh(PROFILE_A);

    expect(lockfileStore.isDrifted(PROFILE_A, 'agents/clean')).toBe(false);
    expect(lockfileStore.isDrifted(PROFILE_A, 'agents/dirty')).toBe(true);
    // Unknown id defaults to false (not drifted == clean).
    expect(lockfileStore.isDrifted(PROFILE_A, 'agents/unknown')).toBe(false);
  });

  it('restore invokes the backend and triggers compile + lockfile reload', async () => {
    const initialLock = buildLockfile([buildEntry('agents/x', 'sha-current')]);
    const restoredLock = buildLockfile([buildEntry('agents/x', 'sha-target')]);
    const report: MutationReport = {
      resourceId: 'agents/x',
      previousSha256: 'sha-current',
      newSha256: 'sha-target',
      historyAdded: true,
      cachePaths: ['.weplex/cache/sha-current/agents/x.md'],
      noOp: false,
    };

    mockedInvoke.mockImplementation(async (cmd) => {
      if (cmd === 'read_lockfile') {
        // First call (initial seed) returns initial; second call (post-
        // restore refresh) returns the restored lockfile.
        return mockedInvoke.mock.calls.filter((c) => c[0] === 'read_lockfile')
          .length === 1
          ? initialLock
          : restoredLock;
      }
      if (cmd === 'restore_resource_version') return report;
      if (cmd === 'scan_profile') {
        return {
          profileDir: PROFILE_A,
          resources: [],
          overall: 'green',
          deepScanRan: false,
          deepScanSkippedReason: null,
          profileFindings: [],
        };
      }
      throw new Error(`unexpected ${cmd}`);
    });

    await lockfileStore.refresh(PROFILE_A);
    const result = await lockfileStore.restore(PROFILE_A, 'agents/x', 'sha-target');

    expect(result).toEqual(report);
    expect(mockedInvoke).toHaveBeenCalledWith('restore_resource_version', {
      profileConfigDir: PROFILE_A,
      resourceId: 'agents/x',
      targetSha256: 'sha-target',
    });
    // Refresh ran again post-restore and updated the cache.
    expect(lockfileStore.byProfile[PROFILE_A]?.lockfile).toEqual(restoredLock);
    // Compile scheduler was kicked.
    expect(mockedSchedule).toHaveBeenCalledWith(
      PROFILE_A,
      expect.objectContaining({ deepScan: expect.any(Boolean) }),
    );
  });
});

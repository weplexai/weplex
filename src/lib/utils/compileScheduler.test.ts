import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { schedule, cancel, __resetForTests } from './compileScheduler';
import type { CompileReport, ScanReport } from '../types/guard';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

const mockedInvoke = vi.mocked(invoke);

const PROFILE = '/tmp/profile';

const okReport: CompileReport = {
  profileDir: PROFILE,
  manifestsSeen: 0,
  targetsWritten: [],
  targetsUnchanged: [],
  orphansRemoved: [],
  errors: [],
};

const okScan: ScanReport = {
  profileDir: PROFILE,
  resources: [],
  overall: 'green',
  deepScanRan: false,
  deepScanSkippedReason: null,
};

function setupHappyPath(): void {
  mockedInvoke.mockImplementation(async (cmd: string) => {
    if (cmd === 'compile_profile_to_external_agents') return okReport;
    if (cmd === 'scan_profile') return okScan;
    throw new Error(`unexpected invoke ${cmd}`);
  });
}

beforeEach(() => {
  __resetForTests();
  mockedInvoke.mockReset();
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
  __resetForTests();
});

describe('compileScheduler.schedule', () => {
  it('coalesces two calls within the debounce window into one invoke', async () => {
    setupHappyPath();
    const p1 = schedule(PROFILE, { debounceMs: 100 });
    const p2 = schedule(PROFILE, { debounceMs: 100 });

    // Same Promise returned to both callers within the window.
    expect(p1).toBe(p2);

    // Before the timer fires, no invokes yet.
    expect(mockedInvoke).not.toHaveBeenCalled();

    // Advance timers and let the run finish.
    await vi.advanceTimersByTimeAsync(100);
    const result = await p1;

    expect(result).not.toBeNull();
    expect(result?.report).toEqual(okReport);
    // One compile invocation, one scan invocation.
    const compileCalls = mockedInvoke.mock.calls.filter(
      (c) => c[0] === 'compile_profile_to_external_agents',
    );
    expect(compileCalls).toHaveLength(1);
  });

  it('runs two compiles when calls are separated by more than the debounce', async () => {
    setupHappyPath();
    const p1 = schedule(PROFILE, { debounceMs: 100 });
    await vi.advanceTimersByTimeAsync(100);
    await p1;

    const p2 = schedule(PROFILE, { debounceMs: 100 });
    await vi.advanceTimersByTimeAsync(100);
    await p2;

    const compileCalls = mockedInvoke.mock.calls.filter(
      (c) => c[0] === 'compile_profile_to_external_agents',
    );
    expect(compileCalls).toHaveLength(2);
  });

  it('retries once after a "compile already in progress" error', async () => {
    let attempts = 0;
    mockedInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'compile_profile_to_external_agents') {
        attempts++;
        if (attempts === 1) {
          throw new Error('compile already in progress for /tmp/profile');
        }
        return okReport;
      }
      if (cmd === 'scan_profile') return okScan;
      throw new Error(`unexpected ${cmd}`);
    });

    const p = schedule(PROFILE, { debounceMs: 50 });
    // Advance through debounce + retry delay (250ms).
    await vi.advanceTimersByTimeAsync(50);
    await vi.advanceTimersByTimeAsync(260);
    const result = await p;

    expect(attempts).toBe(2);
    expect(result).not.toBeNull();
  });

  it('does not drop edits made during an inflight run', async () => {
    let resolveFirstCompile: ((v: CompileReport) => void) | null = null;
    let compileCount = 0;

    mockedInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'compile_profile_to_external_agents') {
        compileCount++;
        if (compileCount === 1) {
          // Block the first compile until we explicitly resolve it.
          return new Promise<CompileReport>((res) => {
            resolveFirstCompile = res;
          });
        }
        return okReport;
      }
      if (cmd === 'scan_profile') return okScan;
      throw new Error(`unexpected ${cmd}`);
    });

    const first = schedule(PROFILE, { debounceMs: 50 });
    await vi.advanceTimersByTimeAsync(50);
    // Inflight now — compile #1 is blocked.
    expect(compileCount).toBe(1);

    // New schedule arrives during inflight — should be queued, not lost.
    const second = schedule(PROFILE, { debounceMs: 50 });
    await vi.advanceTimersByTimeAsync(50);

    // Still only one compile happened (second is chained behind first).
    expect(compileCount).toBe(1);

    // Unblock the first compile.
    resolveFirstCompile!(okReport);
    await first;

    // After first resolves, the second run should fire.
    await vi.advanceTimersByTimeAsync(0);
    await second;
    expect(compileCount).toBe(2);
  });

  it('cancel resolves the pending Promise to null and clears the timer', async () => {
    setupHappyPath();
    const p = schedule(PROFILE, { debounceMs: 200 });
    cancel(PROFILE);

    // Even after the would-be timer fires, no invoke happens.
    await vi.advanceTimersByTimeAsync(500);
    const result = await p;

    expect(result).toBeNull();
    expect(mockedInvoke).not.toHaveBeenCalled();
  });

  it('dispatches compileScheduler:error event on persistent failure', async () => {
    mockedInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'compile_profile_to_external_agents') {
        throw new Error('disk full');
      }
      throw new Error(`unexpected ${cmd}`);
    });

    const seen: { profileConfigDir: string; message: string }[] = [];
    const handler = (e: Event) => {
      const detail = (e as CustomEvent<{ profileConfigDir: string; message: string }>)
        .detail;
      seen.push(detail);
    };
    window.addEventListener('compileScheduler:error', handler);

    try {
      const p = schedule(PROFILE, { debounceMs: 50 });
      await vi.advanceTimersByTimeAsync(50);
      const result = await p;

      expect(result).toBeNull();
      expect(seen).toHaveLength(1);
      expect(seen[0].profileConfigDir).toBe(PROFILE);
      expect(seen[0].message).toMatch(/disk full/);
    } finally {
      window.removeEventListener('compileScheduler:error', handler);
    }
  });
});

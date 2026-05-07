// Debounced compile + scan scheduler. Coalesces rapid mutations (resource
// create/copy/delete or session start) into a single
// `compile_profile_to_external_agents` invocation per profile, then a single
// `scan_profile`. Keeps the UI responsive: callers fire-and-forget, await
// the returned Promise, or chain off it.
//
// Contract:
// - Multiple `schedule()` calls within DEBOUNCE_MS for the same profile share
//   one Promise (trailing-edge: latest opts win, replace not merge).
// - During an inflight compile, a new `schedule()` creates a fresh
//   debounce-scheduled compile that fires after the current one resolves —
//   guaranteeing no edits are dropped.
// - The Phase 1 lock-busy error string contains "compile already in progress".
//   We sleep 250ms and retry once before giving up.
// - We never throw — failed runs resolve to `null` and dispatch a
//   `compileScheduler:error` window event for the UI to toast.

import { invoke } from '@tauri-apps/api/core';
import type { ScanReport, CompileReport } from '../types/guard';

export interface ScheduleOptions {
  projectRoot?: string | null;
  deepScan?: boolean;
  /** Override the 500ms debounce — for tests only. */
  debounceMs?: number;
}

export interface ScheduleResult {
  report: CompileReport;
  /** `null` if the post-compile scan failed; compile itself is still
   *  considered successful. */
  scan: ScanReport | null;
}

const DEFAULT_DEBOUNCE_MS = 500;
const RETRY_DELAY_MS = 250;
const LOCK_BUSY_NEEDLE = 'compile already in progress';

interface PendingEntry {
  /** Resolves when the next debounced run finishes. */
  promise: Promise<ScheduleResult | null>;
  resolve: (value: ScheduleResult | null) => void;
  /** Latest opts; trailing-edge wins. */
  opts: ScheduleOptions;
  /** setTimeout handle for the debounce; null once fired. */
  timer: ReturnType<typeof setTimeout> | null;
}

const pending = new Map<string, PendingEntry>();
const inflight = new Map<string, Promise<ScheduleResult | null>>();

/**
 * Schedule a debounced compile + scan for the given profile.
 *
 * Returns the same Promise to all callers within the debounce window. When
 * called during an inflight run, schedules a fresh trailing run after the
 * current one resolves so edits made mid-compile aren't dropped.
 *
 * Errors never propagate as rejections — the Promise resolves to `null` and
 * the scheduler dispatches a `compileScheduler:error` CustomEvent on
 * `window` (detail: `{ profileConfigDir, message }`).
 */
export function schedule(
  profileConfigDir: string,
  opts: ScheduleOptions = {},
): Promise<ScheduleResult | null> {
  const debounceMs = opts.debounceMs ?? DEFAULT_DEBOUNCE_MS;

  // If a debounce window is already open for this profile, replace its opts
  // and reset its timer — same Promise stays.
  const existing = pending.get(profileConfigDir);
  if (existing) {
    existing.opts = opts;
    if (existing.timer !== null) {
      clearTimeout(existing.timer);
    }
    existing.timer = setTimeout(() => fire(profileConfigDir), debounceMs);
    return existing.promise;
  }

  // Open a new debounce window. The Promise resolves once the run finishes
  // (success → ScheduleResult, failure → null).
  let resolveFn!: (value: ScheduleResult | null) => void;
  const promise = new Promise<ScheduleResult | null>((res) => {
    resolveFn = res;
  });

  const entry: PendingEntry = {
    promise,
    resolve: resolveFn,
    opts,
    timer: setTimeout(() => fire(profileConfigDir), debounceMs),
  };
  pending.set(profileConfigDir, entry);
  return promise;
}

/**
 * Cancel any pending debounce for `profileConfigDir`. Resolves the pending
 * Promise to `null` so awaiters don't hang.
 */
export function cancel(profileConfigDir: string): void {
  const entry = pending.get(profileConfigDir);
  if (!entry) return;
  if (entry.timer !== null) {
    clearTimeout(entry.timer);
    entry.timer = null;
  }
  pending.delete(profileConfigDir);
  entry.resolve(null);
}

/**
 * Test-only: clear all timers and state.
 */
export function __resetForTests(): void {
  for (const entry of pending.values()) {
    if (entry.timer !== null) clearTimeout(entry.timer);
    entry.resolve(null);
  }
  pending.clear();
  inflight.clear();
}

// ─── Internals ──────────────────────────────────────────────────────────

function sleep(ms: number): Promise<void> {
  return new Promise((res) => setTimeout(res, ms));
}

async function fire(profileConfigDir: string): Promise<void> {
  const entry = pending.get(profileConfigDir);
  if (!entry) return;
  // Detach the entry — any further schedule() during this run will open
  // a fresh debounce window.
  pending.delete(profileConfigDir);
  entry.timer = null;

  // If there's already an inflight run for this profile, chain after it.
  const previous = inflight.get(profileConfigDir);
  const runPromise = (previous ?? Promise.resolve()).then(() =>
    runOnce(profileConfigDir, entry.opts),
  );
  inflight.set(profileConfigDir, runPromise);

  try {
    const result = await runPromise;
    entry.resolve(result);
  } finally {
    // Only clear inflight if this is still the latest run.
    if (inflight.get(profileConfigDir) === runPromise) {
      inflight.delete(profileConfigDir);
    }
  }
}

async function runOnce(
  profileConfigDir: string,
  opts: ScheduleOptions,
): Promise<ScheduleResult | null> {
  const projectRoot = opts.projectRoot ?? null;
  const deepScan = opts.deepScan ?? false;

  let report: CompileReport;
  try {
    report = await invokeCompile(profileConfigDir, projectRoot);
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    if (msg.includes(LOCK_BUSY_NEEDLE)) {
      await sleep(RETRY_DELAY_MS);
      try {
        report = await invokeCompile(profileConfigDir, projectRoot);
      } catch (e2) {
        return failWithEvent(profileConfigDir, e2);
      }
    } else {
      return failWithEvent(profileConfigDir, e);
    }
  }

  // Compile succeeded — try to scan. Scan failure is non-fatal: we still
  // return the compile report so the UI can show what was written.
  let scan: ScanReport | null = null;
  try {
    scan = await invoke<ScanReport>('scan_profile', {
      profileConfigDir,
      projectRoot,
      deepScan,
    });
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    console.warn('[weplex] scan_profile failed (non-fatal):', msg);
  }

  return { report, scan };
}

function invokeCompile(
  profileConfigDir: string,
  projectRoot: string | null,
): Promise<CompileReport> {
  return invoke<CompileReport>('compile_profile_to_external_agents', {
    profileConfigDir,
    projectRoot,
  });
}

function failWithEvent(profileConfigDir: string, e: unknown): null {
  const message = e instanceof Error ? e.message : String(e);
  console.warn('[weplex] compile failed:', message);
  if (typeof window !== 'undefined') {
    window.dispatchEvent(
      new CustomEvent('compileScheduler:error', {
        detail: { profileConfigDir, message },
      }),
    );
  }
  return null;
}

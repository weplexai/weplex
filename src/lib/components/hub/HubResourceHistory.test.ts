import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import { invoke } from '@tauri-apps/api/core';
import HubResourceHistory from './HubResourceHistory.svelte';
import type {
  LockfileEntry,
  LockfileHistoryEntry,
} from '../../types/lockfile';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('../../utils/compileScheduler', () => ({
  schedule: vi.fn(() => Promise.resolve(null)),
}));

const mockedInvoke = vi.mocked(invoke);

const PROFILE = '/tmp/profile';
const RESOURCE_ID = 'agents/architect';

function buildCurrent(overrides: Partial<LockfileEntry> = {}): LockfileEntry {
  return {
    id: RESOURCE_ID,
    kind: 'agent',
    source: 'user',
    version: '1.2.0',
    sha256: 'cur123abcdef',
    sidecarSha256: null,
    files: ['agents/architect.md'],
    installedAt: '2026-04-15T12:00:00Z',
    installedBy: 'tester',
    pack: null,
    drifted: false,
    ...overrides,
  };
}

function buildHistory(
  overrides: Partial<LockfileHistoryEntry> = {},
): LockfileHistoryEntry {
  return {
    version: '1.1.0',
    sha256: 'old456abcdef',
    sidecarSha256: null,
    source: 'marketplace',
    installedAt: '2026-03-01T08:00:00Z',
    cachePaths: ['.weplex/cache/old456/agents/architect.md'],
    ...overrides,
  };
}

describe('HubResourceHistory', () => {
  beforeEach(() => {
    mockedInvoke.mockReset();
  });

  it('renders empty state when nothing is recorded', () => {
    const { container } = render(HubResourceHistory, {
      props: {
        profileConfigDir: PROFILE,
        resourceId: RESOURCE_ID,
        current: null,
        history: [],
      },
    });
    expect(container.querySelector('.history-empty')).not.toBeNull();
    // Empty state has no rows.
    expect(container.querySelectorAll('.history-row')).toHaveLength(0);
  });

  it('renders the current entry plus a row per history item', () => {
    const current = buildCurrent();
    const history = [
      buildHistory({ sha256: 'h1', installedAt: '2026-03-01T00:00:00Z' }),
      buildHistory({ sha256: 'h2', installedAt: '2026-02-01T00:00:00Z' }),
    ];
    const { container } = render(HubResourceHistory, {
      props: {
        profileConfigDir: PROFILE,
        resourceId: RESOURCE_ID,
        current,
        history,
      },
    });

    // 1 current row + 2 history rows = 3 total.
    expect(container.querySelectorAll('.history-row')).toHaveLength(3);
    // Current row carries the explicit modifier.
    expect(container.querySelectorAll('.history-row-current')).toHaveLength(1);
    // Restore buttons only render on history rows, not the current.
    const restoreButtons = container.querySelectorAll(
      '.history-row:not(.history-row-current) button',
    );
    expect(restoreButtons.length).toBe(2);
  });

  it('shows the drift banner when current entry is drifted', () => {
    const current = buildCurrent({ drifted: true });
    const { container } = render(HubResourceHistory, {
      props: {
        profileConfigDir: PROFILE,
        resourceId: RESOURCE_ID,
        current,
        history: [],
      },
    });
    expect(container.querySelector('.drift-banner')).not.toBeNull();
  });

  it('does not show the drift banner for clean current entries', () => {
    const current = buildCurrent({ drifted: false });
    const { container } = render(HubResourceHistory, {
      props: {
        profileConfigDir: PROFILE,
        resourceId: RESOURCE_ID,
        current,
        history: [],
      },
    });
    expect(container.querySelector('.drift-banner')).toBeNull();
  });

  it('clicking Restore opens a confirm dialog and calls restore_resource_version on confirm', async () => {
    const current = buildCurrent();
    const history = [
      buildHistory({ sha256: 'old456abcdef' }),
    ];
    // Stub: read_lockfile (post-restore refresh), scan_profile, and the
    // restore call itself.
    mockedInvoke.mockImplementation(async (cmd) => {
      if (cmd === 'restore_resource_version') {
        return {
          resourceId: RESOURCE_ID,
          previousSha256: current.sha256,
          newSha256: 'old456abcdef',
          historyAdded: true,
          cachePaths: [],
          noOp: false,
        };
      }
      if (cmd === 'read_lockfile') {
        return {
          version: 1,
          generatedBy: 'weplex',
          resources: [],
          history: {},
        };
      }
      if (cmd === 'scan_profile') {
        return {
          profileDir: PROFILE,
          resources: [],
          overall: 'green',
          deepScanRan: false,
          deepScanSkippedReason: null,
          profileFindings: [],
        };
      }
      throw new Error(`unexpected ${cmd}`);
    });

    const { container } = render(HubResourceHistory, {
      props: {
        profileConfigDir: PROFILE,
        resourceId: RESOURCE_ID,
        current,
        history,
      },
    });

    // Click the first Restore button — the only history row's button.
    const restoreBtn = container.querySelector(
      '.history-row:not(.history-row-current) button',
    ) as HTMLButtonElement;
    expect(restoreBtn).toBeTruthy();
    await fireEvent.click(restoreBtn);

    // Confirm dialog appears.
    const dialog = container.querySelector(
      '.restore-dialog',
    ) as HTMLElement | null;
    // Dialog is rendered into the same container by Svelte; it lives in
    // the document body via portal — query via the document since the
    // overlay is fixed-position.
    const confirmDialog = dialog ?? document.querySelector('.restore-dialog');
    expect(confirmDialog).not.toBeNull();

    // The "Restore" button inside the dialog is the primary action.
    const buttons = (confirmDialog as HTMLElement).querySelectorAll('button');
    const confirmBtn = Array.from(buttons).find(
      (b) => b.textContent?.trim() === 'Restore',
    ) as HTMLButtonElement;
    expect(confirmBtn).toBeTruthy();

    await fireEvent.click(confirmBtn);
    // Allow microtasks (restore + refresh).
    await Promise.resolve();
    await Promise.resolve();

    const restoreCall = mockedInvoke.mock.calls.find(
      (c) => c[0] === 'restore_resource_version',
    );
    expect(restoreCall).toBeDefined();
    expect(restoreCall![1]).toEqual({
      profileConfigDir: PROFILE,
      resourceId: RESOURCE_ID,
      targetSha256: 'old456abcdef',
    });
  });

  it('renders relative date strings instead of raw ISO timestamps', () => {
    const aMonthAgo = new Date(
      Date.now() - 30 * 24 * 60 * 60 * 1000,
    ).toISOString();
    const current = buildCurrent({ installedAt: aMonthAgo });
    const { container } = render(HubResourceHistory, {
      props: {
        profileConfigDir: PROFILE,
        resourceId: RESOURCE_ID,
        current,
        history: [],
      },
    });
    const meta = container.querySelector('.row-meta')?.textContent ?? '';
    // Relative formatter should produce "X month(s)/day(s) ago" — never
    // the raw ISO. Loosely assert that a unit word is present.
    expect(meta).toMatch(/(month|day|hour|minute|second|year)s? ago/);
  });
});

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import { invoke } from '@tauri-apps/api/core';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import ImportProfileArchiveDialog from './ImportProfileArchiveDialog.svelte';
import type {
  ArchiveInspection,
  ImportReport,
} from '../../types/lockfile';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(),
  save: vi.fn(),
}));

const mockedInvoke = vi.mocked(invoke);
const mockedOpen = vi.mocked(openDialog);

const TARGET = '/home/u/.profile-target';
const ARCHIVE = '/tmp/some.weplex.profile.tar.gz';

function buildInspection(
  overrides: Partial<ArchiveInspection> = {},
): ArchiveInspection {
  return {
    schemaVersion: 1,
    generatedBy: 'weplex',
    resourceCount: 3,
    conflicts: [],
    ...overrides,
  };
}

beforeEach(() => {
  mockedInvoke.mockReset();
  mockedOpen.mockReset();
});

describe('ImportProfileArchiveDialog', () => {
  it('renders the file-picker stage initially', () => {
    const { container } = render(ImportProfileArchiveDialog, {
      props: {
        targetConfigDir: TARGET,
        open: true,
        onclose: () => {},
      },
    });
    // The "Choose archive" primary action should be visible.
    const buttons = Array.from(
      container.querySelectorAll('button'),
    ) as HTMLButtonElement[];
    const choose = buttons.find((b) =>
      (b.textContent ?? '').includes('Choose archive'),
    );
    expect(choose).toBeTruthy();
  });

  it('renders nothing when open=false', () => {
    const { container } = render(ImportProfileArchiveDialog, {
      props: {
        targetConfigDir: TARGET,
        open: false,
        onclose: () => {},
      },
    });
    // No Modal content should render in this state.
    expect(container.querySelector('.archive-dialog')).toBeNull();
  });

  it('inspects archive then enables Skip / Overwrite for conflicts', async () => {
    const inspection = buildInspection({
      conflicts: [
        {
          resourceId: 'agents/architect',
          existingSha256: 'aaa11111aaaa',
          incomingSha256: 'bbb22222bbbb',
        },
      ],
    });
    mockedOpen.mockResolvedValueOnce(ARCHIVE);
    mockedInvoke.mockImplementation(async (cmd) => {
      if (cmd === 'inspect_profile_archive_cmd') return inspection;
      throw new Error(`unexpected ${cmd}`);
    });

    const { container } = render(ImportProfileArchiveDialog, {
      props: {
        targetConfigDir: TARGET,
        open: true,
        onclose: () => {},
      },
    });

    // Click "Choose archive" → file picker → inspect.
    const chooseBtn = Array.from(
      container.querySelectorAll('button'),
    ).find((b) => (b.textContent ?? '').includes('Choose archive')) as HTMLButtonElement;
    expect(chooseBtn).toBeTruthy();
    await fireEvent.click(chooseBtn);
    // Allow inspect promise + state update.
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();

    expect(mockedOpen).toHaveBeenCalled();
    expect(mockedInvoke).toHaveBeenCalledWith(
      'inspect_profile_archive_cmd',
      { archivePath: ARCHIVE, targetConfigDir: TARGET },
    );

    // Now in the review stage: conflict listed, both Skip + Overwrite buttons.
    expect(container.querySelector('.conflict-banner')).not.toBeNull();
    const reviewButtons = Array.from(
      container.querySelectorAll('button'),
    ) as HTMLButtonElement[];
    const overwriteBtn = reviewButtons.find(
      (b) => (b.textContent ?? '').trim() === 'Overwrite all',
    );
    const skipBtn = reviewButtons.find(
      (b) => (b.textContent ?? '').trim() === 'Skip conflicts',
    );
    expect(overwriteBtn).toBeTruthy();
    expect(skipBtn).toBeTruthy();
  });

  it('applies the policy via import_profile and shows the result', async () => {
    const inspection = buildInspection({
      conflicts: [
        {
          resourceId: 'agents/architect',
          existingSha256: 'aaa11111aaaa',
          incomingSha256: 'bbb22222bbbb',
        },
      ],
    });
    const report: ImportReport = {
      installed: 2,
      overwritten: 1,
      skipped: 0,
    };
    mockedOpen.mockResolvedValueOnce(ARCHIVE);
    mockedInvoke.mockImplementation(async (cmd) => {
      if (cmd === 'inspect_profile_archive_cmd') return inspection;
      if (cmd === 'import_profile') return report;
      throw new Error(`unexpected ${cmd}`);
    });

    const { container } = render(ImportProfileArchiveDialog, {
      props: {
        targetConfigDir: TARGET,
        open: true,
        onclose: () => {},
      },
    });

    const chooseBtn = Array.from(
      container.querySelectorAll('button'),
    ).find((b) => (b.textContent ?? '').includes('Choose archive')) as HTMLButtonElement;
    await fireEvent.click(chooseBtn);
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();

    const overwriteBtn = Array.from(
      container.querySelectorAll('button'),
    ).find(
      (b) => (b.textContent ?? '').trim() === 'Overwrite all',
    ) as HTMLButtonElement;
    expect(overwriteBtn).toBeTruthy();
    await fireEvent.click(overwriteBtn);
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();

    // import_profile was called with overwriteAll policy.
    const importCall = mockedInvoke.mock.calls.find(
      (c) => c[0] === 'import_profile',
    );
    expect(importCall).toBeDefined();
    expect(importCall![1]).toEqual({
      targetConfigDir: TARGET,
      archivePath: ARCHIVE,
      policy: 'overwriteAll',
    });

    // Result stage should show the counts.
    expect(container.querySelector('.result-list')).not.toBeNull();
    const text = (container.querySelector('.result-list')?.textContent ?? '')
      .toLowerCase();
    expect(text).toContain('2');
    expect(text).toContain('1');
    expect(text).toContain('0');
  });

  it('cancelling the file picker stays on the pick stage', async () => {
    mockedOpen.mockResolvedValueOnce(null);

    const { container } = render(ImportProfileArchiveDialog, {
      props: {
        targetConfigDir: TARGET,
        open: true,
        onclose: () => {},
      },
    });

    const chooseBtn = Array.from(
      container.querySelectorAll('button'),
    ).find((b) => (b.textContent ?? '').includes('Choose archive')) as HTMLButtonElement;
    await fireEvent.click(chooseBtn);
    await Promise.resolve();
    await Promise.resolve();

    expect(mockedInvoke).not.toHaveBeenCalled();
    // Still on the pick stage — Choose archive button still visible.
    const stillThere = Array.from(
      container.querySelectorAll('button'),
    ).find((b) => (b.textContent ?? '').includes('Choose archive'));
    expect(stillThere).toBeTruthy();
  });

  it('inspect failure surfaces an error and keeps user on pick stage', async () => {
    mockedOpen.mockResolvedValueOnce(ARCHIVE);
    mockedInvoke.mockImplementation(async (cmd) => {
      if (cmd === 'inspect_profile_archive_cmd') {
        throw new Error('archive corrupt');
      }
      throw new Error(`unexpected ${cmd}`);
    });

    const { container } = render(ImportProfileArchiveDialog, {
      props: {
        targetConfigDir: TARGET,
        open: true,
        onclose: () => {},
      },
    });

    const chooseBtn = Array.from(
      container.querySelectorAll('button'),
    ).find((b) => (b.textContent ?? '').includes('Choose archive')) as HTMLButtonElement;
    await fireEvent.click(chooseBtn);
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();

    // Error message should be rendered.
    const err = container.querySelector('.archive-error');
    expect(err).not.toBeNull();
    expect((err?.textContent ?? '').toLowerCase()).toContain('archive');
  });
});

import { describe, it, expect, beforeEach, afterEach, vi, type Mock } from 'vitest';
import { render, fireEvent, waitFor } from '@testing-library/svelte';
import { invoke } from '@tauri-apps/api/core';
import FederatedPackDetail from './FederatedPackDetail.svelte';
import type { FederatedPackDetailDto } from '../../types/federation';

// ── Mocks ──────────────────────────────────────────────────────────────

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

vi.mock('../../services/federationService', () => ({
  federationService: {
    list: vi.fn(),
    getPack: vi.fn(),
  },
}));

vi.mock('../../utils/compileScheduler', () => ({
  schedule: vi.fn(() => Promise.resolve(null)),
}));

vi.mock('../../stores/profileStore.svelte', () => ({
  profileStore: {
    profiles: [{ id: 'p1', name: 'Work', configDir: '/home/u/.claude' }],
    defaultProfile: { id: 'p1', name: 'Work', configDir: '/home/u/.claude' },
  },
}));
vi.mock('../../stores/lockfileStore.svelte', () => ({
  lockfileStore: { refresh: vi.fn(() => Promise.resolve()) },
}));
vi.mock('../../stores/guardStore.svelte', () => ({
  guardStore: { refresh: vi.fn(() => Promise.resolve()) },
}));
vi.mock('../../stores/settingsStore.svelte', () => ({
  settingsStore: { settings: { agentshieldDeepScan: false } },
}));

import { federationService } from '../../services/federationService';
const mockedGetPack = federationService.getPack as unknown as Mock;
const mockedInvoke = invoke as unknown as Mock;

// Stub global fetch + crypto.subtle for the install path. crypto.subtle is
// available in jsdom from a recent enough version, but make it explicit
// so test failures point at the right place.
const originalFetch = globalThis.fetch;
const originalCrypto = globalThis.crypto;

function mockFetch(content: string) {
  globalThis.fetch = vi.fn(async () => ({
    ok: true,
    status: 200,
    text: async () => content,
  })) as unknown as typeof fetch;
}

async function sha256Hex(s: string): Promise<string> {
  const buf = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(s));
  return Array.from(new Uint8Array(buf))
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('');
}

function buildDetail(over: Partial<FederatedPackDetailDto> = {}): FederatedPackDetailDto {
  return {
    id: 'acme/awesome',
    name: 'Awesome',
    description: 'desc',
    repoUrl: 'https://github.com/acme/awesome',
    stars: 10,
    resourceCount: 1,
    resourceKinds: ['agent'],
    score: { overall: 'green', findingsCount: 0 },
    lastIndexedAt: '2026-04-01T12:00:00Z',
    defaultBranch: 'main',
    commitSha: 'deadbeef',
    resources: [],
    ...over,
  };
}

describe('FederatedPackDetail', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    globalThis.fetch = originalFetch;
    if (originalCrypto) {
      Object.defineProperty(globalThis, 'crypto', {
        value: originalCrypto,
        configurable: true,
      });
    }
  });

  it('shows a loading state while fetching the pack', async () => {
    let resolve: (v: FederatedPackDetailDto | null) => void;
    mockedGetPack.mockReturnValueOnce(
      new Promise((r) => {
        resolve = r;
      }),
    );
    const { findByText } = render(FederatedPackDetail, {
      props: { packId: 'acme/awesome', onclose: () => {} },
    });
    await findByText(/Loading pack details/);
    resolve!(buildDetail());
  });

  it('renders the offline error state when the service returns null', async () => {
    mockedGetPack.mockResolvedValueOnce(null);
    const { findByText } = render(FederatedPackDetail, {
      props: { packId: 'acme/awesome', onclose: () => {} },
    });
    await findByText(/Marketplace is offline/);
  });

  it('renders pack metadata once loaded', async () => {
    mockedGetPack.mockResolvedValueOnce(
      buildDetail({ name: 'Cool Pack', description: 'A super cool pack' }),
    );
    const { findByText } = render(FederatedPackDetail, {
      props: { packId: 'acme/awesome', onclose: () => {} },
    });
    await findByText('Cool Pack');
    await findByText(/A super cool pack/);
  });

  it('requires confirmation before installing a RED pack', async () => {
    mockedGetPack.mockResolvedValueOnce(
      buildDetail({
        score: { overall: 'red', findingsCount: 3 },
        resources: [],
      }),
    );
    const { container, findByText } = render(FederatedPackDetail, {
      props: { packId: 'acme/awesome', onclose: () => {} },
    });
    await findByText(/scored RED/);
    const installBtn = Array.from(container.querySelectorAll('button')).find(
      (b) => (b.textContent ?? '').includes('Install all'),
    ) as HTMLButtonElement;
    expect(installBtn).toBeTruthy();
    expect(installBtn.disabled).toBe(true);
  });

  it('refuses to install a resource whose sha256 disagrees with the server', async () => {
    const body = '# original';
    const realSha = await sha256Hex(body);
    mockedGetPack.mockResolvedValueOnce(
      buildDetail({
        resources: [
          {
            kind: 'agent',
            name: 'architect',
            path: 'agents/architect.md',
            size: body.length,
            // Lie about the sha so the body validation fails.
            sha256: 'b'.repeat(64),
            preview: '# original',
            rawUrl: 'https://example.com/architect.md',
            agentshield: { score: 'green', findings: [] },
          },
        ],
      }),
    );
    mockFetch(body); // Returns body whose actual sha256 is `realSha`

    const { container } = render(FederatedPackDetail, {
      props: { packId: 'acme/awesome', onclose: () => {} },
    });
    // Wait for the resource row to render.
    await waitFor(() => {
      expect(container.querySelector('.fed-resource')).not.toBeNull();
    });
    const installBtn = Array.from(container.querySelectorAll('button')).find(
      (b) => (b.textContent ?? '').includes('Install all'),
    ) as HTMLButtonElement;
    expect(installBtn).toBeTruthy();
    await fireEvent.click(installBtn);
    await waitFor(() => {
      expect(container.textContent).toMatch(/sha256 mismatch/);
    });
    // Critically: the lockfile invoke was never called for this resource.
    expect(mockedInvoke).not.toHaveBeenCalled();
    // Sanity: realSha is a 64-hex string we computed (used to prove the
    // helper works in this environment, not asserted on the page).
    expect(realSha).toMatch(/^[0-9a-f]{64}$/);
  });

  it('passes packId and packCommitSha through to install_marketplace_package', async () => {
    const body = '# arch';
    const sha = await sha256Hex(body);
    mockedGetPack.mockResolvedValueOnce(
      buildDetail({
        id: 'acme/awesome',
        commitSha: 'deadbeef',
        resources: [
          {
            kind: 'agent',
            name: 'arch',
            path: 'agents/arch.md',
            size: body.length,
            sha256: sha,
            preview: body,
            rawUrl: 'https://example.com/arch.md',
            agentshield: { score: 'green', findings: [] },
          },
        ],
      }),
    );
    mockFetch(body);
    mockedInvoke.mockResolvedValue({ noOp: false });

    const { container } = render(FederatedPackDetail, {
      props: { packId: 'acme/awesome', onclose: () => {} },
    });
    await waitFor(() => {
      expect(container.querySelector('.fed-resource')).not.toBeNull();
    });
    const installBtn = Array.from(container.querySelectorAll('button')).find(
      (b) => (b.textContent ?? '').includes('Install all'),
    ) as HTMLButtonElement;
    await fireEvent.click(installBtn);
    await waitFor(() => {
      expect(mockedInvoke).toHaveBeenCalledWith(
        'install_marketplace_package',
        expect.objectContaining({
          targetConfigDir: '/home/u/.claude',
          name: 'arch',
          kind: 'agent',
          pack: 'acme/awesome',
          // I3: forensics — the commit sha that pinned the install must
          // round-trip into the lockfile alongside the pack id.
          packCommitSha: 'deadbeef',
        }),
      );
    });
  });
});


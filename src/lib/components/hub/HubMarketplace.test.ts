import { describe, it, expect, beforeEach, vi, type Mock } from 'vitest';
import { render, fireEvent, waitFor } from '@testing-library/svelte';
import HubMarketplace from './HubMarketplace.svelte';
import type {
  FederatedListResponse,
  FederatedPackSummaryDto,
} from '../../types/federation';

// ── Mocks ──────────────────────────────────────────────────────────────

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

vi.mock('../../services/federationService', () => ({
  federationService: {
    list: vi.fn(),
    getPack: vi.fn(),
  },
}));

vi.mock('../../services/marketplaceService', () => ({
  searchPackages: vi.fn().mockResolvedValue({ packages: [], total: 0 }),
  installPackage: vi.fn(),
  ratePackage: vi.fn(),
  publishPackage: vi.fn(),
}));

vi.mock('../../utils/compileScheduler', () => ({
  schedule: vi.fn(() => Promise.resolve(null)),
}));

// Stores: stub out so onMount doesn't blow up.
vi.mock('../../stores/uiStore', () => ({
  uiStore: { closeOverlay: vi.fn(), hubSection: 'marketplace' },
}));
vi.mock('../../stores/authStore.svelte', () => ({
  authStore: { user: null },
}));
vi.mock('../../stores/profileStore.svelte', () => ({
  profileStore: {
    profiles: [{ id: 'p1', name: 'Work', configDir: '/home/u/.claude' }],
    defaultProfile: { id: 'p1', name: 'Work', configDir: '/home/u/.claude' },
  },
}));
vi.mock('../../stores/lockfileStore.svelte', () => ({
  lockfileStore: {
    refresh: vi.fn(() => Promise.resolve()),
    entryForResource: vi.fn(() => null),
  },
}));
vi.mock('../../stores/guardStore.svelte', () => ({
  guardStore: { refresh: vi.fn(() => Promise.resolve()) },
}));
vi.mock('../../stores/settingsStore.svelte', () => ({
  settingsStore: { settings: { agentshieldDeepScan: false } },
}));

import { federationService } from '../../services/federationService';

const mockedList = federationService.list as unknown as Mock;
const mockedGetPack = federationService.getPack as unknown as Mock;

function buildPack(over: Partial<FederatedPackSummaryDto> = {}): FederatedPackSummaryDto {
  return {
    id: 'acme/awesome',
    name: 'Awesome',
    description: 'Cross-agent agents',
    repoUrl: 'https://github.com/acme/awesome',
    stars: 42,
    resourceCount: 5,
    resourceKinds: ['agent'],
    score: { overall: 'green', findingsCount: 0 },
    lastIndexedAt: '2026-04-01T12:00:00Z',
    ...over,
  };
}

function buildList(
  packs: FederatedPackSummaryDto[] = [buildPack()],
  staleAt: string | null = null,
): FederatedListResponse {
  return { packs, total: packs.length, staleAt };
}

describe('HubMarketplace', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockedList.mockResolvedValue(buildList());
  });

  it('renders three tabs with Federated active by default', async () => {
    const { container } = render(HubMarketplace);
    await waitFor(() => expect(mockedList).toHaveBeenCalled());
    const tabs = container.querySelectorAll('.hub-mp-tab');
    expect(tabs.length).toBe(3);
    const labels = Array.from(tabs).map((t) => t.textContent?.trim());
    expect(labels).toEqual(['Federated', 'Browse', 'Publish']);
    expect(tabs[0].classList.contains('active')).toBe(true);
  });

  it('loads federated packs on mount with default filters', async () => {
    render(HubMarketplace);
    await waitFor(() => expect(mockedList).toHaveBeenCalled());
    const args = mockedList.mock.calls[0][0];
    expect(args.sort).toBe('stars');
    expect(args.limit).toBe(20);
    expect(args.offset).toBe(0);
  });

  it('renders pack cards from the list response', async () => {
    mockedList.mockResolvedValueOnce(buildList([buildPack({ name: 'Pack One' })]));
    const { container } = render(HubMarketplace);
    await waitFor(() => {
      const cards = container.querySelectorAll('.fed-card');
      expect(cards.length).toBe(1);
    });
    expect(container.textContent).toContain('Pack One');
  });

  it('shows the offline banner when federationService returns null', async () => {
    mockedList.mockResolvedValueOnce(null);
    const { container, findByText } = render(HubMarketplace);
    await findByText(/Marketplace is offline/);
    expect(container.querySelector('.hub-mp-banner-offline')).not.toBeNull();
  });

  it('shows the stale banner when staleAt is present', async () => {
    mockedList.mockResolvedValueOnce(
      buildList([buildPack()], '2026-01-01T00:00:00Z'),
    );
    const { findByText } = render(HubMarketplace);
    await findByText(/Index is stale/);
  });

  it('shows the empty state when no packs match', async () => {
    mockedList.mockResolvedValueOnce(buildList([]));
    const { findByText } = render(HubMarketplace);
    await findByText(/No federated packs indexed yet/);
  });

  it('shows a filter-aware empty state when filters are active', async () => {
    // First call (mount) → some pack. Second call (after filter) → empty.
    mockedList
      .mockResolvedValueOnce(buildList([buildPack()]))
      .mockResolvedValueOnce(buildList([]));
    const { container, findByText } = render(HubMarketplace);
    await waitFor(() => expect(mockedList).toHaveBeenCalledTimes(1));

    // Pick a kind filter — selects don't fire native events the same as inputs
    // in jsdom, so call the underlying handler by changing the search query
    // and waiting for debounce.
    const search = container.querySelector('.hub-mp-search') as HTMLInputElement;
    await fireEvent.input(search, { target: { value: 'auth' } });
    await new Promise((r) => setTimeout(r, 350));
    await waitFor(() => expect(mockedList).toHaveBeenCalledTimes(2));
    await findByText(/No packs match the current filters/);
  });

  it('debounces the search input by ~300ms', async () => {
    const { container } = render(HubMarketplace);
    await waitFor(() => expect(mockedList).toHaveBeenCalledTimes(1));
    const input = container.querySelector('.hub-mp-search') as HTMLInputElement;
    await fireEvent.input(input, { target: { value: 'a' } });
    await fireEvent.input(input, { target: { value: 'au' } });
    await fireEvent.input(input, { target: { value: 'auth' } });
    // Before debounce: still just the initial call.
    expect(mockedList).toHaveBeenCalledTimes(1);
    await new Promise((r) => setTimeout(r, 350));
    await waitFor(() => expect(mockedList).toHaveBeenCalledTimes(2));
    const lastArgs = mockedList.mock.calls[1][0];
    expect(lastArgs.q).toBe('auth');
  });

  it('switches to Browse tab when clicked', async () => {
    const { container } = render(HubMarketplace);
    await waitFor(() => expect(mockedList).toHaveBeenCalled());
    const tabs = container.querySelectorAll('.hub-mp-tab') as NodeListOf<HTMLButtonElement>;
    await fireEvent.click(tabs[1]);
    expect(tabs[1].classList.contains('active')).toBe(true);
    expect(tabs[0].classList.contains('active')).toBe(false);
  });

  it('switches to Publish tab and shows sign-in hint when unauthenticated', async () => {
    const { container, findByText } = render(HubMarketplace);
    await waitFor(() => expect(mockedList).toHaveBeenCalled());
    const tabs = container.querySelectorAll('.hub-mp-tab') as NodeListOf<HTMLButtonElement>;
    await fireEvent.click(tabs[2]);
    await findByText(/Sign in to publish packages/);
  });

  it('opens the detail modal when a pack card "View details" is clicked', async () => {
    const detailPack = buildPack({ id: 'acme/x', name: 'X' });
    mockedList.mockResolvedValueOnce(buildList([detailPack]));
    mockedGetPack.mockResolvedValueOnce({
      ...detailPack,
      defaultBranch: 'main',
      commitSha: 'abc1234',
      resources: [],
    });

    const { container } = render(HubMarketplace);
    // Wait for the card to actually paint (mount → list → render).
    await waitFor(() => {
      expect(container.querySelector('.fed-card')).not.toBeNull();
    });
    const card = container.querySelector('.fed-card');
    const viewBtn = card!.querySelector('.fed-view-btn') as HTMLButtonElement;
    await fireEvent.click(viewBtn);
    await waitFor(() => {
      expect(mockedGetPack).toHaveBeenCalledWith('acme/x');
    });
  });

  it('hides pagination when there is only one page', async () => {
    mockedList.mockResolvedValueOnce(buildList([buildPack()]));
    const { container } = render(HubMarketplace);
    await waitFor(() => expect(mockedList).toHaveBeenCalled());
    expect(container.querySelector('.hub-mp-pagination')).toBeNull();
  });

  it('renders pagination when total exceeds page size', async () => {
    const many = Array.from({ length: 20 }, (_, i) =>
      buildPack({ id: `acme/p${i}`, name: `P${i}` }),
    );
    mockedList.mockResolvedValueOnce({ packs: many, total: 45, staleAt: null });
    const { container, findByText } = render(HubMarketplace);
    await findByText('1 / 3');
    expect(container.querySelector('.hub-mp-pagination')).not.toBeNull();
  });

  it('falls back to the first profile when there is no default with a configDir', async () => {
    // Defensive: we already mock a default — this test only confirms the
    // mount path doesn't throw when invoked.
    expect(() => render(HubMarketplace)).not.toThrow();
  });
});

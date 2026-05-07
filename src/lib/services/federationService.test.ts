import { describe, it, expect, beforeEach, vi, type Mock } from 'vitest';
import { federationService } from './federationService';
import { ApiError, NetworkError } from './apiClient';
import type {
  FederatedListResponse,
  FederatedPackDetailDto,
} from '../types/federation';

// Mock the request fn from apiClient — federationService is a thin
// wrapper, so we exercise URL building + error flow without spinning up
// a real fetch mock per test.
vi.mock('./apiClient', async () => {
  const actual = await vi.importActual<typeof import('./apiClient')>(
    './apiClient',
  );
  return {
    ...actual,
    request: vi.fn(),
  };
});

import { request } from './apiClient';
const mockedRequest = request as unknown as Mock;

function buildList(): FederatedListResponse {
  return {
    packs: [
      {
        id: 'acme/awesome',
        name: 'Awesome',
        description: 'A pack',
        repoUrl: 'https://github.com/acme/awesome',
        stars: 42,
        resourceCount: 5,
        resourceKinds: ['agent', 'rule'],
        score: { overall: 'green', findingsCount: 0 },
        lastIndexedAt: '2026-04-01T12:00:00Z',
      },
    ],
    total: 1,
    staleAt: null,
  };
}

function buildDetail(): FederatedPackDetailDto {
  return {
    ...buildList().packs[0],
    defaultBranch: 'main',
    commitSha: 'deadbeef',
    resources: [
      {
        kind: 'agent',
        name: 'architect',
        path: 'agents/architect.md',
        size: 1234,
        sha256: 'a'.repeat(64),
        preview: '# Architect',
        rawUrl: 'https://raw.githubusercontent.com/acme/awesome/main/agents/architect.md',
        agentshield: { score: 'green', findings: [] },
      },
    ],
  };
}

describe('federationService.list', () => {
  beforeEach(() => mockedRequest.mockReset());

  it('omits all query params when no filters are passed', async () => {
    mockedRequest.mockResolvedValueOnce(buildList());
    await federationService.list();
    expect(mockedRequest).toHaveBeenCalledWith(
      '/marketplace/federated',
      { skipAuth: true },
    );
  });

  it('builds a query string from filters in stable order', async () => {
    mockedRequest.mockResolvedValueOnce(buildList());
    await federationService.list({
      q: 'auth',
      kind: 'agent',
      score: 'yellow',
      sort: 'stars',
      limit: 20,
      offset: 40,
    });
    const call = mockedRequest.mock.calls[0];
    const path = call[0] as string;
    expect(path.startsWith('/marketplace/federated?')).toBe(true);
    const search = new URL(`http://x${path}`).searchParams;
    expect(search.get('q')).toBe('auth');
    expect(search.get('kind')).toBe('agent');
    expect(search.get('score')).toBe('yellow');
    expect(search.get('sort')).toBe('stars');
    expect(search.get('limit')).toBe('20');
    expect(search.get('offset')).toBe('40');
  });

  it('drops empty q so the server falls back to its default behaviour', async () => {
    mockedRequest.mockResolvedValueOnce(buildList());
    await federationService.list({ q: '' });
    const path = mockedRequest.mock.calls[0][0] as string;
    // No query string at all when only an empty `q` was passed.
    expect(path).toBe('/marketplace/federated');
  });

  it('returns null on NetworkError so the UI can render an offline state', async () => {
    mockedRequest.mockRejectedValueOnce(new NetworkError('boom'));
    const out = await federationService.list();
    expect(out).toBeNull();
  });

  it('rethrows ApiError so the caller can surface the message', async () => {
    mockedRequest.mockRejectedValueOnce(new ApiError(500, 'kapow'));
    await expect(federationService.list()).rejects.toBeInstanceOf(ApiError);
  });
});

describe('federationService.getPack', () => {
  beforeEach(() => mockedRequest.mockReset());

  it('URL-encodes both packId segments', async () => {
    mockedRequest.mockResolvedValueOnce(buildDetail());
    await federationService.getPack('acme/awesome.repo');
    const path = mockedRequest.mock.calls[0][0] as string;
    expect(path).toBe('/marketplace/federated/acme/awesome.repo');
  });

  it('throws on a malformed packId', async () => {
    await expect(federationService.getPack('no-slash')).rejects.toThrow(
      'invalid packId',
    );
    expect(mockedRequest).not.toHaveBeenCalled();
  });

  it('returns null on NetworkError', async () => {
    mockedRequest.mockRejectedValueOnce(new NetworkError('offline'));
    const out = await federationService.getPack('acme/awesome');
    expect(out).toBeNull();
  });
});

// Federated marketplace API service — thin wrapper over apiClient.request().
//
// The server crawls a curated list of GitHub repos, scores each resource
// with AgentShield, and exposes the result behind /marketplace/federated.
// This module owns the on-the-wire shape (see ../types/federation.ts) and
// returns `null` on transport failure so the Hub can show an "offline"
// state instead of throwing.

import { request, ApiError, NetworkError } from './apiClient';
import type {
  FederatedListResponse,
  FederatedPackDetailDto,
  ListFederatedFilters,
} from '../types/federation';
import { logger } from '../utils/logger';

/**
 * Build the query string for `GET /marketplace/federated`.
 *
 * `URLSearchParams` is used over manual concat so values are encoded
 * once, consistently. Only defined fields are emitted — the server
 * applies defaults for missing keys.
 */
function buildListQuery(filters: ListFederatedFilters | undefined): string {
  const params = new URLSearchParams();
  if (!filters) return '';
  if (filters.q !== undefined && filters.q !== '') params.set('q', filters.q);
  if (filters.kind !== undefined) params.set('kind', filters.kind);
  if (filters.score !== undefined) params.set('score', filters.score);
  if (filters.sort !== undefined) params.set('sort', filters.sort);
  if (filters.limit !== undefined) params.set('limit', String(filters.limit));
  if (filters.offset !== undefined) params.set('offset', String(filters.offset));
  const s = params.toString();
  return s ? `?${s}` : '';
}

export const federationService = {
  /**
   * List federated packs. Returns `null` on network failure so the
   * caller can render an "Marketplace offline" state without try/catch.
   * `ApiError` (4xx/5xx with a body) is still thrown — those are
   * caller-level signals, not transport failures.
   */
  async list(
    filters?: ListFederatedFilters,
  ): Promise<FederatedListResponse | null> {
    try {
      return await request<FederatedListResponse>(
        `/marketplace/federated${buildListQuery(filters)}`,
        { skipAuth: true },
      );
    } catch (e) {
      if (e instanceof NetworkError) {
        logger.info('[federation] marketplace offline — list failed');
        return null;
      }
      if (e instanceof ApiError) {
        // Bubble structured errors so the caller can render the message.
        throw e;
      }
      throw e;
    }
  },

  /**
   * Fetch the full pack detail (resources + AgentShield findings).
   * `packId` is `<owner>/<repo>` lowercase — both segments are
   * URL-encoded so a `repo.with.dots` doesn't get parsed as a path.
   */
  async getPack(packId: string): Promise<FederatedPackDetailDto | null> {
    if (!packId) return null;
    const [owner, repo] = packId.split('/');
    if (!owner || !repo) {
      throw new Error(`invalid packId: ${packId}`);
    }
    const path = `/marketplace/federated/${encodeURIComponent(owner)}/${encodeURIComponent(repo)}`;
    try {
      return await request<FederatedPackDetailDto>(path, { skipAuth: true });
    } catch (e) {
      if (e instanceof NetworkError) {
        logger.info(`[federation] marketplace offline — getPack(${packId}) failed`);
        return null;
      }
      throw e;
    }
  },
};

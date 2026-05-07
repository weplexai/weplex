// Federated marketplace types — mirror the server's
// `FederatedListResponse` / `FederatedPackDetailDto` contract. All API
// payloads are camelCase; types use the same shape verbatim.

import type { GuardVerdict } from './guard';

/** Three-level severity used by AgentShield findings. */
export type FederatedFindingSeverity = 'info' | 'warn' | 'block';

/**
 * Resource kinds surfaced by the federation index. Mirrors the local
 * `ResourceKind` union — duplicated here so federation types stay
 * self-contained and can evolve independently if the server ever
 * supports a kind that the lockfile doesn't.
 */
export type ResourceKindFederated = 'agent' | 'rule' | 'skill' | 'command';

/**
 * Score level used both at the pack level and per-resource.
 * Shared with the AgentShield guard so `ResourceGuardBadge` accepts it.
 */
export type ScoreLevel = GuardVerdict;

export interface AgentShieldFindingDto {
  ruleId: string;
  severity: FederatedFindingSeverity;
  message: string;
  explanation: string;
  /** Snippet from the resource that triggered the rule. */
  snippet: string | null;
  /** Source location pointer (e.g. `line 42`). */
  location: string | null;
  /** 16-hex-char per-instance fingerprint. */
  fingerprint: string;
}

export interface FederatedPackScore {
  overall: ScoreLevel;
  findingsCount: number;
}

export interface FederatedPackSummaryDto {
  /** `<owner>/<repo>` lowercase. URL-encode when used in path params. */
  id: string;
  name: string;
  description: string;
  repoUrl: string;
  stars: number;
  resourceCount: number;
  resourceKinds: ResourceKindFederated[];
  score: FederatedPackScore;
  /** ISO-8601 timestamp of the last successful index run. */
  lastIndexedAt: string;
}

export interface FederatedResourceDto {
  kind: ResourceKindFederated;
  name: string;
  /** Path inside the source repo — purely informational. */
  path: string;
  size: number;
  sha256: string;
  /** First 500 chars of the body, for inline preview without download. */
  preview: string;
  /** Direct raw URL — install fetches from here, then validates sha256. */
  rawUrl: string;
  agentshield: {
    score: ScoreLevel;
    findings: AgentShieldFindingDto[];
  };
}

export interface FederatedPackDetailDto extends FederatedPackSummaryDto {
  defaultBranch: string;
  commitSha: string;
  resources: FederatedResourceDto[];
}

export interface FederatedListResponse {
  packs: FederatedPackSummaryDto[];
  total: number;
  /**
   * ISO-8601 timestamp of the oldest record powering the response — UI
   * surfaces a "stale since X" hint when this is older than ~24h.
   */
  staleAt: string | null;
}

/** Sort keys accepted by the server. */
export type FederatedSort = 'stars' | 'newest' | 'name';

export interface ListFederatedFilters {
  q?: string;
  kind?: ResourceKindFederated;
  score?: ScoreLevel;
  sort?: FederatedSort;
  limit?: number;
  offset?: number;
}

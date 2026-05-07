// Phase 3 lockfile types — mirror src-tauri/src/lockfile.rs.
// All Tauri commands serialize with serde camelCase.

import type { ResourceKind } from './guard';

// Re-export so callers can import lockfile + ResourceKind from one place.
export type { ResourceKind };

// ─── Provenance ─────────────────────────────────────────────────────────

/** Where a resource entered the profile from. */
export type ResourceSource = 'builtin' | 'user' | 'marketplace' | 'imported';

// ─── Lockfile shape ─────────────────────────────────────────────────────

export interface LockfileEntry {
  /** Stable id: `<kind_dir>/<name>`, e.g. `agents/architect`. */
  id: string;
  kind: ResourceKind;
  source: ResourceSource;
  /** Optional version string from the sidecar manifest. */
  version: string | null;
  sha256: string;
  sidecarSha256: string | null;
  /** Files owned by this entry, relative to `profile_config_dir`. */
  files: string[];
  /** ISO-8601 timestamp. */
  installedAt: string;
  installedBy: string;
  /** Set when on-disk bytes diverge from the recorded sha. Never persisted. */
  drifted: boolean;
}

export interface LockfileHistoryEntry {
  version: string | null;
  sha256: string;
  sidecarSha256: string | null;
  source: ResourceSource;
  installedAt: string;
  /** Cache paths relative to `profile_config_dir`. */
  cachePaths: string[];
}

export interface Lockfile {
  version: number;
  generatedBy: string;
  resources: LockfileEntry[];
  /** Keyed by `LockfileEntry.id`. */
  history: Record<string, LockfileHistoryEntry[]>;
}

// ─── Mutation report ────────────────────────────────────────────────────

export interface MutationReport {
  resourceId: string;
  previousSha256: string | null;
  newSha256: string | null;
  historyAdded: boolean;
  cachePaths: string[];
  noOp: boolean;
}

// ─── Export / Import / Migration ────────────────────────────────────────

export interface ExportReport {
  archivePath: string;
  bytes: number;
  resourceCount: number;
}

/**
 * Conflict between an incoming archive entry and the existing on-disk
 * resource. Backend's `ConflictItem` doesn't carry `kind` directly, but
 * `resourceId` is `<kind_dir>/<name>` so the kind can be derived if a
 * caller needs it (UI labels do today).
 */
export interface ConflictItem {
  resourceId: string;
  existingSha256: string;
  incomingSha256: string;
}

export interface ArchiveInspection {
  schemaVersion: number;
  generatedBy: string;
  resourceCount: number;
  conflicts: ConflictItem[];
}

/** Backend ConflictPolicy serializes as camelCase. */
export type ConflictPolicy = 'overwriteAll' | 'skipAll';

export interface ImportReport {
  /** Resources written fresh (no prior on-disk version). */
  installed: number;
  /** Conflicts skipped because policy = skipAll. */
  skipped: number;
  /** Conflicts overwritten because policy = overwriteAll. */
  overwritten: number;
}

export interface MigrationReport {
  alreadyDone: boolean;
  migratedAgents: number;
  migratedSkills: number;
}

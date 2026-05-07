// Thin wrappers around the lockfile module's export/inspect/import/migrate
// Tauri commands. Pure async functions — no state. UI components own
// dialog flow and error toasting.

import { invoke } from '@tauri-apps/api/core';
import type {
  ArchiveInspection,
  ConflictPolicy,
  ExportReport,
  ImportReport,
  MigrationReport,
} from '../types/lockfile';

/**
 * Bundle a profile's resources + lockfile + cache into a portable
 * `.weplex.profile.tar.gz`. The output path must be absolute.
 */
export async function exportProfile(
  profileConfigDir: string,
  outputPath: string,
): Promise<ExportReport> {
  return invoke<ExportReport>('export_profile', {
    profileConfigDir,
    outputPath,
  });
}

/**
 * Read an archive without applying it. If `targetConfigDir` is provided,
 * the inspection compares incoming SHAs against the existing lockfile so
 * the UI can list conflicts upfront.
 */
export async function inspectArchive(
  archivePath: string,
  targetConfigDir?: string,
): Promise<ArchiveInspection> {
  return invoke<ArchiveInspection>('inspect_profile_archive_cmd', {
    archivePath,
    targetConfigDir: targetConfigDir ?? null,
  });
}

/**
 * Apply an archive to a target profile under the given conflict policy.
 * Caller must call `lockfileStore.refresh` afterwards if the UI cares.
 */
export async function importProfile(
  targetConfigDir: string,
  archivePath: string,
  policy: ConflictPolicy,
): Promise<ImportReport> {
  return invoke<ImportReport>('import_profile', {
    targetConfigDir,
    archivePath,
    policy,
  });
}

/**
 * One-shot migration of legacy `~/.weplex/agents/` and `~/.weplex/skills/`
 * into a target profile. Idempotent via flag file — safe to call on
 * every launch.
 */
export async function migrateLegacyWeplex(
  targetConfigDir: string,
): Promise<MigrationReport> {
  return invoke<MigrationReport>('migrate_legacy_weplex', { targetConfigDir });
}

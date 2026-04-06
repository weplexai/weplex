// Settings & config sync service — silent failures, debounced push
//
// Syncs to api.weplex.ai/sync (50KB max JSON blob per user).
// Never syncs: local paths, shell config, credentials, API keys, secrets.

import { request } from './apiClient';
import { settingsStore } from '../stores/settingsStore';
import { spaceStore } from '../stores/spaceStore';
import { profileStore } from '../stores/profileStore';
import { noteStore } from '../stores/noteStore';
import type { AppSettings, Space, Profile, Note, SyncStatus } from '../types';

const DEBOUNCE_MS = 5000;
let pushTimer: ReturnType<typeof setTimeout> | null = null;
let currentStatus: SyncStatus = 'idle';

// ── Safe extraction ──────────────────────────────────────────────────────────

/** Cosmetic settings safe to sync. */
const SYNCABLE_SETTINGS: readonly (keyof AppSettings)[] = [
  'theme',
  'fontFamily',
  'fontSize',
  'sidebarDefault',
] as const;

function extractSettings(settings: AppSettings): Partial<AppSettings> {
  const payload: Partial<AppSettings> = {};
  for (const key of SYNCABLE_SETTINGS) {
    if (key in settings) {
      (payload as any)[key] = settings[key];
    }
  }
  return payload;
}

/** Extract space config safe for sync. Never sync serverId, teamId, createdBy. */
function extractSpaces(spaces: Space[]): Partial<Space>[] {
  return spaces.map((s) => ({
    id: s.id,
    name: s.name,
    color: s.color,
    order: s.order,
    profileId: s.profileId,
    bgColor: s.bgColor,
    type: s.type,
    shared: s.shared,
    // directory intentionally excluded — local path
    // serverId, teamId, createdBy — server-side state, not synced from client
  }));
}

/** Extract profile config safe for sync. Never sync envVars values with secrets. */
function extractProfiles(profiles: Profile[]): Partial<Profile>[] {
  return profiles
    .filter((p) => !p.isDefault) // default profile is system-generated
    .map((p) => ({
      id: p.id,
      name: p.name,
      isDefault: false,
      configDir: p.configDir, // relative path hint, not a secret
      // envVars: sync keys only, not values (may contain API keys)
      envVars: Object.fromEntries(
        Object.entries(p.envVars || {}).map(([k]) => [k, '']),
      ),
      linkedAccount: p.linkedAccount,
    }));
}

/** Extract notes safe for sync (cwd-based notes excluded — local paths). */
function extractNotes(notes: Note[]): Partial<Note>[] {
  return notes
    .filter((n) => n.keyType === 'ssh') // only SSH notes (host-based, not path-based)
    .slice(0, 50) // cap to prevent bloat
    .map((n) => ({
      id: n.id,
      content: n.content.slice(0, 2000), // cap content size
      key: n.key,
      keyType: n.keyType,
      createdAt: n.createdAt,
      updatedAt: n.updatedAt,
    }));
}

// ── Sync blob ────────────────────────────────────────────────────────────────

interface SyncBlob {
  settings?: Partial<AppSettings>;
  spaces?: Partial<Space>[];
  profiles?: Partial<Profile>[];
  notes?: Partial<Note>[];
  version?: number;
}

function buildSyncBlob(): SyncBlob {
  return {
    settings: extractSettings(settingsStore.settings),
    spaces: extractSpaces(spaceStore.spaces),
    profiles: extractProfiles(profileStore.profiles),
    notes: extractNotes(noteStore.notes),
    version: 2, // sync format version
  };
}

function applySyncBlob(blob: SyncBlob): void {
  // Settings
  if (blob.settings && Object.keys(blob.settings).length > 0) {
    const safe: Partial<AppSettings> = {};
    for (const key of SYNCABLE_SETTINGS) {
      if (key in blob.settings) {
        (safe as any)[key] = blob.settings[key];
      }
    }
    if (Object.keys(safe).length > 0) {
      settingsStore.update(safe);
    }
  }

  // Spaces — merge by id, don't overwrite local-only fields
  if (blob.spaces && Array.isArray(blob.spaces)) {
    for (const remoteSpace of blob.spaces) {
      if (!remoteSpace.id) continue;
      const local = spaceStore.spaces.find((s) => s.id === remoteSpace.id);
      if (local) {
        // Update cosmetic fields only
        spaceStore.update(local.id, {
          name: remoteSpace.name || local.name,
          color: remoteSpace.color || local.color,
          bgColor: remoteSpace.bgColor,
          profileId: remoteSpace.profileId,
        });
      }
      // Don't create new spaces from remote — could be from another machine
    }
  }

  // Profiles — merge by id
  if (blob.profiles && Array.isArray(blob.profiles)) {
    for (const remoteProfile of blob.profiles) {
      if (!remoteProfile.id || !remoteProfile.name) continue;
      const local = profileStore.getById(remoteProfile.id);
      if (local) {
        // Update name and linkedAccount only — don't overwrite local envVars
        profileStore.update(local.id, {
          name: remoteProfile.name,
          linkedAccount: remoteProfile.linkedAccount,
        });
      }
    }
  }

  // Notes — merge SSH notes only
  if (blob.notes && Array.isArray(blob.notes)) {
    for (const remoteNote of blob.notes) {
      if (!remoteNote.key || !remoteNote.content) continue;
      const local = noteStore.getByKey(remoteNote.key);
      if (!local) {
        // Create missing SSH notes from remote
        noteStore.upsert(remoteNote.key, 'ssh', remoteNote.content);
      }
    }
  }
}

// ── Public API ───────────────────────────────────────────────────────────────

export const syncService = {
  get status(): SyncStatus {
    return currentStatus;
  },

  /** Pull remote sync blob and apply safe fields. */
  async pull(): Promise<void> {
    currentStatus = 'pulling';
    try {
      const data = await request<SyncBlob>('/sync', { method: 'GET' });
      if (data && typeof data === 'object') {
        applySyncBlob(data);
      }
      currentStatus = 'idle';
    } catch (e) {
      console.warn('[Weplex] Sync pull failed:', e);
      currentStatus = 'error';
    }
  },

  /** Push sync blob to remote. */
  async push(): Promise<void> {
    currentStatus = 'pushing';
    try {
      const blob = buildSyncBlob();
      // Check size before sending (server enforces 50KB)
      const size = new Blob([JSON.stringify(blob)]).size;
      if (size > 48 * 1024) {
        console.warn(`[Weplex] Sync blob too large (${(size / 1024).toFixed(1)}KB), skipping`);
        currentStatus = 'error';
        return;
      }
      await request<void>('/sync', { method: 'PUT', body: blob });
      currentStatus = 'idle';
    } catch (e) {
      console.warn('[Weplex] Sync push failed:', e);
      currentStatus = 'error';
    }
  },

  /** Schedule a debounced push (5s). Resets on repeated calls. */
  schedulePush(): void {
    if (pushTimer) clearTimeout(pushTimer);
    pushTimer = setTimeout(() => {
      pushTimer = null;
      this.push();
    }, DEBOUNCE_MS);
  },
};

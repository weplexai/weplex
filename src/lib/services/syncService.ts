// Settings sync service — silent failures, debounced push

import { request } from './apiClient';
import { settingsStore } from '../stores/settingsStore';
import type { AppSettings, SyncStatus } from '../types';

const DEBOUNCE_MS = 5000;
let pushTimer: ReturnType<typeof setTimeout> | null = null;
let currentStatus: SyncStatus = 'idle';

// Only sync cosmetic preferences — never local paths, shells, or security settings
const SYNCABLE_KEYS: readonly (keyof AppSettings)[] = [
  'theme',
  'fontFamily',
  'fontSize',
  'sidebarDefault',
] as const;

/** Extract only safe cosmetic fields from settings for remote sync. */
function extractSyncPayload(settings: AppSettings): Partial<AppSettings> {
  const payload: Partial<AppSettings> = {};
  for (const key of SYNCABLE_KEYS) {
    if (key in settings) {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (payload as any)[key] = settings[key];
    }
  }
  return payload;
}

/** Filter incoming remote settings to only apply safe cosmetic fields. */
function filterIncomingSettings(remote: Partial<AppSettings>): Partial<AppSettings> {
  const filtered: Partial<AppSettings> = {};
  for (const key of SYNCABLE_KEYS) {
    if (key in remote) {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (filtered as any)[key] = remote[key];
    }
  }
  return filtered;
}

export const syncService = {
  get status(): SyncStatus {
    return currentStatus;
  },

  /** Pull remote settings and apply only safe cosmetic fields. */
  async pull(): Promise<void> {
    currentStatus = 'pulling';
    try {
      const data = await request<{ settings?: Partial<AppSettings> }>('/sync', {
        method: 'GET',
      });
      if (data?.settings && Object.keys(data.settings).length > 0) {
        const safe = filterIncomingSettings(data.settings);
        if (Object.keys(safe).length > 0) {
          settingsStore.update(safe);
        }
      }
      currentStatus = 'idle';
    } catch (e) {
      console.warn('[Weplex] Sync pull failed:', e);
      currentStatus = 'error';
    }
  },

  /** Push only safe cosmetic settings to remote. */
  async push(): Promise<void> {
    currentStatus = 'pushing';
    try {
      await request<void>('/sync', {
        method: 'PUT',
        body: { settings: extractSyncPayload(settingsStore.settings) },
      });
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

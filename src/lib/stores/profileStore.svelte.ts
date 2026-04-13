import { invoke } from '@tauri-apps/api/core';
import type { Profile } from '../types';

const STORAGE_KEY = 'weplex_profiles';

const DEFAULT_PROFILE: Profile = {
  id: 'default',
  name: 'Default',
  isDefault: true,
  configDir: null,
  envVars: {},
};

function loadProfiles(): Profile[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return [DEFAULT_PROFILE];
    const saved: Profile[] = JSON.parse(raw);
    if (!saved.some((p) => p.id === 'default')) {
      saved.unshift(DEFAULT_PROFILE);
    }
    return saved;
  } catch {
    return [DEFAULT_PROFILE];
  }
}

let profiles = $state<Profile[]>(loadProfiles());

function persist() {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(profiles));
  } catch {}
}

export const profileStore = {
  get profiles() {
    return profiles;
  },

  get defaultProfile(): Profile {
    return profiles.find((p) => p.isDefault) || profiles[0];
  },

  getById(id: string): Profile | undefined {
    return profiles.find((p) => p.id === id);
  },

  hasConfigDir(configDir: string): boolean {
    return profiles.some((p) => p.configDir === configDir);
  },

  create(name: string, configDir?: string): Profile {
    const id = `profile-${Date.now()}`;
    const profile: Profile = {
      id,
      name,
      isDefault: false,
      configDir: configDir ?? null,
      envVars: {},
    };
    profiles.push(profile);
    persist();

    // Sync Weplex hooks into the profile's settings.json
    invoke('sync_hooks_for_profile', { configDir: configDir ?? '' }).catch((e) =>
      console.warn('[weplex] failed to sync hooks for new profile:', e),
    );

    return profile;
  },

  update(id: string, patch: Partial<Omit<Profile, 'id' | 'isDefault'>>) {
    const idx = profiles.findIndex((p) => p.id === id);
    if (idx !== -1) {
      profiles[idx] = { ...profiles[idx], ...patch };
      persist();
    }
  },

  remove(id: string) {
    if (id === 'default') return;
    profiles = profiles.filter((p) => p.id !== id);
    persist();
  },

  /** Sync Weplex hooks into all profiles' settings.json. Call on app startup. */
  syncHooks() {
    const configDirs = profiles.filter((p) => p.configDir).map((p) => p.configDir as string);
    invoke('sync_hooks_for_profiles', { profileConfigDirs: configDirs }).catch((e) =>
      console.warn('[weplex] failed to sync hooks to profiles:', e),
    );
  },
};

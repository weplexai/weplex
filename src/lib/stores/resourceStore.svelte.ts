import { invoke } from '@tauri-apps/api/core';
import { profileStore } from './profileStore.svelte';

// ─── Types ──────────────────────────────────────────────────────────────

export type ResourceType = 'agent' | 'rule' | 'skill';
export type ResourceOrigin = 'profile-local' | 'weplex-managed' | 'marketplace';

export interface Resource {
  name: string;
  resourceType: ResourceType;
  origin: ResourceOrigin;
  profileName?: string;
  profileConfigDir?: string;
  filePath: string;
  contentHash: string;
  description: string;
  marketplaceId?: string;
  marketplaceVersion?: string;
  isOutdated: boolean;
}

export interface Conflict {
  name: string;
  resourceType: ResourceType;
  versions: { profileName: string; profileConfigDir: string; contentHash: string }[];
}

export interface DriftEntry {
  name: string;
  resourceType: ResourceType;
  profileName: string;
  profileConfigDir: string;
  expectedHash: string;
  actualHash: string;
}

interface ProfileInfo {
  id: string;
  name: string;
  configDir: string | null;
}

// ─── State ──────────────────────────────────────────────────────────────

let resources = $state<Resource[]>([]);
let conflicts = $state<Conflict[]>([]);
let drifts = $state<DriftEntry[]>([]);
let loading = $state(false);

// ─── Helpers ────────────────────────────────────────────────────────────

function getProfileInfos(): ProfileInfo[] {
  return profileStore.profiles.map((p) => ({
    id: p.id,
    name: p.name,
    configDir: p.configDir,
  }));
}

function getAllConfigDirs(): string[] {
  // Default profile has configDir=null — pass empty string so Rust
  // resolves it to ~/.claude/ (same logic as sync_hooks_for_profile).
  return profileStore.profiles.map((p) => p.configDir ?? '');
}

// ─── Store ──────────────────────────────────────────────────────────────

export const resourceStore = {
  get resources() {
    return resources;
  },
  get loading() {
    return loading;
  },
  get conflicts() {
    return conflicts;
  },
  get drifts() {
    return drifts;
  },

  // Filtered by type
  get agents(): Resource[] {
    return resources.filter((r) => r.resourceType === 'agent');
  },
  get rules(): Resource[] {
    return resources.filter((r) => r.resourceType === 'rule');
  },
  get skills(): Resource[] {
    return resources.filter((r) => r.resourceType === 'skill');
  },

  // Grouped by origin
  get shared(): Resource[] {
    return resources.filter(
      (r) => r.origin === 'weplex-managed' || r.origin === 'marketplace',
    );
  },
  get profileLocal(): Resource[] {
    return resources.filter((r) => r.origin === 'profile-local');
  },
  get marketplace(): Resource[] {
    return resources.filter((r) => r.origin === 'marketplace');
  },

  /** Discover all resources from all profiles + ~/.weplex/. */
  async discover() {
    loading = true;
    try {
      const profiles = getProfileInfos();
      const result = await invoke<{ resources: Resource[]; conflicts: Conflict[] }>(
        'discover_resources',
        { profiles },
      );
      resources = result.resources;
      conflicts = result.conflicts;
    } catch (e) {
      console.warn('[weplex] resource discovery failed:', e);
    } finally {
      loading = false;
    }
  },

  /** Check for locally modified copies (drift). */
  async checkDrift() {
    try {
      const dirs = getAllConfigDirs();
      drifts = await invoke<DriftEntry[]>('check_resource_drift', {
        profileConfigDirs: dirs,
      });
    } catch (e) {
      console.warn('[weplex] drift check failed:', e);
    }
  },

  /** Share a profile-local resource to all profiles. */
  async share(resource: Resource) {
    const dirs = getAllConfigDirs();
    await invoke('share_resource', {
      sourcePath: resource.filePath,
      name: resource.name,
      resourceType: resource.resourceType,
      profileConfigDirs: dirs,
    });
    await this.discover();
  },

  /** Create a new Weplex-managed resource, distributed to all profiles. */
  async create(resourceType: ResourceType, name: string, content: string) {
    const dirs = getAllConfigDirs();
    await invoke('create_shared_resource', {
      name,
      resourceType,
      content,
      profileConfigDirs: dirs,
    });
    await this.discover();
  },

  /** Update a Weplex-managed resource and re-distribute. */
  async update(name: string, resourceType: ResourceType, content: string) {
    await invoke('update_shared_resource', { name, resourceType, content });
    await this.discover();
  },

  /** Delete a Weplex-managed resource and remove all copies. */
  async delete(name: string, resourceType: ResourceType) {
    await invoke('delete_shared_resource', { name, resourceType });
    await this.discover();
  },

  /** Distribute all shared resources to a new profile. */
  async syncToProfile(configDir: string) {
    await invoke('sync_resources_to_profile', { configDir });
  },
};

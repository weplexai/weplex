import { invoke } from '@tauri-apps/api/core';
import { profileStore } from './profileStore.svelte';

// ─── Types ──────────────────────────────────────────────────────────────

export type ResourceType = 'agent' | 'rule' | 'skill';

export interface ResourceProfile {
  profileId: string;
  profileName: string;
  filePath: string;
  contentHash: string;
}

export interface UnifiedResource {
  name: string;
  resourceType: ResourceType;
  description: string;
  profiles: ResourceProfile[];
  differs: boolean;
}

export interface ResourceCounts {
  agents: number;
  rules: number;
  skills: number;
}

interface ProfileInfo {
  id: string;
  name: string;
  configDir: string | null;
}

// ─── State ──────────────────────────────────────────────────────────────

let resources = $state<UnifiedResource[]>([]);
let loading = $state(false);
let error = $state<string | null>(null);

// ─── Helpers ────────────────────────────────────────────────────────────

function getProfileInfos(): ProfileInfo[] {
  return profileStore.profiles.map((p) => ({
    id: p.id,
    name: p.name,
    configDir: p.configDir,
  }));
}

// ─── Store ──────────────────────────────────────────────────────────────

export const resourceStore = {
  get resources() {
    return resources;
  },
  get loading() {
    return loading;
  },
  get error() {
    return error;
  },

  // Filtered by type
  get agents(): UnifiedResource[] {
    return resources.filter((r) => r.resourceType === 'agent');
  },
  get rules(): UnifiedResource[] {
    return resources.filter((r) => r.resourceType === 'rule');
  },
  get skills(): UnifiedResource[] {
    return resources.filter((r) => r.resourceType === 'skill');
  },

  get hasMultipleProfiles(): boolean {
    return profileStore.profiles.length > 1;
  },

  /** Discover all resources from all profiles. */
  async discover() {
    loading = true;
    error = null;
    try {
      const profiles = getProfileInfos();
      resources = await invoke<UnifiedResource[]>('discover_resources', { profiles });
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      error = msg;
      console.warn('[weplex] resource discovery failed:', msg);
    } finally {
      loading = false;
    }
  },

  /** Count resources across profiles (for import dialog). */
  async getCounts(profileInfos?: ProfileInfo[]): Promise<ResourceCounts> {
    const profiles = profileInfos ?? getProfileInfos();
    return invoke<ResourceCounts>('count_profile_resources', { profiles });
  },

  /** Copy a resource from one profile to another. */
  async copyTo(
    sourcePath: string,
    targetConfigDir: string,
    resourceType: ResourceType,
    name: string,
    overwrite = false,
  ): Promise<boolean> {
    const copied = await invoke<boolean>('copy_resource_to_profile', {
      sourcePath,
      targetConfigDir,
      resourceType,
      name,
      overwrite,
    });
    await this.discover();
    return copied;
  },

  /** Copy all resources from existing profiles to a new profile. */
  async copyAllToProfile(targetConfigDir: string): Promise<number> {
    const sourceProfiles = getProfileInfos();
    const count = await invoke<number>('copy_all_resources_to_profile', {
      sourceProfiles,
      targetConfigDir,
    });
    return count;
  },

  /** Create a new resource in a specific profile. */
  async create(
    configDir: string,
    resourceType: ResourceType,
    name: string,
    content: string,
  ): Promise<string> {
    const path = await invoke<string>('create_resource_in_profile', {
      configDir,
      resourceType,
      name,
      content,
    });
    await this.discover();
    return path;
  },

  /** Delete a resource file. */
  async delete(filePath: string) {
    await invoke('delete_resource_file', { filePath });
    await this.discover();
  },
};

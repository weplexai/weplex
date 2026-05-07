import { invoke } from '@tauri-apps/api/core';
import { profileStore } from './profileStore.svelte';
import { settingsStore } from './settingsStore.svelte';
import { schedule as scheduleCompile } from '../utils/compileScheduler';

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

/** Fire-and-forget cross-agent compile for a profile after a mutation. */
function triggerCompile(configDir: string | null | undefined): void {
  if (!configDir) return;
  scheduleCompile(configDir, {
    deepScan: settingsStore.settings.agentshieldDeepScan,
  });
}

/**
 * Find the owning profile for a body file by configDir match.
 * The match must be on a path-segment boundary: `/Users/x/.claude` should
 * own `/Users/x/.claude/agents/foo.md` but NOT `/Users/x/.claude-evil/foo`.
 * A naive `startsWith` accepts the latter (W-7) — append `/` to the
 * configDir before comparing so the prefix has to end at a path boundary.
 */
function profileForFilePath(filePath: string): string | null {
  for (const p of profileStore.profiles) {
    if (!p.configDir) continue;
    const dir = p.configDir.endsWith('/') ? p.configDir : p.configDir + '/';
    if (filePath === p.configDir || filePath.startsWith(dir)) {
      return p.configDir;
    }
  }
  return null;
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
    triggerCompile(targetConfigDir);
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
    triggerCompile(configDir);
    return path;
  },

  /** Delete a resource file. */
  async delete(filePath: string) {
    const owningProfile = profileForFilePath(filePath);
    await invoke('delete_resource_file', { filePath });
    await this.discover();
    triggerCompile(owningProfile);
  },
};

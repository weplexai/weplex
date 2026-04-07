/**
 * Plugin Store — manages installed plugins, activation state, and tray.
 */

import { invoke } from '@tauri-apps/api/core';

// ── Types ────────────────────────────────────────────────────────────────────

export interface PluginSessionType {
  type: string;
  label: string;
  icon: string;
}

export interface PluginManifest {
  id: string;
  name: string;
  version: string;
  icon: string;
  description: string;
  author: string;
  license: string;
  entry: string;
  rust_plugin: string | null;
  permissions: string[];
  min_deck_version: string;
  session_type: PluginSessionType | null;
}

export interface PluginInfo {
  manifest: PluginManifest;
  path: string;
  active: boolean;
  entry_path: string;
}

// ── State ────────────────────────────────────────────────────────────────────

let plugins = $state<PluginInfo[]>([]);
let activePanelId = $state<string | null>(null);
let loadedPlugins = $state<Map<string, any>>(new Map());

// ── Public API ───────────────────────────────────────────────────────────────

export const pluginStore = {
  get plugins() {
    return plugins;
  },

  get activePlugins() {
    return plugins.filter((p) => p.active);
  },

  get activePanelId() {
    return activePanelId;
  },

  /** Load installed plugins from Rust backend. */
  async refresh(): Promise<void> {
    try {
      plugins = await invoke<PluginInfo[]>('list_installed_plugins');
    } catch (e) {
      console.error('[Weplex] Failed to list plugins:', e);
    }
  },

  /** Activate a plugin. */
  async activate(pluginId: string): Promise<void> {
    try {
      await invoke('activate_plugin', { pluginId });
      const idx = plugins.findIndex((p) => p.manifest.id === pluginId);
      if (idx !== -1) {
        plugins[idx] = { ...plugins[idx], active: true };
        plugins = [...plugins];
      }
    } catch (e) {
      console.error('[Weplex] Failed to activate plugin:', e);
    }
  },

  /** Deactivate a plugin. */
  async deactivate(pluginId: string): Promise<void> {
    try {
      await invoke('deactivate_plugin', { pluginId });
      const idx = plugins.findIndex((p) => p.manifest.id === pluginId);
      if (idx !== -1) {
        plugins[idx] = { ...plugins[idx], active: false };
        plugins = [...plugins];
      }
      // Close panel if this plugin's panel was open
      if (activePanelId === pluginId) {
        activePanelId = null;
      }
      // Remove from loaded plugins
      loadedPlugins.delete(pluginId);
      loadedPlugins = new Map(loadedPlugins);
    } catch (e) {
      console.error('[Weplex] Failed to deactivate plugin:', e);
    }
  },

  /** Toggle a plugin's tray panel. */
  togglePanel(pluginId: string): void {
    activePanelId = activePanelId === pluginId ? null : pluginId;
  },

  /** Close any open panel. */
  closePanel(): void {
    activePanelId = null;
  },

  /** Get a loaded plugin module by ID. */
  getLoadedModule(pluginId: string): any {
    return loadedPlugins.get(pluginId);
  },

  /** Store a loaded plugin module. */
  setLoadedModule(pluginId: string, module: any): void {
    loadedPlugins.set(pluginId, module);
    loadedPlugins = new Map(loadedPlugins);
  },

  /** Get all registered session types from active plugins. */
  get sessionTypes(): { pluginId: string; type: string; label: string; icon: string }[] {
    return plugins
      .filter((p) => p.active && p.manifest.session_type)
      .map((p) => ({
        pluginId: p.manifest.id,
        type: p.manifest.session_type!.type,
        label: p.manifest.session_type!.label,
        icon: p.manifest.session_type!.icon,
      }));
  },
};

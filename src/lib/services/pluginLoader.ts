/**
 * Plugin Loader — dynamically loads plugin JS bundles and manages lifecycle.
 *
 * SECURITY NOTE (alpha): Plugins run unsandboxed in the same JS context as
 * Weplex core. They have full access to the Tauri API (window.__TAURI__),
 * localStorage, and all IPC commands. This is a known limitation.
 * Post-alpha: plugins should run in iframe sandbox with postMessage API.
 *
 * Plugins are pre-compiled JS modules in ~/.weplex/plugins/<id>/dist/index.js.
 * They export a DeckPlugin interface that registers session types, tray panels,
 * and pane headers.
 */

import { pluginStore, type PluginInfo } from '../stores/pluginStore.svelte';
import { convertFileSrc } from '@tauri-apps/api/core';
import { logger } from '../utils/logger';

/** Plugin module interface — what a plugin's index.js must export. */
export interface DeckPlugin {
  id: string;
  name: string;
  icon: string;
  version: string;

  /** Optional: register a new session type. */
  sessionType?: {
    type: string;
    label: string;
    icon: string;
    create?(opts: Record<string, unknown>): Promise<Record<string, unknown>>;
    destroy?(sessionId: string): Promise<void>;
    render?(container: HTMLElement, session: Record<string, unknown>): void;
  };

  /** Optional: tray panel Svelte component or render function. */
  trayPanel?: {
    render?(container: HTMLElement): void;
    width?: number;
  };

  /** Lifecycle */
  onActivate?(): Promise<void>;
  onDeactivate?(): Promise<void>;
}

/** Load and activate a plugin from its entry path. */
export async function loadPlugin(plugin: PluginInfo): Promise<DeckPlugin | null> {
  try {
    // Convert filesystem path to Tauri asset URL
    const assetUrl = convertFileSrc(plugin.entry_path);

    // Dynamic import of the plugin's JS bundle
    const module = await import(/* @vite-ignore */ assetUrl);
    const pluginModule: DeckPlugin = module.default || module;

    if (!pluginModule.id) {
      console.error(`[Weplex] Plugin at ${plugin.entry_path} has no 'id' export`);
      return null;
    }

    // Call onActivate lifecycle hook
    if (pluginModule.onActivate) {
      await pluginModule.onActivate();
    }

    // Store loaded module
    pluginStore.setLoadedModule(plugin.manifest.id, pluginModule);

    logger.info(`Plugin loaded: ${pluginModule.name} v${pluginModule.version}`);
    return pluginModule;
  } catch (e) {
    console.error(`[Weplex] Failed to load plugin ${plugin.manifest.id}:`, e);
    return null;
  }
}

/** Deactivate and unload a plugin. */
export async function unloadPlugin(pluginId: string): Promise<void> {
  const module = pluginStore.getLoadedModule(pluginId);
  if (module?.onDeactivate) {
    try {
      await module.onDeactivate();
    } catch (e) {
      console.error(`[Weplex] Plugin ${pluginId} deactivation error:`, e);
    }
  }
}

/** Load all active plugins on startup (parallel). */
export async function loadActivePlugins(): Promise<void> {
  const activePlugins = pluginStore.activePlugins;
  await Promise.allSettled(activePlugins.map((p) => loadPlugin(p)));
}

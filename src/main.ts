import { recoverStores, STORE_KEYS } from './lib/utils/durablePersist';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { invoke } from '@tauri-apps/api/core';
import { initAnalytics } from './lib/services/analytics';
import { featureFlags } from './lib/stores/featureFlagsStore.svelte';

async function init() {
  // PostHog: init early so feature flags load in parallel with other startup work
  initAnalytics();
  // Don't await — flags resolve asynchronously and UI reacts via the store
  featureFlags.bootstrap();

  // Recover stores from file backup before loading any Svelte stores
  await recoverStores();

  // Dev-only screenshot demo seed. Runs BEFORE store modules are imported
  // (they read localStorage at top-level), so the seeded spaces/sessions
  // are what the stores load. No-op in production — DEV flag is compile-time.
  if (import.meta.env.DEV && import.meta.env.VITE_WEPLEX_DEMO === '1') {
    const { seedDemoData } = await import('./lib/dev/seedDemo');
    seedDemoData();
  }

  // Dynamic import so store modules load AFTER recovery
  const { mount } = await import('svelte');
  const { default: App } = await import('./App.svelte');

  mount(App, {
    target: document.getElementById('app')!,
  });

  // Flush all stores to file on window close (Tauri event, not JS beforeunload).
  // Guarantees file backup is up-to-date even if debounced writes are pending.
  // Must call preventDefault() — otherwise Tauri waits on the async handler
  // and the window appears frozen until persist completes.
  const appWindow = getCurrentWindow();
  let isClosing = false;
  appWindow.onCloseRequested(async (event) => {
    event.preventDefault();
    if (isClosing) return;
    isClosing = true;

    const PERSIST_TIMEOUT_MS = 2000;
    try {
      type Entry = { key: (typeof STORE_KEYS)[number]; value: string };
      const persists = STORE_KEYS.map((key) => ({ key, value: localStorage.getItem(key) }))
        .filter((e): e is Entry => e.value !== null && e.value !== '')
        .map(({ key, value }) => invoke('persist_store', { key, value }));
      await Promise.race([
        Promise.all(persists),
        new Promise((_, reject) =>
          setTimeout(() => reject(new Error('persist timeout')), PERSIST_TIMEOUT_MS),
        ),
      ]);
    } catch (e) {
      console.warn('persist on close failed:', e);
    }

    try {
      await appWindow.destroy();
    } catch (e) {
      console.error('window destroy failed, falling back to close:', e);
      try {
        await appWindow.close();
      } catch {
        /* swallow — process exit is the last resort */
      }
    }
  });
}

init();

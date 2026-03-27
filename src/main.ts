import { recoverStores, STORE_KEYS } from './lib/utils/durablePersist';
import { getCurrentWindow } from '@tauri-apps/api/window';

async function init() {
  // Recover stores from file backup before loading any Svelte stores
  await recoverStores();

  // Dynamic import so store modules load AFTER recovery
  const { mount } = await import('svelte');
  const { default: App } = await import('./App.svelte');

  mount(App, {
    target: document.getElementById('app')!,
  });

  // Flush all stores to file on window close (Tauri event, not JS beforeunload).
  // Guarantees file backup is up-to-date even if debounced writes are pending.
  const appWindow = getCurrentWindow();
  appWindow.onCloseRequested(async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    await Promise.all(
      STORE_KEYS.map((key) => ({ key, value: localStorage.getItem(key) }))
        .filter((e): e is { key: string; value: string } => e.value !== null && e.value !== '')
        .map(({ key, value }) => invoke('persist_store', { key, value })),
    );
  });
}

init();

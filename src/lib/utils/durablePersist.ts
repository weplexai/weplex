import { invoke } from '@tauri-apps/api/core';

/** All store keys that participate in durable persistence. Single source of truth. */
export const STORE_KEYS = [
  'weplex_sessions',
  'weplex_active_session',
  'weplex_spaces',
  'weplex_active_space',
  'weplex_space_sessions',
  'weplex_splits',
  'weplex_splits_focus',
  'weplex_folders',
  'weplex_notes',
] as const;

// Debounce-only keys. Everything else writes to file immediately.
const DEBOUNCED_KEYS = new Set(['weplex_splits', 'weplex_splits_focus']);

const DEBOUNCE_MS = 500;
const timers: Record<string, ReturnType<typeof setTimeout>> = {};

/**
 * Save to localStorage (sync) + file backup (immediate or debounced).
 * Critical stores write to file immediately. Layout stores debounce.
 */
export function durableSave(key: string, value: string): void {
  localStorage.setItem(key, value);

  if (DEBOUNCED_KEYS.has(key)) {
    clearTimeout(timers[key]);
    timers[key] = setTimeout(() => {
      invoke('persist_store', { key, value }).catch(console.error);
    }, DEBOUNCE_MS);
  } else {
    clearTimeout(timers[key]);
    invoke('persist_store', { key, value }).catch(console.error);
  }
}

/**
 * Remove a key from both localStorage and file backup.
 * Use instead of bare localStorage.removeItem() to keep file in sync.
 */
export function durableRemove(key: string): void {
  localStorage.removeItem(key);
  // Write empty string as tombstone — load_store treats empty as absent
  invoke('persist_store', { key, value: '' }).catch(console.error);
}

/** Count items in a JSON value. Returns -1 if not parseable. */
function countItems(json: string): number {
  try {
    const data = JSON.parse(json);
    if (Array.isArray(data)) return data.length;
    if (typeof data === 'object' && data !== null) return Object.keys(data).length;
    return 0;
  } catch {
    return -1;
  }
}

/**
 * Run BEFORE app mount. For each store key, compare localStorage vs file backup.
 * If file has more items, localStorage was corrupted by a crash — recover from file.
 * For scalar keys (active_session, active_space): recover if localStorage is empty but file has data.
 */
export async function recoverStores(): Promise<void> {
  // SECURITY: Before recovering, verify file data belongs to the same user.
  // Prevents cross-account data leakage when users switch on the same machine.
  try {
    const fileEmail = await invoke<string | null>('load_store', { key: 'weplex_last_user_email' });
    const localEmail = localStorage.getItem('weplex_last_user_email');
    if (fileEmail && localEmail && fileEmail !== localEmail) {
      console.warn(`[Weplex] File backup belongs to different user (${fileEmail} vs ${localEmail}). Skipping recovery.`);
      // Clear stale file data
      for (const key of STORE_KEYS) {
        invoke('persist_store', { key, value: '' }).catch(() => {});
      }
      invoke('persist_store', { key: 'weplex_last_user_email', value: localEmail }).catch(() => {});
      return;
    }
    if (fileEmail && !localEmail) {
      // File has user email but localStorage doesn't — recover the email
      localStorage.setItem('weplex_last_user_email', fileEmail);
    }
  } catch (e) {
    console.error('[Weplex] User email check failed:', e);
  }

  for (const key of STORE_KEYS) {
    try {
      const fileValue = await invoke<string | null>('load_store', { key });
      const localValue = localStorage.getItem(key);

      if (!fileValue) {
        // No file backup yet — seed it from localStorage
        if (localValue) {
          invoke('persist_store', { key, value: localValue }).catch(console.error);
        }
        continue;
      }

      // Validate file JSON before using it
      const fileCount = countItems(fileValue);

      if (!localValue) {
        // localStorage empty but file has data — recover
        if (fileCount === -1) {
          // File has a scalar value (e.g., active session ID "7") — still valid
          console.warn(`[Weplex] Recovered ${key} from file (localStorage was empty)`);
        } else {
          console.warn(
            `[Weplex] Recovered ${key} from file (localStorage was empty, ${fileCount} items)`,
          );
        }
        localStorage.setItem(key, fileValue);
        continue;
      }

      const localCount = countItems(localValue);
      if (localCount === -1 && fileCount >= 0) {
        // localStorage has garbage but file has valid collection — recover
        console.warn(`[Weplex] Recovered ${key} from file (localStorage was corrupt)`);
        localStorage.setItem(key, fileValue);
        continue;
      }

      // Both valid collections — if file has more items, localStorage lost data
      if (fileCount >= 0 && localCount >= 0 && fileCount > localCount) {
        console.warn(
          `[Weplex] Recovered ${key} from file (localStorage: ${localCount} items, file: ${fileCount} items)`,
        );
        localStorage.setItem(key, fileValue);
      }
    } catch (e) {
      console.error(`[Weplex] Recovery check failed for ${key}:`, e);
    }
  }
}

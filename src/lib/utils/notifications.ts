/**
 * Native notification wrapper for Tauri.
 *
 * NOTE: Requires @tauri-apps/plugin-notification to be installed in both
 * Cargo.toml and package.json. Until then, falls back to console.log.
 */

let notificationAvailable: boolean | null = null;

async function ensurePermission(): Promise<boolean> {
  try {
    const mod = await import('@tauri-apps/plugin-notification');
    let granted = await mod.isPermissionGranted();
    if (!granted) {
      const result = await mod.requestPermission();
      granted = result === 'granted';
    }
    return granted;
  } catch {
    return false;
  }
}

/**
 * Show a native OS notification. Silently falls back to console if the
 * Tauri notification plugin is not available.
 */
export async function showNativeNotification(title: string, body: string): Promise<void> {
  // Cache availability check
  if (notificationAvailable === null) {
    notificationAvailable = await ensurePermission();
  }

  if (!notificationAvailable) {
    if (import.meta.env.DEV) console.log(`[Notification] ${title}: ${body}`);
    return;
  }

  try {
    const { sendNotification } = await import('@tauri-apps/plugin-notification');
    sendNotification({ title, body });
  } catch {
    if (import.meta.env.DEV) console.log(`[Notification] ${title}: ${body}`);
  }
}

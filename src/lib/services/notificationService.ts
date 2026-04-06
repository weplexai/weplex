// OS notification service — sends native notifications when agent events occur.
// Only fires when the app window is not focused.

import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from '@tauri-apps/plugin-notification';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { listen } from '@tauri-apps/api/event';
import type { HookEventPayload } from '../types';

let permissionGranted = false;
let windowFocused = true;
let enabled = true;

/** Initialize notification service — request permission and track focus. */
export async function initNotifications(): Promise<void> {
  try {
    permissionGranted = await isPermissionGranted();
    if (!permissionGranted) {
      const result = await requestPermission();
      permissionGranted = result === 'granted';
    }
  } catch {
    // Notifications not available (e.g., Linux without dbus)
    permissionGranted = false;
  }

  // Track window focus state
  const appWindow = getCurrentWindow();
  appWindow.onFocusChanged(({ payload: focused }) => {
    windowFocused = focused;
  });

  // Listen for hook events
  listen<HookEventPayload>('hook-event', (event) => {
    if (!enabled || windowFocused || !permissionGranted) return;
    handleHookEvent(event.payload);
  });
}

/** Enable/disable notifications. */
export function setNotificationsEnabled(value: boolean): void {
  enabled = value;
}

/** Handle a hook event and send notification if appropriate. */
function handleHookEvent(event: HookEventPayload): void {
  if (event.event_type === 'stop') {
    notify('Agent finished', `Session #${event.session_id} completed its task.`);
  }
}

/** Send an OS notification. */
function notify(title: string, body: string): void {
  try {
    sendNotification({ title, body });
  } catch {
    // Silently fail
  }
}

/** Notify that an agent is waiting for user input. */
export function notifyWaitingForInput(sessionName: string): void {
  if (!enabled || windowFocused || !permissionGranted) return;
  notify('Agent waiting', `${sessionName} needs your input.`);
}

/** Notify that an agent encountered an error. */
export function notifyError(sessionName: string, errorMessage?: string): void {
  if (!enabled || windowFocused || !permissionGranted) return;
  notify('Agent error', errorMessage
    ? `${sessionName}: ${errorMessage.slice(0, 100)}`
    : `${sessionName} encountered an error.`);
}

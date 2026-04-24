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
import { sessionStore } from '../stores/sessionStore.svelte';

let permissionGranted = false;
let windowFocused = true;
let enabled = true;

// Stuck detection: track last activity per session
const lastActivityMap = new Map<number, number>();
const STUCK_THRESHOLD_MS = 5 * 60_000; // 5 minutes without output = stuck
let stuckCheckInterval: ReturnType<typeof setInterval> | null = null;

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
    // Track activity for stuck detection (always, regardless of focus)
    lastActivityMap.set(event.payload.session_id, Date.now());

    if (!enabled || windowFocused || !permissionGranted) return;
    handleHookEvent(event.payload);
  });

  // Start stuck detection check (guard against double init)
  if (stuckCheckInterval) clearInterval(stuckCheckInterval);
  stuckCheckInterval = setInterval(checkStuckSessions, 60_000);
}

/** Enable/disable notifications. */
export function setNotificationsEnabled(value: boolean): void {
  enabled = value;
}

/** Get session display name. */
function getSessionName(sessionId: number): string {
  const session = sessionStore.sessions.find((s) => s.id === sessionId);
  return session?.name || `Session #${sessionId}`;
}

/** Handle a hook event and send notification if appropriate. */
function handleHookEvent(event: HookEventPayload): void {
  if (event.event_type === 'stop') {
    notify('Agent finished', `${getSessionName(event.session_id)} completed its task.`);
  }
}

/** Check for sessions that appear stuck (active for too long without hook events). */
function checkStuckSessions(): void {
  if (!enabled || windowFocused || !permissionGranted) return;

  const now = Date.now();
  for (const session of sessionStore.sessions) {
    if (session.status !== 'active' && session.status !== 'thinking') continue;

    const lastActivity = lastActivityMap.get(session.id);
    if (!lastActivity) continue;

    const elapsed = now - lastActivity;
    if (elapsed >= STUCK_THRESHOLD_MS) {
      notify('Agent may be stuck', `${session.name} has been running for ${Math.round(elapsed / 60_000)}min without progress.`);
      // Reset to avoid repeated notifications — next activity will re-add
      lastActivityMap.delete(session.id);
    }
  }
}

/** Redact potential secrets from error messages. */
export function redactSecrets(text: string): string {
  return text
    .replace(/sk-[a-zA-Z0-9_-]{10,}/g, 'sk-***')
    .replace(/key[=:]\s*\S+/gi, 'key=***')
    .replace(/token[=:]\s*\S+/gi, 'token=***')
    .replace(/Bearer\s+\S+/g, 'Bearer ***')
    .replace(/password[=:]\s*\S+/gi, 'password=***')
    .replace(/secret[=:]\s*\S+/gi, 'secret=***');
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
    ? `${sessionName}: ${redactSecrets(errorMessage.slice(0, 100))}`
    : `${sessionName} encountered an error.`);
}

/** Track PTY output as activity (call from TerminalView). */
export function trackSessionActivity(sessionId: number): void {
  lastActivityMap.set(sessionId, Date.now());
}

/** Clean up when session is closed. */
export function clearSessionTracking(sessionId: number): void {
  lastActivityMap.delete(sessionId);
}

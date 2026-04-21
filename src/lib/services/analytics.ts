// PostHog analytics + feature flags
//
// Initialization is lazy: if VITE_POSTHOG_KEY is not set, we operate in a no-op
// mode so dev builds and CI don't need credentials to run. In production, the key
// is baked into the bundle at build time via Vite env.
//
// Feature flag values are cached on the instance after the bootstrap call completes.
// Consumers should `await analytics.ready()` once at app startup before reading flags,
// or read via `getFlag(name, fallback)` which returns the fallback until bootstrap.

import posthog from 'posthog-js';

const POSTHOG_KEY = import.meta.env.VITE_POSTHOG_KEY as string | undefined;
const POSTHOG_HOST = (import.meta.env.VITE_POSTHOG_HOST as string) || 'https://us.i.posthog.com';

/**
 * localStorage key for the user's opt-out choice. Persisted outside PostHog
 * so we can honor it before the SDK even initializes (first load after
 * toggle, no race with SDK bootstrap).
 */
const OPT_OUT_KEY = 'weplex_analytics_opt_out';

export type FeatureFlag =
  | 'feature_marketplace'
  | 'feature_commands'
  | 'feature_resources';

let initialized = false;
let enabled = false;
let bootstrapPromise: Promise<void> | null = null;
let flagsReady = false;

function isOptedOut(): boolean {
  try {
    return localStorage.getItem(OPT_OUT_KEY) === '1';
  } catch {
    return false;
  }
}

/** Initialize PostHog. Safe to call multiple times — second call is a no-op. */
export function initAnalytics(): void {
  if (initialized) return;
  initialized = true;

  if (!POSTHOG_KEY) {
    // No key → analytics disabled (dev, CI). Feature flags return fallbacks.
    return;
  }

  try {
    posthog.init(POSTHOG_KEY, {
      api_host: POSTHOG_HOST,
      // Desktop app: we don't want PostHog's session recording, autocapture,
      // or pageview tracking — we emit events explicitly.
      autocapture: false,
      capture_pageview: false,
      capture_pageleave: false,
      disable_session_recording: true,
      persistence: 'localStorage',
      // If user previously opted out, start the SDK in opted-out state so
      // nothing fires between init() and the loaded callback.
      opt_out_capturing_by_default: isOptedOut(),
      loaded: () => {
        flagsReady = true;
      },
    });
    enabled = true;
  } catch (e) {
    console.warn('[analytics] PostHog init failed:', e);
  }
}

/** User's current analytics preference. True = data collection is OFF. */
export function isAnalyticsOptedOut(): boolean {
  return isOptedOut();
}

/**
 * Turn analytics off: persist the choice, tell PostHog to stop capturing,
 * and clear the stored distinct ID so no identifier survives.
 */
export function optOutAnalytics(): void {
  try {
    localStorage.setItem(OPT_OUT_KEY, '1');
  } catch {
    // ignore — the SDK call below is still the primary enforcement
  }
  if (!enabled) return;
  try {
    posthog.opt_out_capturing();
    posthog.reset();
  } catch (e) {
    console.warn('[analytics] opt-out failed:', e);
  }
}

/** Turn analytics back on. */
export function optInAnalytics(): void {
  try {
    localStorage.removeItem(OPT_OUT_KEY);
  } catch {
    // ignore
  }
  if (!enabled) return;
  try {
    posthog.opt_in_capturing();
  } catch (e) {
    console.warn('[analytics] opt-in failed:', e);
  }
}

/** Wait for feature flags to be loaded from PostHog (or resolve immediately if disabled). */
export function readyFlags(): Promise<void> {
  if (!enabled) return Promise.resolve();
  if (flagsReady) return Promise.resolve();
  if (bootstrapPromise) return bootstrapPromise;

  bootstrapPromise = new Promise<void>((resolve) => {
    // PostHog's `onFeatureFlags` fires on initial load and on every update
    const unsub = posthog.onFeatureFlags(() => {
      flagsReady = true;
      unsub();
      resolve();
    });
    // Safety timeout: if PostHog can't reach the network, don't block UI forever
    setTimeout(() => {
      if (!flagsReady) {
        flagsReady = true;
        resolve();
      }
    }, 3000);
  });
  return bootstrapPromise;
}

/** Identify the current user. Call on login and when profile loads. */
export function identifyUser(userId: string, email?: string): void {
  if (!enabled) return;
  try {
    posthog.identify(userId, email ? { email } : undefined);
  } catch (e) {
    console.warn('[analytics] identify failed:', e);
  }
}

/** Clear user identification (call on logout). */
export function resetAnalytics(): void {
  if (!enabled) return;
  try {
    posthog.reset();
  } catch (e) {
    console.warn('[analytics] reset failed:', e);
  }
}

/** Fire a custom event. Fails silently if analytics is disabled. */
export function capture(event: string, properties?: Record<string, unknown>): void {
  if (!enabled) return;
  try {
    posthog.capture(event, properties);
  } catch (e) {
    console.warn('[analytics] capture failed:', e);
  }
}

/**
 * Read a feature flag. Returns `fallback` if analytics is disabled or flags
 * haven't loaded yet. Use `readyFlags()` at startup to wait for initial load.
 */
export function getFlag(flag: FeatureFlag, fallback = false): boolean {
  if (!enabled || !flagsReady) return fallback;
  try {
    return posthog.isFeatureEnabled(flag) === true;
  } catch {
    return fallback;
  }
}

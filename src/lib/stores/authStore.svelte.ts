import type { AuthUser, AuthTokens } from '../types';
import { initApiClient, resolveApiEndpoint, getBaseUrl } from '../services/apiClient';
import { authService } from '../services/authService';
import { syncService } from '../services/syncService';
import { invoke } from '@tauri-apps/api/core';
import { teamStore } from './teamStore.svelte';
import { collabPipelineStore } from './collabPipelineStore.svelte';
import { presenceStore } from './presenceStore.svelte';
import { chatStore } from './chatStore.svelte';
import { pipelineWsService } from '../services/pipelineWsService';

const KEYCHAIN_KEY = 'auth_tokens';
const FILE_BACKUP_KEY = 'weplex_auth_tokens';
const LAST_USER_KEY = 'weplex_last_user_email';

/** Extract email from JWT access token without verification (client-side check only). */
function extractEmailFromJwt(token: string): string | null {
  try {
    const payload = JSON.parse(atob(token.split('.')[1]));
    return payload.email || null;
  } catch {
    return null;
  }
}

/** Save last authenticated user email for cross-account protection. */
function saveLastUserEmail(email: string): void {
  localStorage.setItem(LAST_USER_KEY, email);
  // Also persist to file backup so recoverStores() can verify ownership
  invoke('persist_store', { key: LAST_USER_KEY, value: email }).catch(() => {});
}

// ── State ──────────────────────────────────────────────────────────────────

let user = $state<AuthUser | null>(null);
let tokens = $state<AuthTokens | null>(null);
let loading = $state(false);
let error = $state<string | null>(null);
let focusRetryCleanup: (() => void) | null = null;

// ── Persistence helpers (OS keychain) ──────────────────────────────────────

async function keychainLoadTokens(): Promise<AuthTokens | null> {
  try {
    const raw = await invoke<string | null>('keychain_load', { key: KEYCHAIN_KEY });
    if (!raw) return null;
    return JSON.parse(raw) as AuthTokens;
  } catch {
    return null;
  }
}

async function keychainSaveTokens(t: AuthTokens): Promise<void> {
  await invoke('keychain_save', { key: KEYCHAIN_KEY, value: JSON.stringify(t) });
}

async function keychainDeleteTokens(): Promise<void> {
  await invoke('keychain_delete', { key: KEYCHAIN_KEY });
}

// ── Persistence helpers (encrypted file backup) ──────────────────────────
// Encrypted backup uses AES-256-GCM with a machine-derived key (hostname + username).
// Primary storage is OS keychain; encrypted file is fallback for keychain failures.
// Files stored in appDataDir/secure/ with 0600 permissions.

async function fileSaveTokens(t: AuthTokens): Promise<void> {
  try {
    await invoke('secure_store_save', { key: FILE_BACKUP_KEY, value: JSON.stringify(t) });
  } catch (e) {
    console.warn('[auth] Encrypted backup save failed:', e);
  }
}

async function fileLoadTokens(): Promise<AuthTokens | null> {
  try {
    const raw = await invoke<string | null>('secure_store_load', { key: FILE_BACKUP_KEY });
    if (!raw) return null;
    const decoded = JSON.parse(raw) as AuthTokens;
    if (decoded.accessToken && decoded.refreshToken) return decoded;
    return null;
  } catch {
    return null;
  }
}

async function fileDeleteTokens(): Promise<void> {
  try {
    await invoke('secure_store_delete', { key: FILE_BACKUP_KEY });
  } catch (e) {
    console.warn('[auth] Encrypted backup delete failed:', e);
  }
}

// ── Wire up apiClient token access ─────────────────────────────────────────

initApiClient(
  () => tokens,
  (newTokens) => {
    tokens = newTokens;
    if (newTokens) {
      keychainSaveTokens(newTokens).catch(console.error);
      fileSaveTokens(newTokens).catch(console.error);
    } else {
      keychainDeleteTokens().catch(console.error);
      fileDeleteTokens().catch(console.error);
    }
  },
);

// ── Store ──────────────────────────────────────────────────────────────────

export const authStore = {
  get user() {
    return user;
  },
  get loading() {
    return loading;
  },
  get error() {
    return error;
  },
  get isAuthenticated() {
    // Consider authenticated if we have tokens, even if profile is not yet loaded
    // (e.g., server is temporarily unreachable after app restart)
    return tokens !== null;
  },
  get syncStatus() {
    return syncService.status;
  },

  /** Load persisted tokens, fetch profile, trigger sync. Call once at app start. */
  async init(): Promise<void> {
    // Resolve best API endpoint (try .ai first, fallback to .xyz)
    await resolveApiEndpoint();

    // Try keychain first, fall back to file backup
    let saved = await keychainLoadTokens();
    if (!saved) {
      saved = await fileLoadTokens();
      if (saved) {
        await keychainSaveTokens(saved).catch(() => {});
        console.log('[auth] Restored tokens from file backup');
      }
    }
    if (!saved) return;

    // SECURITY: Verify loaded tokens belong to the last logged-in user.
    // Prevents cross-account contamination if tokens from another account
    // somehow ended up in keychain or encrypted file backup.
    const lastEmail = localStorage.getItem('weplex_last_user_email');
    const tokenEmail = extractEmailFromJwt(saved.accessToken);
    if (lastEmail && tokenEmail && tokenEmail !== lastEmail) {
      console.warn(`[auth] Token mismatch: expected ${lastEmail}, got ${tokenEmail}. Discarding foreign tokens.`);
      await keychainDeleteTokens().catch(() => {});
      await fileDeleteTokens();
      return;
    }
    // If no lastEmail yet (first login on this device), extract from JWT and record it
    // so that subsequent loads can validate ownership
    if (!lastEmail && tokenEmail) {
      saveLastUserEmail(tokenEmail);
    }

    tokens = saved;
    try {
      user = await authService.getProfile();
      if (user?.email) {
        // Double-check: if we had a last_user_email, verify server confirmed the same user
        // This catches cases where JWT check was skipped (no last_user_email yet)
        const lastEmail = localStorage.getItem(LAST_USER_KEY);
        if (lastEmail && user.email !== lastEmail) {
          console.warn(`[auth] Server returned ${user.email} but expected ${lastEmail}. Logging out foreign session.`);
          tokens = null;
          user = null;
          await keychainDeleteTokens().catch(() => {});
          await fileDeleteTokens();
          return;
        }
        saveLastUserEmail(user.email);
      }
      // Pull remote settings silently after login
      syncService.pull().catch((e) => console.warn('[Weplex] Settings sync failed after init:', e));
      // Initialize team and collaborative pipelines after auth
      teamStore.init().catch((e) => console.warn('[Weplex] Team init failed:', e));
      collabPipelineStore.init().catch((e) => console.warn('[Weplex] Collab pipeline init failed:', e));
      presenceStore.init();
      chatStore.init();
      // Ensure file backup is in sync
      fileSaveTokens(tokens!).catch(() => {});
    } catch (e: any) {
      // Only clear tokens on confirmed auth failure (401)
      // Network errors, timeouts, server errors — keep tokens, retry later
      const status = e?.status || e?.response?.status;
      if (status === 401) {
        tokens = null;
        user = null;
        await keychainDeleteTokens().catch((err) =>
          console.error('[Weplex] Failed to clear keychain during init cleanup:', err),
        );
        await fileDeleteTokens();
      } else {
        // Keep tokens, user stays "logged in" but profile not loaded
        // Will retry on next app focus or scheduled retry
        console.warn('[auth] Profile fetch failed, keeping tokens for retry:', e?.message || e);
        this._scheduleProfileRetry();
      }
    }
  },

  /** Retry profile fetch after transient failure. Cleans up on success or 401. */
  _scheduleProfileRetry(): void {
    // Clean up any previous listener
    if (focusRetryCleanup) {
      focusRetryCleanup();
      focusRetryCleanup = null;
    }

    const retryProfile = async () => {
      if (!tokens || user) return; // already resolved
      try {
        user = await authService.getProfile();
        if (user?.email) saveLastUserEmail(user.email);
        // Success — init dependent stores
        syncService.pull().catch((e) => console.warn('[Weplex] Settings sync failed after retry:', e));
        teamStore.init().catch((e) => console.warn('[Weplex] Team init failed:', e));
        collabPipelineStore.init().catch((e) => console.warn('[Weplex] Collab pipeline init failed:', e));
      presenceStore.init();
      chatStore.init();
        // Clean up listener
        if (focusRetryCleanup) {
          focusRetryCleanup();
          focusRetryCleanup = null;
        }
      } catch (err: any) {
        const status = err?.status || err?.response?.status;
        if (status === 401) {
          // Confirmed auth failure — clean up
          tokens = null;
          user = null;
          await keychainDeleteTokens().catch(() => {});
          await fileDeleteTokens();
          if (focusRetryCleanup) {
            focusRetryCleanup();
            focusRetryCleanup = null;
          }
        }
        // Otherwise keep waiting for next focus/retry
      }
    };

    // Retry once after 30 seconds
    const timerId = setTimeout(retryProfile, 30_000);

    // Retry on window focus
    const onFocus = () => retryProfile();
    window.addEventListener('focus', onFocus);

    focusRetryCleanup = () => {
      clearTimeout(timerId);
      window.removeEventListener('focus', onFocus);
    };
  },

  async login(email: string, password: string): Promise<void> {
    loading = true;
    error = null;
    try {
      const res = await authService.login(email, password);
      tokens = { accessToken: res.accessToken, refreshToken: res.refreshToken };
      user = res.user;
      if (user?.email) saveLastUserEmail(user.email);
      await keychainSaveTokens(tokens);
      await fileSaveTokens(tokens);
      syncService
        .pull()
        .catch((e) => console.warn('[Weplex] Settings sync failed after login:', e));
      // Initialize team and collaborative pipelines after login
      teamStore.init().catch((e) => console.warn('[Weplex] Team init failed:', e));
      collabPipelineStore.init().catch((e) => console.warn('[Weplex] Collab pipeline init failed:', e));
      presenceStore.init();
      chatStore.init();
    } catch (e) {
      error = e instanceof Error ? e.message : 'Login failed';
      throw e;
    } finally {
      loading = false;
    }
  },

  async register(email: string, password: string): Promise<void> {
    loading = true;
    error = null;
    try {
      const res = await authService.register(email, password);
      tokens = { accessToken: res.accessToken, refreshToken: res.refreshToken };
      user = res.user;
      if (user?.email) saveLastUserEmail(user.email);
      await keychainSaveTokens(tokens);
      await fileSaveTokens(tokens);
    } catch (e) {
      error = e instanceof Error ? e.message : 'Registration failed';
      throw e;
    } finally {
      loading = false;
    }
  },

  /** Start OAuth flow: open browser, wait for callback, exchange code. */
  async oauthLogin(provider: 'github' | 'google'): Promise<void> {
    loading = true;
    error = null;
    try {
      const { listen } = await import('@tauri-apps/api/event');
      const apiBase = getBaseUrl();
      const stateNonce = crypto.randomUUID();

      // Listen for the dynamically allocated port from the Rust server
      const unlisten = await listen<number>('oauth-server-ready', (event) => {
        const port = event.payload;
        const oauthUrl = `${apiBase}/auth/${provider}?redirect_uri=http://127.0.0.1:${port}/auth/callback&state=${stateNonce}`;
        invoke('open_url', { url: oauthUrl }).catch(() => {
          // Browser open failure is handled when serverPromise resolves/rejects
        });
      });

      // start_oauth_server binds to port 0, emits the port event, then blocks waiting for callback
      const serverPromise = invoke<string>('start_oauth_server', { expectedState: stateNonce });

      let code: string;
      try {
        code = await serverPromise;
      } finally {
        unlisten();
      }

      // Exchange code for tokens
      const res = await authService.exchange(code, provider);
      tokens = { accessToken: res.accessToken, refreshToken: res.refreshToken };
      user = res.user;
      if (user?.email) saveLastUserEmail(user.email);
      await keychainSaveTokens(tokens);
      await fileSaveTokens(tokens);
      syncService
        .pull()
        .catch((e) => console.warn('[Weplex] Settings sync failed after OAuth:', e));
      // Initialize team and collaborative pipelines after OAuth
      teamStore.init().catch((e) => console.warn('[Weplex] Team init failed:', e));
      collabPipelineStore.init().catch((e) => console.warn('[Weplex] Collab pipeline init failed:', e));
      presenceStore.init();
      chatStore.init();
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      if (msg.includes('Bind failed')) {
        error = 'OAuth port is busy. Close other Weplex instances and try again.';
      } else if (msg.includes('timed out')) {
        error = 'Authentication timed out. Please try again.';
      } else if (msg.includes('state mismatch')) {
        error = 'Authentication failed — security check failed. Please try again.';
      } else if (msg.includes('OAuth denied')) {
        error = 'Authentication was denied. Please try again.';
      } else {
        error = msg || 'OAuth failed';
      }
      throw e;
    } finally {
      loading = false;
    }
  },

  async logout(): Promise<void> {
    try {
      await authService.logout();
    } catch {
      // Ignore — server may be unreachable, still clear local state
    }
    // Clean up team and collaborative pipeline state
    teamStore.reset();
    collabPipelineStore.reset();
    presenceStore.reset();
    chatStore.reset();
    pipelineWsService.disconnect();

    // Clean up any pending retry listener
    if (focusRetryCleanup) {
      focusRetryCleanup();
      focusRetryCleanup = null;
    }

    tokens = null;
    user = null;
    error = null;
    await keychainDeleteTokens().catch((e) =>
      console.error('[Weplex] Failed to clear keychain on logout:', e),
    );
    await fileDeleteTokens();

    // CRITICAL: Clear ALL app state to prevent cross-account data leakage.
    // Without this, the next user on the same machine would see previous user's
    // sessions, spaces, notes, folders via recoverStores().
    const { STORE_KEYS } = await import('../utils/durablePersist');
    for (const key of STORE_KEYS) {
      localStorage.removeItem(key);
      // Also clear file backups
      invoke('persist_store', { key, value: '' }).catch(() => {});
    }
    // Clear additional UI state
    localStorage.removeItem('weplex_sidebar_collapsed');
    localStorage.removeItem('weplex_detail_visible');
  },

  async updateProfile(patch: { displayName?: string }): Promise<void> {
    error = null;
    try {
      user = await authService.updateProfile(patch);
    } catch (e) {
      error = e instanceof Error ? e.message : 'Update failed';
      throw e;
    }
  },

  async verifyEmail(code: string): Promise<void> {
    loading = true;
    error = null;
    try {
      await authService.verifyEmail(code);
      // Refresh profile to get updated emailVerified status
      user = await authService.getProfile();
    } catch (e) {
      error = e instanceof Error ? e.message : 'Verification failed';
      throw e;
    } finally {
      loading = false;
    }
  },

  async sendVerificationCode(): Promise<void> {
    loading = true;
    error = null;
    try {
      await authService.sendVerification();
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to send code';
      throw e;
    } finally {
      loading = false;
    }
  },

  async forgotPassword(email: string): Promise<void> {
    loading = true;
    error = null;
    try {
      await authService.forgotPassword(email);
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to send reset code';
      throw e;
    } finally {
      loading = false;
    }
  },

  async resetPassword(email: string, code: string, newPassword: string): Promise<void> {
    loading = true;
    error = null;
    try {
      await authService.resetPassword(email, code, newPassword);
    } catch (e) {
      error = e instanceof Error ? e.message : 'Password reset failed';
      throw e;
    } finally {
      loading = false;
    }
  },

  clearError(): void {
    error = null;
  },
};

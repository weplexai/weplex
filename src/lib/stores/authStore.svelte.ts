import type { AuthUser, AuthTokens } from '../types';
import { initApiClient, resolveApiEndpoint, getBaseUrl } from '../services/apiClient';
import { authService } from '../services/authService';
import { syncService } from '../services/syncService';
import { invoke } from '@tauri-apps/api/core';

const KEYCHAIN_KEY = 'auth_tokens';

// ── State ──────────────────────────────────────────────────────────────────

let user = $state<AuthUser | null>(null);
let tokens = $state<AuthTokens | null>(null);
let loading = $state(false);
let error = $state<string | null>(null);

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

// ── Wire up apiClient token access ─────────────────────────────────────────

initApiClient(
  () => tokens,
  (newTokens) => {
    tokens = newTokens;
    if (newTokens) {
      keychainSaveTokens(newTokens).catch(console.error);
    } else {
      keychainDeleteTokens().catch(console.error);
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
    return user !== null && tokens !== null;
  },
  get syncStatus() {
    return syncService.status;
  },

  /** Load persisted tokens, fetch profile, trigger sync. Call once at app start. */
  async init(): Promise<void> {
    // Resolve best API endpoint (try .ai first, fallback to .xyz)
    await resolveApiEndpoint();

    const saved = await keychainLoadTokens();
    if (!saved) return;

    tokens = saved;
    try {
      user = await authService.getProfile();
      // Pull remote settings silently after login
      syncService.pull().catch((e) => console.warn('[Weplex] Settings sync failed after init:', e));
    } catch {
      // Token expired and refresh failed — clean up
      tokens = null;
      user = null;
      await keychainDeleteTokens().catch((e) =>
        console.error('[Weplex] Failed to clear keychain during init cleanup:', e),
      );
    }
  },

  async login(email: string, password: string): Promise<void> {
    loading = true;
    error = null;
    try {
      const res = await authService.login(email, password);
      tokens = { accessToken: res.accessToken, refreshToken: res.refreshToken };
      user = res.user;
      await keychainSaveTokens(tokens);
      syncService
        .pull()
        .catch((e) => console.warn('[Weplex] Settings sync failed after login:', e));
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
      await keychainSaveTokens(tokens);
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
      await keychainSaveTokens(tokens);
      syncService
        .pull()
        .catch((e) => console.warn('[Weplex] Settings sync failed after OAuth:', e));
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
    tokens = null;
    user = null;
    error = null;
    await keychainDeleteTokens().catch((e) =>
      console.error('[Weplex] Failed to clear keychain on logout:', e),
    );
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

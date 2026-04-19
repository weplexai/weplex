import type { AuthTokens } from '../types';
import { invoke } from '@tauri-apps/api/core';

const KEYCHAIN_KEY = 'auth_tokens';
const FILE_BACKUP_KEY = 'weplex_auth_tokens';
export const LAST_USER_KEY = 'weplex_last_user_email';

/** Extract email from JWT access token without verification (client-side check only). */
export function extractEmailFromJwt(token: string): string | null {
  try {
    const payload = JSON.parse(atob(token.split('.')[1]));
    return payload.email || null;
  } catch {
    return null;
  }
}

/** Save last authenticated user email for cross-account protection. */
export function saveLastUserEmail(email: string): void {
  localStorage.setItem(LAST_USER_KEY, email);
  invoke('persist_store', { key: LAST_USER_KEY, value: email }).catch(() => {});
}

// ── OS Keychain ──

export async function keychainLoadTokens(): Promise<AuthTokens | null> {
  try {
    const raw = await invoke<string | null>('keychain_load', { key: KEYCHAIN_KEY });
    if (!raw) return null;
    return JSON.parse(raw) as AuthTokens;
  } catch {
    return null;
  }
}

export async function keychainSaveTokens(t: AuthTokens): Promise<void> {
  await invoke('keychain_save', { key: KEYCHAIN_KEY, value: JSON.stringify(t) });
}

export async function keychainDeleteTokens(): Promise<void> {
  await invoke('keychain_delete', { key: KEYCHAIN_KEY });
}

// ── Encrypted file backup ──
// AES-256-GCM with machine-derived key. Fallback for keychain failures.

export async function fileSaveTokens(t: AuthTokens): Promise<void> {
  try {
    await invoke('secure_store_save', { key: FILE_BACKUP_KEY, value: JSON.stringify(t) });
  } catch (e) {
    console.warn('[auth] Encrypted backup save failed:', e);
  }
}

export async function fileLoadTokens(): Promise<AuthTokens | null> {
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

export async function fileDeleteTokens(): Promise<void> {
  try {
    await invoke('secure_store_delete', { key: FILE_BACKUP_KEY });
  } catch (e) {
    console.warn('[auth] Encrypted backup delete failed:', e);
  }
}

// API client with bearer auth injection and single-flight token refresh

import { logger } from '../utils/logger';

const API_ENDPOINTS = import.meta.env.VITE_API_URL
  ? [import.meta.env.VITE_API_URL as string]
  : ['https://api.weplex.ai', 'https://api.weplex.xyz'];

let resolvedBaseUrl: string | null = null;

/** Resolve the best available API endpoint. Called once on app start. */
export async function resolveApiEndpoint(): Promise<string | null> {
  for (const endpoint of API_ENDPOINTS) {
    try {
      const res = await fetch(`${endpoint}/health`, {
        signal: AbortSignal.timeout(3000),
      });
      if (res.ok) {
        resolvedBaseUrl = endpoint;
        logger.info(`API endpoint: ${endpoint}`);
        return endpoint;
      }
    } catch {
      // try next
    }
  }
  console.warn('[Weplex] No API endpoint available — running offline');
  return null;
}

export function getBaseUrl(): string {
  return resolvedBaseUrl || API_ENDPOINTS[0];
}

// ── Error classes ──────────────────────────────────────────────────────────

export class ApiError extends Error {
  constructor(
    public status: number,
    message: string,
  ) {
    super(message);
    this.name = 'ApiError';
  }
}

export class NetworkError extends Error {
  constructor(message: string = 'Network error') {
    super(message);
    this.name = 'NetworkError';
  }
}

export class AuthError extends ApiError {
  constructor(message: string = 'Unauthorized') {
    super(401, message);
    this.name = 'AuthError';
  }
}

// ── Token management ───────────────────────────────────────────────────────

type TokenGetter = () => { accessToken: string; refreshToken: string } | null;
type TokenSetter = (tokens: { accessToken: string; refreshToken: string } | null) => void;

let getTokens: TokenGetter = () => null;
let setTokens: TokenSetter = () => {};

/** Single-flight refresh: only one refresh request at a time, others queue. */
let refreshPromise: Promise<boolean> | null = null;

/** Initialize the API client with token access functions. Called by authStore. */
export function initApiClient(tokenGetter: TokenGetter, tokenSetter: TokenSetter): void {
  getTokens = tokenGetter;
  setTokens = tokenSetter;
}

/** Get the current access token, or null if not authenticated. */
export function getAccessToken(): string | null {
  return getTokens()?.accessToken ?? null;
}

// ── Core request method ────────────────────────────────────────────────────

interface RequestOptions extends Omit<RequestInit, 'body'> {
  body?: unknown;
  skipAuth?: boolean;
  /** Internal flag: prevents infinite refresh-retry loops. Do not set manually. */
  _isRetry?: boolean;
}

export async function request<T>(path: string, options: RequestOptions = {}): Promise<T> {
  const { body, skipAuth, _isRetry, ...fetchOpts } = options;
  const url = `${getBaseUrl()}${path}`;

  const headers = new Headers(fetchOpts.headers);
  if (body !== undefined) {
    headers.set('Content-Type', 'application/json');
  }

  // Inject bearer token unless skipped (login/register don't need it)
  if (!skipAuth) {
    const tokens = getTokens();
    if (tokens?.accessToken) {
      headers.set('Authorization', `Bearer ${tokens.accessToken}`);
    }
  }

  let response: Response;
  try {
    response = await fetch(url, {
      ...fetchOpts,
      headers,
      body: body !== undefined ? JSON.stringify(body) : undefined,
    });
  } catch (e) {
    throw new NetworkError(e instanceof Error ? e.message : 'Network error');
  }

  // Handle 401 — attempt single-flight token refresh (once only)
  if (response.status === 401 && !skipAuth && !_isRetry) {
    const refreshed = await attemptRefresh();
    if (refreshed) {
      // Retry the original request with the new token, marked as retry
      return request<T>(path, { ...options, _isRetry: true });
    }
    // Refresh failed — clear tokens and throw
    setTokens(null);
    throw new AuthError('Session expired');
  }

  // Already retried once and still 401 — don't loop, just fail
  if (response.status === 401 && !skipAuth && _isRetry) {
    setTokens(null);
    throw new AuthError('Session expired');
  }

  if (!response.ok) {
    const text = await response.text().catch(() => '');
    let message = text;
    try {
      const json = JSON.parse(text);
      message = json.message || json.error || text;
    } catch {
      // text is fine
    }
    throw new ApiError(response.status, message);
  }

  // 204 No Content
  if (response.status === 204) {
    return undefined as T;
  }

  return response.json();
}

// ── Token refresh ──────────────────────────────────────────────────────────

async function attemptRefresh(): Promise<boolean> {
  // Single-flight: if a refresh is already in progress, wait for it
  if (refreshPromise) {
    return refreshPromise;
  }

  const tokens = getTokens();
  if (!tokens?.refreshToken) return false;

  refreshPromise = doRefresh(tokens.refreshToken);
  try {
    return await refreshPromise;
  } finally {
    refreshPromise = null;
  }
}

async function doRefresh(refreshToken: string): Promise<boolean> {
  try {
    const response = await fetch(`${getBaseUrl()}/auth/refresh`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ refreshToken }),
    });

    if (!response.ok) return false;

    const data = await response.json();
    setTokens({
      accessToken: data.accessToken,
      refreshToken: data.refreshToken,
    });
    return true;
  } catch {
    return false;
  }
}

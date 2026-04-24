export interface AuthUser {
  id: string;
  email: string;
  displayName: string | null;
  plan: string;
  oauthProvider: string | null;
  emailVerified: boolean;
}

export interface AuthTokens {
  accessToken: string;
  refreshToken: string;
}

export interface AuthResponse {
  accessToken: string;
  refreshToken: string;
  user: AuthUser;
}

export type SyncStatus = 'idle' | 'pulling' | 'pushing' | 'error';

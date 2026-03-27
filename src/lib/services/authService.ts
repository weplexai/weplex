// Stateless auth service — thin layer over apiClient

import { request } from './apiClient';
import type { AuthResponse, AuthUser } from '../types';

export const authService = {
  async register(email: string, password: string): Promise<AuthResponse> {
    return request<AuthResponse>('/auth/register', {
      method: 'POST',
      body: { email, password },
      skipAuth: true,
    });
  },

  async login(email: string, password: string): Promise<AuthResponse> {
    return request<AuthResponse>('/auth/login', {
      method: 'POST',
      body: { email, password },
      skipAuth: true,
    });
  },

  async refresh(refreshToken: string): Promise<AuthResponse> {
    return request<AuthResponse>('/auth/refresh', {
      method: 'POST',
      body: { refreshToken },
      skipAuth: true,
    });
  },

  /** Exchange OAuth code for tokens (desktop callback flow). */
  async exchange(code: string, provider: string): Promise<AuthResponse> {
    return request<AuthResponse>('/auth/exchange', {
      method: 'POST',
      body: { code, provider },
      skipAuth: true,
    });
  },

  async logout(): Promise<void> {
    return request<void>('/auth/logout', { method: 'POST' });
  },

  async getProfile(): Promise<AuthUser> {
    return request<AuthUser>('/auth/profile', { method: 'GET' });
  },

  async updateProfile(patch: { displayName?: string }): Promise<AuthUser> {
    return request<AuthUser>('/auth/profile', {
      method: 'PATCH',
      body: patch,
    });
  },
};

// Team management service — thin layer over apiClient

import { request } from './apiClient';
import type { TeamInfo } from '../types';

export const teamService = {
  async createTeam(name: string): Promise<TeamInfo> {
    return request<TeamInfo>('/teams', {
      method: 'POST',
      body: { name },
    });
  },

  async getMyTeam(): Promise<TeamInfo | null> {
    // Server returns 204 when user has no team
    const result = await request<TeamInfo | undefined>('/teams/mine', {
      method: 'GET',
    });
    return result ?? null;
  },

  async joinTeam(inviteCode: string): Promise<TeamInfo> {
    return request<TeamInfo>('/teams/join', {
      method: 'POST',
      body: { inviteCode },
    });
  },

  async leaveTeam(): Promise<void> {
    return request<void>('/teams/leave', {
      method: 'POST',
    });
  },

  async regenerateCode(): Promise<{ inviteCode: string }> {
    return request<{ inviteCode: string }>('/teams/regenerate-code', {
      method: 'POST',
    });
  },

  async removeMember(memberId: string): Promise<void> {
    return request<void>(`/teams/members/${memberId}`, {
      method: 'DELETE',
    });
  },
};

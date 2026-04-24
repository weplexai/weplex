// Team management service — thin layer over apiClient

import { request } from './apiClient';
import type { TeamInfo } from '../types';

export const teamService = {
  async getMyTeams(): Promise<TeamInfo[]> {
    return request<TeamInfo[]>('/teams', {
      method: 'GET',
    });
  },

  async getTeam(teamId: string): Promise<TeamInfo> {
    return request<TeamInfo>(`/teams/${teamId}`, {
      method: 'GET',
    });
  },

  async createTeam(name: string): Promise<TeamInfo> {
    return request<TeamInfo>('/teams', {
      method: 'POST',
      body: { name },
    });
  },

  async joinTeam(inviteCode: string): Promise<TeamInfo> {
    return request<TeamInfo>('/teams/join', {
      method: 'POST',
      body: { inviteCode },
    });
  },

  async leaveTeam(teamId: string): Promise<void> {
    return request<void>(`/teams/${teamId}/leave`, {
      method: 'POST',
    });
  },

  async deleteTeam(teamId: string): Promise<void> {
    return request<void>(`/teams/${teamId}`, {
      method: 'DELETE',
    });
  },

  async regenerateCode(teamId: string): Promise<{ inviteCode: string }> {
    return request<{ inviteCode: string }>(`/teams/${teamId}/regenerate-code`, {
      method: 'POST',
    });
  },

  async removeMember(teamId: string, memberId: string): Promise<void> {
    return request<void>(`/teams/${teamId}/members/${memberId}`, {
      method: 'DELETE',
    });
  },

  async transferOwnership(teamId: string, memberId: string): Promise<TeamInfo> {
    return request<TeamInfo>(`/teams/${teamId}/transfer`, {
      method: 'POST',
      body: { memberId },
    });
  },
};

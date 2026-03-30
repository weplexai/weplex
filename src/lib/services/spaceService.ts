// Space management service — server-synced shared/team spaces

import { request } from './apiClient';
import type { ServerSpace, SpaceType } from '../types';

export const spaceService = {
  /** Create a shared or team space on the server. */
  async createSpace(
    teamId: string,
    name: string,
    color: string,
    type: SpaceType,
    shared = true,
  ): Promise<ServerSpace> {
    return request<ServerSpace>('/teams/spaces', {
      method: 'POST',
      body: { teamId, name, color, type, shared },
    });
  },

  /** List all spaces for a team (shared + team spaces visible to the user). */
  async listSpaces(teamId: string): Promise<ServerSpace[]> {
    return request<ServerSpace[]>(`/teams/spaces?teamId=${encodeURIComponent(teamId)}`, {
      method: 'GET',
    });
  },

  /** Update a server-synced space. */
  async updateSpace(
    spaceId: string,
    patch: Partial<Pick<ServerSpace, 'name' | 'color' | 'shared'>>,
  ): Promise<ServerSpace> {
    return request<ServerSpace>(`/teams/spaces/${spaceId}`, {
      method: 'PATCH',
      body: patch,
    });
  },

  /** Delete a server-synced space. */
  async deleteSpace(spaceId: string): Promise<void> {
    return request<void>(`/teams/spaces/${spaceId}`, {
      method: 'DELETE',
    });
  },
};

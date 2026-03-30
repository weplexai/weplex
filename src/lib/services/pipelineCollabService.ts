// Pipeline collaboration service — REST API for collaborative pipeline runs

import { request } from './apiClient';
import type { CollaborativeRun, CreateRunPayload, CreateRunResponse } from '../types';

export const pipelineCollabService = {
  async createRun(payload: CreateRunPayload): Promise<CreateRunResponse> {
    return request<CreateRunResponse>('/pipelines/runs', {
      method: 'POST',
      body: payload,
    });
  },

  async getRuns(status?: string): Promise<CollaborativeRun[]> {
    const query = status ? `?status=${encodeURIComponent(status)}` : '';
    return request<CollaborativeRun[]>(`/pipelines/runs${query}`, {
      method: 'GET',
    });
  },

  async getRun(runId: string): Promise<CollaborativeRun> {
    return request<CollaborativeRun>(`/pipelines/runs/${runId}`, {
      method: 'GET',
    });
  },

  async startStage(runId: string, stageName: string): Promise<CollaborativeRun> {
    return request<CollaborativeRun>(
      `/pipelines/runs/${runId}/stages/${encodeURIComponent(stageName)}/start`,
      { method: 'POST' },
    );
  },

  async completeStage(
    runId: string,
    stageName: string,
    artifact: string,
  ): Promise<CollaborativeRun> {
    return request<CollaborativeRun>(
      `/pipelines/runs/${runId}/stages/${encodeURIComponent(stageName)}/complete`,
      {
        method: 'POST',
        body: { artifact },
      },
    );
  },

  async failStage(runId: string, stageName: string, error: string): Promise<CollaborativeRun> {
    return request<CollaborativeRun>(
      `/pipelines/runs/${runId}/stages/${encodeURIComponent(stageName)}/fail`,
      {
        method: 'POST',
        body: { error },
      },
    );
  },

  async cancelRun(runId: string): Promise<CollaborativeRun> {
    return request<CollaborativeRun>(`/pipelines/runs/${runId}/cancel`, {
      method: 'POST',
    });
  },

  async getArtifact(runId: string, stageName: string): Promise<string> {
    return request<string>(
      `/pipelines/runs/${runId}/artifacts/${encodeURIComponent(stageName)}`,
      { method: 'GET' },
    );
  },
};

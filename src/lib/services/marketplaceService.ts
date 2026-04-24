// Marketplace API service — thin wrapper over apiClient.request()

import { request } from './apiClient';

interface MarketplacePackage {
  id: string;
  name: string;
  description: string;
  type: string;
  category: string;
  tags: string[];
  version: string;
  icon: string | null;
  downloads: number;
  rating: number;
  ratingCount: number;
  verified: boolean;
  author: { id: string; displayName: string };
  createdAt: string;
}

interface SearchResult {
  packages: MarketplacePackage[];
  total: number;
}

interface InstallResult {
  name: string;
  type: string;
  content: string;
}

interface SearchParams {
  q?: string;
  type?: string;
  category?: string;
  sort?: string;
  limit?: number;
  offset?: number;
}

export type { MarketplacePackage, SearchResult, InstallResult };

export async function searchPackages(params: SearchParams): Promise<SearchResult> {
  const query = new URLSearchParams();
  if (params.q) query.set('q', params.q);
  if (params.type) query.set('type', params.type);
  if (params.category) query.set('category', params.category);
  if (params.sort) query.set('sort', params.sort);
  if (params.limit) query.set('limit', String(params.limit));
  if (params.offset) query.set('offset', String(params.offset));

  return request<SearchResult>(`/marketplace/search?${query}`, { skipAuth: true });
}

export async function installPackage(id: string): Promise<InstallResult> {
  return request<InstallResult>(`/marketplace/${id}/install`, { method: 'POST' });
}

export async function ratePackage(id: string, rating: number): Promise<void> {
  await request(`/marketplace/${id}/rate`, {
    method: 'POST',
    body: { rating },
  });
}

export async function publishPackage(data: {
  name: string;
  description: string;
  type: string;
  category: string;
  tags: string[];
  version: string;
  content: string;
  license: string;
}): Promise<void> {
  await request('/marketplace/publish', {
    method: 'POST',
    body: data,
  });
}

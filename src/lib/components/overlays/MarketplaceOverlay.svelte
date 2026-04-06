<script lang="ts">
  import { uiStore } from '../../stores/uiStore';
  import { authStore } from '../../stores/authStore.svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { Modal } from '../ui';

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

  let query = $state('');
  let category = $state('');
  let type = $state('');
  let sort = $state('popular');
  let packages = $state<MarketplacePackage[]>([]);
  let total = $state(0);
  let loading = $state(false);
  let selectedPkg = $state<MarketplacePackage | null>(null);
  let installing = $state<string | null>(null);
  let installSuccess = $state<string | null>(null);

  const categories = [
    '', 'backend', 'frontend', 'mobile', 'database', 'security',
    'testing', 'devops', 'development', 'bugfix', 'refactoring',
  ];

  async function search() {
    loading = true;
    try {
      const params = new URLSearchParams();
      if (query) params.set('q', query);
      if (category) params.set('category', category);
      if (type) params.set('type', type);
      params.set('sort', sort);

      const res = await fetch(`${getApiBase()}/marketplace/search?${params}`);
      if (res.ok) {
        const data = await res.json();
        packages = data.packages || [];
        total = data.total || 0;
      }
    } catch (e) {
      console.error('[Marketplace] Search failed:', e);
    } finally {
      loading = false;
    }
  }

  async function installPackage(pkg: MarketplacePackage) {
    installing = pkg.id;
    try {
      const res = await fetch(`${getApiBase()}/marketplace/${pkg.id}/install`, { method: 'POST' });
      if (!res.ok) throw new Error('Install failed');
      const data = await res.json();

      // Save YAML to local agents/pipelines directory
      const dir = data.type === 'agent' ? 'agents' : 'pipelines';
      await invoke('save_marketplace_package', {
        dir,
        name: data.name,
        content: data.content,
      });

      installSuccess = pkg.id;
      setTimeout(() => { installSuccess = null; }, 3000);
    } catch (e) {
      console.error('[Marketplace] Install failed:', e);
    } finally {
      installing = null;
    }
  }

  function getApiBase(): string {
    return 'https://api.weplex.ai';
  }

  function formatRating(rating: number): string {
    return rating > 0 ? `${rating.toFixed(1)}★` : '—';
  }

  function formatDownloads(n: number): string {
    if (n >= 1000) return `${(n / 1000).toFixed(1)}K`;
    return String(n);
  }

  function close() {
    uiStore.closeOverlay();
  }

  // Initial search on mount
  $effect(() => {
    search();
  });
</script>

<Modal onclose={close} title="Marketplace" width="720px">
  <!-- Search & Filters -->
  <div class="mp-toolbar">
    <input
      class="mp-search"
      type="text"
      placeholder="Search agents & pipelines..."
      bind:value={query}
      oninput={() => search()}
    />
    <select class="mp-filter" bind:value={type} onchange={() => search()}>
      <option value="">All types</option>
      <option value="agent">Agents</option>
      <option value="pipeline">Pipelines</option>
    </select>
    <select class="mp-filter" bind:value={category} onchange={() => search()}>
      {#each categories as cat}
        <option value={cat}>{cat || 'All categories'}</option>
      {/each}
    </select>
    <select class="mp-filter" bind:value={sort} onchange={() => search()}>
      <option value="popular">Popular</option>
      <option value="rating">Top rated</option>
      <option value="newest">Newest</option>
      <option value="name">A-Z</option>
    </select>
  </div>

  <!-- Results -->
  <div class="mp-results">
    {#if loading}
      <div class="mp-loading">Loading...</div>
    {:else if packages.length === 0}
      <div class="mp-empty">No packages found</div>
    {:else}
      <div class="mp-grid">
        {#each packages as pkg (pkg.id)}
          <button
            class="mp-card"
            class:selected={selectedPkg?.id === pkg.id}
            onclick={() => selectedPkg = pkg}
          >
            <div class="mp-card-header">
              <span class="mp-card-type" class:agent={pkg.type === 'agent'} class:pipeline={pkg.type === 'pipeline'}>
                {pkg.type}
              </span>
              {#if pkg.verified}
                <span class="mp-verified" title="Verified publisher">✓</span>
              {/if}
            </div>
            <h3 class="mp-card-name">{pkg.name}</h3>
            <p class="mp-card-desc">{pkg.description}</p>
            <div class="mp-card-meta">
              <span>{formatRating(pkg.rating)}</span>
              <span>{formatDownloads(pkg.downloads)} installs</span>
              <span>v{pkg.version}</span>
            </div>
            <div class="mp-card-tags">
              {#each pkg.tags.slice(0, 3) as tag}
                <span class="mp-tag">{tag}</span>
              {/each}
            </div>
          </button>
        {/each}
      </div>
      <div class="mp-total">{total} packages</div>
    {/if}
  </div>

  <!-- Detail panel (when package selected) -->
  {#if selectedPkg}
    <div class="mp-detail">
      <div class="mp-detail-header">
        <h2>{selectedPkg.name}</h2>
        <span class="mp-detail-version">v{selectedPkg.version}</span>
      </div>
      <p class="mp-detail-desc">{selectedPkg.description}</p>
      <div class="mp-detail-stats">
        <span>{formatRating(selectedPkg.rating)} ({selectedPkg.ratingCount} ratings)</span>
        <span>{formatDownloads(selectedPkg.downloads)} installs</span>
        <span>by {selectedPkg.author?.displayName}</span>
      </div>
      <div class="mp-detail-tags">
        {#each selectedPkg.tags as tag}
          <span class="mp-tag">{tag}</span>
        {/each}
      </div>
      <button
        class="mp-install-btn"
        class:installing={installing === selectedPkg.id}
        class:success={installSuccess === selectedPkg.id}
        onclick={() => selectedPkg && installPackage(selectedPkg)}
        disabled={installing !== null}
      >
        {#if installSuccess === selectedPkg.id}
          Installed ✓
        {:else if installing === selectedPkg.id}
          Installing...
        {:else}
          Install {selectedPkg.type}
        {/if}
      </button>
    </div>
  {/if}
</Modal>

<style>
  .mp-toolbar {
    display: flex;
    gap: 8px;
    margin-bottom: 12px;
  }

  .mp-search {
    flex: 1;
    padding: 6px 10px;
    background: var(--weplex-bg);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-sm);
    color: var(--weplex-text);
    font-size: var(--weplex-text-sm);
    outline: none;
  }

  .mp-search:focus {
    border-color: var(--weplex-accent);
  }

  .mp-filter {
    padding: 6px 8px;
    background: var(--weplex-surface);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-sm);
    color: var(--weplex-text);
    font-size: var(--weplex-text-xs);
    outline: none;
  }

  .mp-results {
    max-height: 400px;
    overflow-y: auto;
  }

  .mp-loading, .mp-empty {
    padding: 40px;
    text-align: center;
    color: var(--weplex-text-muted);
    font-size: var(--weplex-text-sm);
  }

  .mp-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    gap: 8px;
  }

  .mp-card {
    padding: 10px;
    background: var(--weplex-surface);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-md);
    cursor: pointer;
    text-align: left;
    color: var(--weplex-text);
    width: 100%;
  }

  .mp-card:hover {
    border-color: var(--weplex-border-active);
  }

  .mp-card.selected {
    border-color: var(--weplex-accent);
  }

  .mp-card-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 4px;
  }

  .mp-card-type {
    font-size: 9px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    padding: 1px 4px;
    border-radius: 2px;
  }

  .mp-card-type.agent { color: var(--weplex-accent); background: color-mix(in srgb, var(--weplex-accent) 15%, transparent); }
  .mp-card-type.pipeline { color: var(--weplex-active); background: color-mix(in srgb, var(--weplex-active) 15%, transparent); }

  .mp-verified { color: var(--weplex-active); font-size: 12px; }

  .mp-card-name {
    font-size: var(--weplex-text-sm);
    font-weight: 600;
    margin: 0 0 4px;
  }

  .mp-card-desc {
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
    margin: 0 0 6px;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .mp-card-meta {
    display: flex;
    gap: 8px;
    font-size: 10px;
    color: var(--weplex-text-muted);
  }

  .mp-card-tags {
    display: flex;
    gap: 3px;
    margin-top: 6px;
    flex-wrap: wrap;
  }

  .mp-tag {
    font-size: 9px;
    padding: 1px 4px;
    background: var(--weplex-surface-hover);
    border-radius: 2px;
    color: var(--weplex-text-muted);
  }

  .mp-total {
    text-align: center;
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
    margin-top: 8px;
  }

  .mp-detail {
    margin-top: 12px;
    padding: 12px;
    border-top: 1px solid var(--weplex-border);
  }

  .mp-detail-header {
    display: flex;
    align-items: baseline;
    gap: 8px;
    margin-bottom: 6px;
  }

  .mp-detail-header h2 {
    font-size: var(--weplex-text-md);
    font-weight: 600;
    margin: 0;
  }

  .mp-detail-version {
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
  }

  .mp-detail-desc {
    font-size: var(--weplex-text-sm);
    color: var(--weplex-text-secondary);
    margin: 0 0 8px;
  }

  .mp-detail-stats {
    display: flex;
    gap: 12px;
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
    margin-bottom: 8px;
  }

  .mp-detail-tags {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
    margin-bottom: 12px;
  }

  .mp-install-btn {
    padding: 8px 20px;
    background: var(--weplex-accent);
    color: white;
    border: none;
    border-radius: var(--weplex-radius-sm);
    font-size: var(--weplex-text-sm);
    font-weight: 600;
    cursor: pointer;
  }

  .mp-install-btn:hover {
    background: var(--weplex-accent-hover);
  }

  .mp-install-btn.installing {
    opacity: 0.6;
    cursor: wait;
  }

  .mp-install-btn.success {
    background: var(--weplex-active);
  }

  .mp-install-btn:disabled {
    cursor: not-allowed;
  }
</style>

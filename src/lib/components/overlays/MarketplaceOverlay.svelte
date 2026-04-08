<script lang="ts">
  import { onMount } from 'svelte';
  import { uiStore } from '../../stores/uiStore';
  import { invoke } from '@tauri-apps/api/core';
  import { Modal } from '../ui';
  import { authStore } from '../../stores/authStore.svelte';
  import {
    searchPackages, installPackage as apiInstall, ratePackage, publishPackage,
    type MarketplacePackage,
  } from '../../services/marketplaceService';

  // ── Search state ──
  let query = $state('');
  let category = $state('');
  let type = $state('');
  let sort = $state('popular');
  let packages = $state<MarketplacePackage[]>([]);
  let total = $state(0);
  let page = $state(0);
  let loading = $state(false);
  let selectedPkg = $state<MarketplacePackage | null>(null);
  let installing = $state<string | null>(null);
  let installSuccess = $state<string | null>(null);

  // ── Installed packages tracking ──
  let installedNames = $state<Set<string>>(new Set());

  // ── View mode: browse | publish ──
  let view = $state<'browse' | 'publish' | 'rate'>('browse');

  // ── Error feedback ──
  let browseError = $state('');

  // ── Rating state ──
  let ratingValue = $state(0);
  let ratingHover = $state(0);
  let ratingSubmitting = $state(false);
  let ratingDone = $state(false);

  // ── Publish state ──
  let pubName = $state('');
  let pubDesc = $state('');
  let pubType = $state<'agent' | 'pipeline'>('agent');
  let pubCategory = $state('development');
  let pubTags = $state('');
  let pubVersion = $state('1.0.0');
  let pubContent = $state('');
  let pubLicense = $state('Apache-2.0');
  let publishing = $state(false);
  let publishError = $state('');
  let publishSuccess = $state(false);

  const PAGE_SIZE = 20;
  const MAX_CONTENT_SIZE = 102_400; // 100KB
  const categories = [
    '', 'backend', 'frontend', 'mobile', 'database', 'security',
    'testing', 'devops', 'development', 'bugfix', 'refactoring',
  ];

  // ── Search ──
  async function search(resetPage = true) {
    if (resetPage) page = 0;
    loading = true;
    browseError = '';
    try {
      const data = await searchPackages({
        q: query || undefined,
        type: type || undefined,
        category: category || undefined,
        sort,
        limit: PAGE_SIZE,
        offset: page * PAGE_SIZE,
      });
      packages = data.packages || [];
      total = data.total || 0;
    } catch (e: unknown) {
      browseError = e instanceof Error ? e.message : 'Search failed';
    } finally {
      loading = false;
    }
  }

  function nextPage() { page++; search(false); }
  function prevPage() { if (page > 0) { page--; search(false); } }
  let totalPages = $derived(Math.ceil(total / PAGE_SIZE));

  let searchTimer: ReturnType<typeof setTimeout> | null = null;
  function debouncedSearch() {
    if (searchTimer) clearTimeout(searchTimer);
    searchTimer = setTimeout(() => search(), 300);
  }

  /** Basic YAML structure validation — checks it looks like valid YAML config. */
  function validateYamlContent(content: string, expectedType: string): string | null {
    if (!content || content.trim().length === 0) return 'Empty content';
    if (content.length > MAX_CONTENT_SIZE) return 'Content exceeds 100KB limit';
    // Check for basic YAML structure (must have key: value lines)
    const lines = content.split('\n').filter((l) => l.trim() && !l.trim().startsWith('#'));
    if (lines.length === 0) return 'No YAML content found';
    const hasKeyValue = lines.some((l) => /^\s*\w[\w\s]*:/.test(l));
    if (!hasKeyValue) return 'Invalid YAML: no key-value pairs found';
    // Type-specific checks
    if (expectedType === 'pipeline' && !content.includes('stages:')) {
      return 'Pipeline YAML must contain "stages:" section';
    }
    return null; // valid
  }

  // ── Install ──
  async function installPackage(pkg: MarketplacePackage) {
    installing = pkg.id;
    browseError = '';
    try {
      const data = await apiInstall(pkg.id);

      // Validate YAML before writing to disk
      const validationError = validateYamlContent(data.content, data.type);
      if (validationError) {
        browseError = `Invalid package content: ${validationError}`;
        return;
      }

      const dir = data.type === 'skill' ? 'skills' : data.type === 'agent' ? 'agents' : 'pipelines';

      if (data.type === 'skill') {
        await invoke('save_marketplace_skill', { name: data.name, content: data.content });
      } else {
        await invoke('save_marketplace_package', { dir, name: data.name, content: data.content });
      }

      installedNames.add(pkg.name);
      installedNames = new Set(installedNames);
      installSuccess = pkg.id;
      setTimeout(() => { installSuccess = null; }, 3000);
    } catch (e: unknown) {
      browseError = e instanceof Error ? e.message : 'Install failed';
    } finally {
      installing = null;
    }
  }

  // ── Rate ──
  function openRating(pkg: MarketplacePackage) {
    selectedPkg = pkg;
    ratingValue = 0;
    ratingHover = 0;
    ratingDone = false;
    view = 'rate';
  }

  let ratingError = $state('');

  async function submitRating() {
    if (!selectedPkg || ratingValue < 1) return;
    ratingSubmitting = true;
    ratingError = '';
    try {
      await ratePackage(selectedPkg.id, ratingValue);
      ratingDone = true;
      setTimeout(() => { view = 'browse'; search(); }, 1500);
    } catch (e: unknown) {
      ratingError = e instanceof Error ? e.message : 'Rating failed';
    } finally {
      ratingSubmitting = false;
    }
  }

  // ── Publish ──
  async function publish() {
    publishError = '';
    if (!pubName.trim() || !pubDesc.trim() || !pubContent.trim()) {
      publishError = 'Name, description, and content are required.';
      return;
    }
    // Client-side YAML validation
    const validationError = validateYamlContent(pubContent, pubType);
    if (validationError) {
      publishError = validationError;
      return;
    }
    publishing = true;
    try {
      await publishPackage({
        name: pubName.trim(),
        description: pubDesc.trim(),
        type: pubType,
        category: pubCategory,
        tags: pubTags.split(',').map((t) => t.trim()).filter(Boolean),
        version: pubVersion.trim(),
        content: pubContent,
        license: pubLicense,
      });
      publishSuccess = true;
      setTimeout(() => {
        publishSuccess = false;
        view = 'browse';
        search();
      }, 2000);
    } catch (e: unknown) {
      publishError = e instanceof Error ? e.message : 'Publish failed';
    } finally {
      publishing = false;
    }
  }

  // ── Load installed packages ──
  async function loadInstalled() {
    try {
      const agents: { name: string }[] = await invoke('list_agents');
      const pipelines: { name: string }[] = await invoke('list_pipelines');
      const names = new Set<string>();
      for (const a of agents) names.add(a.name);
      for (const p of pipelines) names.add(p.name);
      installedNames = names;
    } catch {
      // If commands not available, skip
    }
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

  onMount(() => {
    search();
    loadInstalled();
  });
</script>

<Modal onclose={close} label="Marketplace" class="mp-modal">
  <!-- Tab bar -->
  <div class="mp-tabs">
    <button class="mp-tab" class:active={view === 'browse'} onclick={() => view = 'browse'}>Browse</button>
    <button class="mp-tab" class:active={view === 'publish'} onclick={() => view = 'publish'}>Publish</button>
  </div>

  {#if view === 'browse'}
    <!-- Search & Filters -->
    <div class="mp-toolbar">
      <input
        class="mp-search"
        type="text"
        placeholder="Search agents & pipelines..."
        bind:value={query}
        oninput={() => debouncedSearch()}
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
      {#if browseError}
        <div class="mp-error">{browseError}</div>
      {/if}
      {#if loading}
        <div class="mp-empty">Loading...</div>
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
                {#if installedNames.has(pkg.name)}
                  <span class="mp-installed-badge">installed</span>
                {/if}
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

        <!-- Pagination -->
        {#if totalPages > 1}
          <div class="mp-pagination">
            <button class="mp-page-btn" disabled={page === 0} onclick={prevPage}>← Prev</button>
            <span class="mp-page-info">{page + 1} / {totalPages}</span>
            <button class="mp-page-btn" disabled={page >= totalPages - 1} onclick={nextPage}>Next →</button>
          </div>
        {/if}
      {/if}
    </div>

    <!-- Detail panel -->
    {#if selectedPkg}
      <div class="mp-detail">
        <div class="mp-detail-header">
          <h2>{selectedPkg.name}</h2>
          <span class="mp-detail-version">v{selectedPkg.version}</span>
          {#if installedNames.has(selectedPkg.name)}
            <span class="mp-installed-badge">installed</span>
          {/if}
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
        <div class="mp-detail-actions">
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
            {:else if installedNames.has(selectedPkg.name)}
              Reinstall
            {:else}
              Install
            {/if}
          </button>
          {#if authStore.user}
            <button class="mp-rate-btn" onclick={() => openRating(selectedPkg!)}>Rate</button>
          {/if}
        </div>
      </div>
    {/if}

  {:else if view === 'rate' && selectedPkg}
    <!-- Rating view -->
    <div class="mp-rate-view">
      <h3>Rate {selectedPkg.name}</h3>
      <p class="mp-rate-hint">How useful is this package?</p>
      <div class="mp-stars">
        {#each [1, 2, 3, 4, 5] as star}
          <button
            class="mp-star"
            class:filled={star <= (ratingHover || ratingValue)}
            onmouseenter={() => ratingHover = star}
            onmouseleave={() => ratingHover = 0}
            onclick={() => ratingValue = star}
          >★</button>
        {/each}
      </div>
      {#if ratingError}
        <p class="mp-pub-error">{ratingError}</p>
      {/if}
      {#if ratingDone}
        <p class="mp-rate-done">Thanks for your rating!</p>
      {:else}
        <button
          class="mp-install-btn"
          disabled={ratingValue < 1 || ratingSubmitting}
          onclick={submitRating}
        >
          {ratingSubmitting ? 'Submitting...' : 'Submit rating'}
        </button>
      {/if}
      <button class="mp-back-btn" onclick={() => view = 'browse'}>← Back</button>
    </div>

  {:else if view === 'publish'}
    <!-- Publish view -->
    <div class="mp-publish">
      {#if !authStore.user}
        <div class="mp-empty">Sign in to publish packages.</div>
      {:else}
        <div class="mp-pub-form">
          <div class="mp-pub-row">
            <label>Name</label>
            <input type="text" bind:value={pubName} placeholder="my-pipeline" />
          </div>
          <div class="mp-pub-row">
            <label>Description</label>
            <input type="text" bind:value={pubDesc} placeholder="What does this do?" />
          </div>
          <div class="mp-pub-row mp-pub-half">
            <div>
              <label>Type</label>
              <select bind:value={pubType}>
                <option value="agent">Agent</option>
                <option value="pipeline">Pipeline</option>
              </select>
            </div>
            <div>
              <label>Category</label>
              <select bind:value={pubCategory}>
                {#each categories.filter(c => c) as cat}
                  <option value={cat}>{cat}</option>
                {/each}
              </select>
            </div>
          </div>
          <div class="mp-pub-row mp-pub-half">
            <div>
              <label>Version</label>
              <input type="text" bind:value={pubVersion} placeholder="1.0.0" />
            </div>
            <div>
              <label>License</label>
              <input type="text" bind:value={pubLicense} placeholder="Apache-2.0" />
            </div>
          </div>
          <div class="mp-pub-row">
            <label>Tags (comma-separated)</label>
            <input type="text" bind:value={pubTags} placeholder="nestjs, auth, backend" />
          </div>
          <div class="mp-pub-row">
            <label>YAML Content</label>
            <textarea bind:value={pubContent} rows="10" placeholder="Paste your agent or pipeline YAML here..."></textarea>
          </div>
          {#if publishError}
            <p class="mp-pub-error">{publishError}</p>
          {/if}
          {#if publishSuccess}
            <p class="mp-pub-success">Published successfully!</p>
          {/if}
          <button class="mp-install-btn" disabled={publishing} onclick={publish}>
            {publishing ? 'Publishing...' : 'Publish'}
          </button>
        </div>
      {/if}
    </div>
  {/if}
</Modal>

<style>
  .mp-tabs {
    display: flex;
    gap: 2px;
    margin-bottom: 12px;
    border-bottom: 1px solid var(--weplex-border);
    padding-bottom: 8px;
  }
  .mp-tab {
    padding: 4px 14px;
    font-size: var(--weplex-text-sm);
    font-weight: 500;
    border: none;
    background: none;
    color: var(--weplex-text-muted);
    cursor: pointer;
    border-radius: var(--weplex-radius-sm);
  }
  .mp-tab:hover { color: var(--weplex-text); }
  .mp-tab.active { color: var(--weplex-text); background: var(--weplex-surface); font-weight: 600; }

  .mp-toolbar { display: flex; gap: 8px; margin-bottom: 12px; }
  .mp-search {
    flex: 1; padding: 6px 10px;
    background: var(--weplex-bg); border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-sm); color: var(--weplex-text);
    font-size: var(--weplex-text-sm); outline: none;
  }
  .mp-search:focus { border-color: var(--weplex-accent); }
  .mp-filter {
    padding: 6px 8px; background: var(--weplex-surface);
    border: 1px solid var(--weplex-border); border-radius: var(--weplex-radius-sm);
    color: var(--weplex-text); font-size: var(--weplex-text-xs); outline: none;
  }

  .mp-results { max-height: 360px; overflow-y: auto; }
  .mp-empty {
    padding: 40px; text-align: center;
    color: var(--weplex-text-muted); font-size: var(--weplex-text-sm);
  }
  .mp-error {
    padding: 8px 12px; margin-bottom: 8px;
    background: color-mix(in srgb, var(--weplex-error) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--weplex-error) 25%, transparent);
    border-radius: var(--weplex-radius-sm);
    color: var(--weplex-error); font-size: var(--weplex-text-xs);
  }
  .mp-grid {
    display: grid; grid-template-columns: repeat(auto-fill, minmax(200px, 1fr)); gap: 8px;
  }
  .mp-card {
    padding: 10px; background: var(--weplex-surface);
    border: 1px solid var(--weplex-border); border-radius: var(--weplex-radius-md);
    cursor: pointer; text-align: left; color: var(--weplex-text); width: 100%;
    font-family: inherit; font-size: inherit;
  }
  .mp-card:hover { border-color: var(--weplex-border-active); }
  .mp-card.selected { border-color: var(--weplex-accent); }

  .mp-card-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 4px; gap: 4px; }
  .mp-card-type {
    font-size: 9px; font-weight: 600; text-transform: uppercase;
    letter-spacing: 0.05em; padding: 1px 4px; border-radius: 2px;
  }
  .mp-card-type.agent { color: var(--weplex-accent); background: color-mix(in srgb, var(--weplex-accent) 15%, transparent); }
  .mp-card-type.pipeline { color: var(--weplex-active); background: color-mix(in srgb, var(--weplex-active) 15%, transparent); }
  .mp-verified { color: var(--weplex-active); font-size: 12px; }
  .mp-installed-badge {
    font-size: 9px; font-weight: 600; text-transform: uppercase;
    padding: 1px 5px; border-radius: 2px; letter-spacing: 0.04em;
    color: var(--weplex-active); background: color-mix(in srgb, var(--weplex-active) 12%, transparent);
  }

  .mp-card-name { font-size: var(--weplex-text-sm); font-weight: 600; margin: 0 0 4px; }
  .mp-card-desc {
    font-size: var(--weplex-text-xs); color: var(--weplex-text-muted);
    margin: 0 0 6px; display: -webkit-box; -webkit-line-clamp: 2;
    -webkit-box-orient: vertical; overflow: hidden;
  }
  .mp-card-meta { display: flex; gap: 8px; font-size: 10px; color: var(--weplex-text-muted); }
  .mp-card-tags { display: flex; gap: 3px; margin-top: 6px; flex-wrap: wrap; }
  .mp-tag {
    font-size: 9px; padding: 1px 4px;
    background: var(--weplex-surface-hover); border-radius: 2px; color: var(--weplex-text-muted);
  }

  /* Pagination */
  .mp-pagination {
    display: flex; align-items: center; justify-content: center;
    gap: 12px; margin-top: 12px; padding-top: 8px; border-top: 1px solid var(--weplex-border);
  }
  .mp-page-btn {
    padding: 4px 10px; font-size: var(--weplex-text-xs);
    background: var(--weplex-surface); border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-sm); color: var(--weplex-text); cursor: pointer;
  }
  .mp-page-btn:hover:not(:disabled) { border-color: var(--weplex-accent); }
  .mp-page-btn:disabled { opacity: 0.4; cursor: not-allowed; }
  .mp-page-info { font-size: var(--weplex-text-xs); color: var(--weplex-text-muted); }

  /* Detail */
  .mp-detail { margin-top: 12px; padding: 12px; border-top: 1px solid var(--weplex-border); }
  .mp-detail-header { display: flex; align-items: baseline; gap: 8px; margin-bottom: 6px; }
  .mp-detail-header h2 { font-size: var(--weplex-text-md); font-weight: 600; margin: 0; }
  .mp-detail-version { font-size: var(--weplex-text-xs); color: var(--weplex-text-muted); }
  .mp-detail-desc { font-size: var(--weplex-text-sm); color: var(--weplex-text-secondary); margin: 0 0 8px; }
  .mp-detail-stats { display: flex; gap: 12px; font-size: var(--weplex-text-xs); color: var(--weplex-text-muted); margin-bottom: 8px; }
  .mp-detail-tags { display: flex; gap: 4px; flex-wrap: wrap; margin-bottom: 12px; }
  .mp-detail-actions { display: flex; gap: 8px; }

  .mp-install-btn {
    padding: 8px 20px; background: var(--weplex-accent); color: white;
    border: none; border-radius: var(--weplex-radius-sm);
    font-size: var(--weplex-text-sm); font-weight: 600; cursor: pointer;
  }
  .mp-install-btn:hover:not(:disabled) { background: var(--weplex-accent-hover); }
  .mp-install-btn.installing { opacity: 0.6; cursor: wait; }
  .mp-install-btn.success { background: var(--weplex-active); }
  .mp-install-btn:disabled { cursor: not-allowed; opacity: 0.5; }

  .mp-rate-btn {
    padding: 8px 16px; background: var(--weplex-surface);
    border: 1px solid var(--weplex-border); border-radius: var(--weplex-radius-sm);
    font-size: var(--weplex-text-sm); color: var(--weplex-text); cursor: pointer;
  }
  .mp-rate-btn:hover { border-color: var(--weplex-accent); }

  /* Rating view */
  .mp-rate-view { text-align: center; padding: 20px 0; }
  .mp-rate-view h3 { margin: 0 0 4px; font-size: var(--weplex-text-md); }
  .mp-rate-hint { font-size: var(--weplex-text-sm); color: var(--weplex-text-muted); margin: 0 0 16px; }
  .mp-stars { display: flex; justify-content: center; gap: 4px; margin-bottom: 16px; }
  .mp-star {
    font-size: 28px; background: none; border: none;
    color: var(--weplex-border); cursor: pointer; transition: color 0.1s;
  }
  .mp-star.filled { color: var(--weplex-warning, #f59e0b); }
  .mp-star:hover { transform: scale(1.15); }
  .mp-rate-done { color: var(--weplex-active); font-size: var(--weplex-text-sm); margin-bottom: 12px; }
  .mp-back-btn {
    margin-top: 12px; padding: 4px 12px; font-size: var(--weplex-text-xs);
    background: none; border: none; color: var(--weplex-text-muted); cursor: pointer;
  }
  .mp-back-btn:hover { color: var(--weplex-text); }

  :global(.mp-modal) { width: 760px; max-width: 90vw; }

  /* Publish view */
  .mp-publish { padding: 4px 0; }
  .mp-pub-form { display: flex; flex-direction: column; gap: 10px; }
  .mp-pub-row { display: flex; flex-direction: column; gap: 3px; }
  .mp-pub-row label {
    font-size: var(--weplex-text-xs); font-weight: 600;
    text-transform: uppercase; letter-spacing: 0.04em; color: var(--weplex-text-muted);
  }
  .mp-pub-row input, .mp-pub-row select, .mp-pub-row textarea {
    padding: 6px 10px; background: var(--weplex-bg);
    border: 1px solid var(--weplex-border); border-radius: var(--weplex-radius-sm);
    color: var(--weplex-text); font-size: var(--weplex-text-sm);
    font-family: inherit; outline: none;
  }
  .mp-pub-row input:focus, .mp-pub-row select:focus, .mp-pub-row textarea:focus {
    border-color: var(--weplex-accent);
  }
  .mp-pub-row textarea {
    font-family: var(--weplex-font-mono); font-size: 11px; resize: vertical;
  }
  .mp-pub-half { flex-direction: row; gap: 10px; }
  .mp-pub-half > div { flex: 1; display: flex; flex-direction: column; gap: 3px; }
  .mp-pub-error { color: var(--weplex-error); font-size: var(--weplex-text-xs); margin: 0; }
  .mp-pub-success { color: var(--weplex-active); font-size: var(--weplex-text-sm); margin: 0; }
</style>

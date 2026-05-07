<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { authStore } from '../../stores/authStore.svelte';
  import { profileStore } from '../../stores/profileStore.svelte';
  import { lockfileStore } from '../../stores/lockfileStore.svelte';
  import { guardStore } from '../../stores/guardStore.svelte';
  import { settingsStore } from '../../stores/settingsStore.svelte';
  import { schedule as scheduleCompile } from '../../utils/compileScheduler';
  import type { MutationReport, ResourceKind } from '../../types/lockfile';
  import {
    searchPackages,
    installPackage as apiInstall,
    ratePackage,
    publishPackage,
    type MarketplacePackage,
  } from '../../services/marketplaceService';
  import { federationService } from '../../services/federationService';
  import type {
    FederatedPackSummaryDto,
    ListFederatedFilters,
    FederatedSort,
    ResourceKindFederated,
    ScoreLevel,
  } from '../../types/federation';
  import { Select } from '../ui';
  import FederatedPackCard from './FederatedPackCard.svelte';
  import FederatedPackDetail from './FederatedPackDetail.svelte';

  // ─── View ──────────────────────────────────────────────────────────
  // Federated is the first (and default) tab so the cross-agent
  // marketplace is the headline experience. Browse / Publish keep the
  // legacy single-resource flow alive for users with packages already
  // shipped to the WP marketplace.

  let view = $state<'federated' | 'browse' | 'publish' | 'rate'>('federated');

  // ─── Federated tab state ───────────────────────────────────────────

  interface FederatedState {
    packs: FederatedPackSummaryDto[];
    total: number;
    loading: boolean;
    /** Distinguishes "real error" from "marketplace is offline". */
    error: string | null;
    offline: boolean;
    filters: ListFederatedFilters;
    staleAt: string | null;
    page: number;
  }

  const FED_PAGE_SIZE = 20;

  let federatedState = $state<FederatedState>({
    packs: [],
    total: 0,
    loading: false,
    error: null,
    offline: false,
    filters: { sort: 'stars' },
    staleAt: null,
    page: 0,
  });

  let federatedQueryInput = $state('');

  let detailPackId = $state<string | null>(null);

  let federatedTotalPages = $derived(
    Math.max(1, Math.ceil(federatedState.total / FED_PAGE_SIZE)),
  );

  // ─── Federated tab — data loading ──────────────────────────────────

  async function loadFederated() {
    federatedState = { ...federatedState, loading: true, error: null };
    try {
      const data = await federationService.list({
        ...federatedState.filters,
        limit: FED_PAGE_SIZE,
        offset: federatedState.page * FED_PAGE_SIZE,
      });
      if (!data) {
        federatedState = {
          ...federatedState,
          loading: false,
          offline: true,
          packs: [],
          total: 0,
          staleAt: null,
        };
        return;
      }
      federatedState = {
        ...federatedState,
        loading: false,
        offline: false,
        packs: data.packs,
        total: data.total,
        staleAt: data.staleAt,
      };
    } catch (e) {
      federatedState = {
        ...federatedState,
        loading: false,
        error: e instanceof Error ? e.message : String(e),
      };
    }
  }

  let federatedSearchTimer: ReturnType<typeof setTimeout> | null = null;
  function debouncedFederatedSearch() {
    if (federatedSearchTimer) clearTimeout(federatedSearchTimer);
    federatedSearchTimer = setTimeout(() => {
      federatedState = {
        ...federatedState,
        page: 0,
        filters: { ...federatedState.filters, q: federatedQueryInput || undefined },
      };
      loadFederated();
    }, 300);
  }

  function setFederatedKind(kind: ResourceKindFederated | '') {
    federatedState = {
      ...federatedState,
      page: 0,
      filters: { ...federatedState.filters, kind: kind || undefined },
    };
    loadFederated();
  }

  function setFederatedScore(score: ScoreLevel | '') {
    federatedState = {
      ...federatedState,
      page: 0,
      filters: { ...federatedState.filters, score: score || undefined },
    };
    loadFederated();
  }

  function setFederatedSort(sort: FederatedSort) {
    federatedState = {
      ...federatedState,
      page: 0,
      filters: { ...federatedState.filters, sort },
    };
    loadFederated();
  }

  function nextFederatedPage() {
    if (federatedState.page < federatedTotalPages - 1) {
      federatedState = { ...federatedState, page: federatedState.page + 1 };
      loadFederated();
    }
  }
  function prevFederatedPage() {
    if (federatedState.page > 0) {
      federatedState = { ...federatedState, page: federatedState.page - 1 };
      loadFederated();
    }
  }

  function openPackDetail(packId: string) {
    detailPackId = packId;
  }
  function closePackDetail() {
    detailPackId = null;
    // Pack details often install resources — refresh stale list so
    // counts/etc stay current (best effort; cheap call).
    loadFederated();
  }

  /**
   * "stale since X" helper. The server tags responses where any record
   * is older than the freshness window. Surface it so users know
   * they're looking at a snapshot, not live data.
   */
  function staleAgo(iso: string): string {
    const then = new Date(iso).getTime();
    if (!Number.isFinite(then)) return iso;
    const diff = Date.now() - then;
    const day = 86_400_000;
    const hour = 3_600_000;
    if (diff > day) return `${Math.floor(diff / day)}d ago`;
    if (diff > hour) return `${Math.floor(diff / hour)}h ago`;
    return 'recently';
  }

  // ─── Legacy "Browse" tab (single-resource marketplace) ─────────────

  let query = $state('');
  let category = $state('');
  let typeFilter = $state('');
  let sort = $state('popular');
  let packages = $state<MarketplacePackage[]>([]);
  let total = $state(0);
  let page = $state(0);
  let loading = $state(false);
  let selectedPkg = $state<MarketplacePackage | null>(null);
  let installing = $state<string | null>(null);
  let installSuccess = $state<string | null>(null);
  let browseError = $state('');

  let installedNames = $state<Set<string>>(new Set());
  let installTargetConfigDir = $state<string>('');
  let installableProfiles = $derived(
    profileStore.profiles.filter((p) => !!p.configDir),
  );

  // Rating
  let ratingValue = $state(0);
  let ratingHover = $state(0);
  let ratingSubmitting = $state(false);
  let ratingDone = $state(false);
  let ratingError = $state('');

  // Publish
  let pubName = $state('');
  let pubDesc = $state('');
  let pubType = $state<'agent' | 'skill'>('agent');
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

  async function searchLegacy(resetPage = true) {
    if (resetPage) page = 0;
    loading = true;
    browseError = '';
    try {
      const data = await searchPackages({
        q: query || undefined,
        type: typeFilter || undefined,
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

  function nextPage() { page++; searchLegacy(false); }
  function prevPage() { if (page > 0) { page--; searchLegacy(false); } }
  let totalPages = $derived(Math.ceil(total / PAGE_SIZE));

  let searchTimer: ReturnType<typeof setTimeout> | null = null;
  function debouncedSearch() {
    if (searchTimer) clearTimeout(searchTimer);
    searchTimer = setTimeout(() => searchLegacy(), 300);
  }

  function validateYamlContent(content: string, _expectedType: string): string | null {
    if (!content || content.trim().length === 0) return 'Empty content';
    if (content.length > MAX_CONTENT_SIZE) return 'Content exceeds 100KB limit';
    const lines = content.split('\n').filter((l) => l.trim() && !l.trim().startsWith('#'));
    if (lines.length === 0) return 'No YAML content found';
    const hasKeyValue = lines.some((l) => /^\s*\w[\w\s]*:/.test(l));
    if (!hasKeyValue) return 'Invalid YAML: no key-value pairs found';
    return null;
  }

  function mapPackageKind(t: string): ResourceKind {
    if (t === 'skill') return 'skill';
    if (t === 'rule') return 'rule';
    if (t === 'command') return 'command';
    return 'agent';
  }

  function lockfileIdFor(kind: ResourceKind, name: string): string {
    const dir =
      kind === 'agent' ? 'agents'
      : kind === 'rule' ? 'rules'
      : kind === 'skill' ? 'skills'
      : 'commands';
    return `${dir}/${name}`;
  }

  async function installPackageLegacy(pkg: MarketplacePackage) {
    if (!installTargetConfigDir) {
      browseError = 'Choose a profile to install into.';
      return;
    }
    installing = pkg.id;
    browseError = '';
    try {
      const data = await apiInstall(pkg.id);
      const validationError = validateYamlContent(data.content, data.type);
      if (validationError) {
        browseError = `Invalid package content: ${validationError}`;
        return;
      }
      const kind = mapPackageKind(data.type);
      // Phase 5: pass `pack: null` for single-resource publishes so the
      // lockfile records this entry as user-published (not a federated pack).
      await invoke<MutationReport>('install_marketplace_package', {
        targetConfigDir: installTargetConfigDir,
        name: data.name,
        content: data.content,
        sidecar: null,
        kind,
        pack: null,
      });
      lockfileStore.refresh(installTargetConfigDir);
      guardStore.refresh(
        installTargetConfigDir,
        settingsStore.settings.agentshieldDeepScan,
      );
      scheduleCompile(installTargetConfigDir, {
        deepScan: settingsStore.settings.agentshieldDeepScan,
      });
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

  function openRating(pkg: MarketplacePackage) {
    selectedPkg = pkg;
    ratingValue = 0;
    ratingHover = 0;
    ratingDone = false;
    view = 'rate';
  }

  async function submitRating() {
    if (!selectedPkg || ratingValue < 1) return;
    ratingSubmitting = true;
    ratingError = '';
    try {
      await ratePackage(selectedPkg.id, ratingValue);
      ratingDone = true;
      setTimeout(() => { view = 'browse'; searchLegacy(); }, 1500);
    } catch (e: unknown) {
      ratingError = e instanceof Error ? e.message : 'Rating failed';
    } finally {
      ratingSubmitting = false;
    }
  }

  async function publish() {
    publishError = '';
    if (!pubName.trim() || !pubDesc.trim() || !pubContent.trim()) {
      publishError = 'Name, description, and content are required.';
      return;
    }
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
        searchLegacy();
      }, 2000);
    } catch (e: unknown) {
      publishError = e instanceof Error ? e.message : 'Publish failed';
    } finally {
      publishing = false;
    }
  }

  async function loadInstalled() {
    try {
      const agents: { name: string }[] = await invoke('list_agents');
      const names = new Set<string>();
      for (const a of agents) names.add(a.name);
      installedNames = names;
    } catch {
      // command may be unavailable in some contexts — non-fatal
    }
  }

  function formatRating(rating: number): string {
    return rating > 0 ? `${rating.toFixed(1)}★` : '—';
  }
  function formatDownloads(n: number): string {
    if (n >= 1000) return `${(n / 1000).toFixed(1)}K`;
    return String(n);
  }

  onMount(() => {
    // Federated tab is opened by default — kick off its load.
    loadFederated();

    // Legacy state: pre-pick install target + warm cache so the user
    // can switch tabs without a noticeable second-load.
    const def = profileStore.defaultProfile;
    if (def?.configDir) {
      installTargetConfigDir = def.configDir;
    } else {
      installTargetConfigDir = installableProfiles[0]?.configDir ?? '';
    }
    if (installTargetConfigDir) {
      lockfileStore.refresh(installTargetConfigDir);
    }
    searchLegacy();
    loadInstalled();
  });
</script>

<div class="hub-mp">
  <!-- Tab bar -->
  <div class="hub-mp-tabs" role="tablist" aria-label="Marketplace sections">
    <button
      class="hub-mp-tab"
      role="tab"
      aria-selected={view === 'federated'}
      class:active={view === 'federated'}
      onclick={() => (view = 'federated')}
    >Federated</button>
    <button
      class="hub-mp-tab"
      role="tab"
      aria-selected={view === 'browse'}
      class:active={view === 'browse'}
      onclick={() => (view = 'browse')}
    >Browse</button>
    <button
      class="hub-mp-tab"
      role="tab"
      aria-selected={view === 'publish'}
      class:active={view === 'publish'}
      onclick={() => (view = 'publish')}
    >Publish</button>
  </div>

  {#if view === 'federated'}
    <!-- Federated marketplace -->
    <div class="hub-mp-toolbar">
      <input
        class="hub-mp-search"
        type="text"
        placeholder="Search federated packs…"
        bind:value={federatedQueryInput}
        oninput={() => debouncedFederatedSearch()}
      />
      <Select
        class="hub-mp-filter"
        value={federatedState.filters.kind ?? ''}
        onchange={(v) => setFederatedKind(v as ResourceKindFederated | '')}
        options={[
          { value: '', label: 'All kinds' },
          { value: 'agent', label: 'Agents' },
          { value: 'rule', label: 'Rules' },
          { value: 'skill', label: 'Skills' },
          { value: 'command', label: 'Commands' },
        ]}
      />
      <Select
        class="hub-mp-filter"
        value={federatedState.filters.score ?? ''}
        onchange={(v) => setFederatedScore(v as ScoreLevel | '')}
        options={[
          { value: '', label: 'Any score' },
          { value: 'green', label: 'Green' },
          { value: 'yellow', label: 'Yellow' },
          { value: 'red', label: 'Red' },
        ]}
      />
      <Select
        class="hub-mp-filter"
        value={federatedState.filters.sort ?? 'stars'}
        onchange={(v) => setFederatedSort(v as FederatedSort)}
        options={[
          { value: 'stars', label: 'Most stars' },
          { value: 'newest', label: 'Newest' },
          { value: 'name', label: 'A–Z' },
        ]}
      />
    </div>

    {#if federatedState.offline}
      <div class="hub-mp-banner hub-mp-banner-offline" role="status">
        Marketplace is offline — try again in a moment.
      </div>
    {:else if federatedState.staleAt}
      <div class="hub-mp-banner hub-mp-banner-stale" role="status">
        Index is stale (oldest entry indexed {staleAgo(federatedState.staleAt)}).
        Results may not reflect upstream changes.
      </div>
    {/if}

    {#if federatedState.error}
      <div class="hub-mp-error" role="alert">{federatedState.error}</div>
    {/if}

    <div class="hub-mp-results">
      {#if federatedState.loading}
        <div class="hub-mp-empty">Loading…</div>
      {:else if federatedState.packs.length === 0}
        <div class="hub-mp-empty">
          {#if federatedState.offline}
            No data while offline.
          {:else if federatedState.filters.q || federatedState.filters.kind || federatedState.filters.score}
            No packs match the current filters.
          {:else}
            No federated packs indexed yet.
          {/if}
        </div>
      {:else}
        <div class="hub-mp-grid">
          {#each federatedState.packs as pack (pack.id)}
            <FederatedPackCard {pack} onview={openPackDetail} />
          {/each}
        </div>
        {#if federatedTotalPages > 1}
          <div class="hub-mp-pagination">
            <button
              class="hub-mp-page-btn"
              disabled={federatedState.page === 0}
              onclick={prevFederatedPage}
            >← Prev</button>
            <span class="hub-mp-page-info">
              {federatedState.page + 1} / {federatedTotalPages}
            </span>
            <button
              class="hub-mp-page-btn"
              disabled={federatedState.page >= federatedTotalPages - 1}
              onclick={nextFederatedPage}
            >Next →</button>
          </div>
        {/if}
      {/if}
    </div>

  {:else if view === 'browse'}
    <!-- Legacy single-resource marketplace -->
    <div class="hub-mp-toolbar">
      <input
        class="hub-mp-search"
        type="text"
        placeholder="Search agents & skills..."
        bind:value={query}
        oninput={() => debouncedSearch()}
      />
      <Select
        class="hub-mp-filter"
        value={typeFilter}
        onchange={(v) => { typeFilter = v; searchLegacy(); }}
        options={[
          { value: '', label: 'All types' },
          { value: 'agent', label: 'Agents' },
          { value: 'skill', label: 'Skills' },
        ]}
      />
      <Select
        class="hub-mp-filter"
        value={category}
        onchange={(v) => { category = v; searchLegacy(); }}
        options={categories.map((c) => ({ value: c, label: c || 'All categories' }))}
      />
      <Select
        class="hub-mp-filter"
        value={sort}
        onchange={(v) => { sort = v; searchLegacy(); }}
        options={[
          { value: 'popular', label: 'Popular' },
          { value: 'rating', label: 'Top rated' },
          { value: 'newest', label: 'Newest' },
          { value: 'name', label: 'A-Z' },
        ]}
      />
    </div>

    {#if installableProfiles.length > 1}
      <div class="hub-mp-install-target">
        <label for="hub-mp-install-target-select">Install to profile:</label>
        <select
          id="hub-mp-install-target-select"
          class="hub-mp-install-target-select"
          bind:value={installTargetConfigDir}
          onchange={() => installTargetConfigDir && lockfileStore.refresh(installTargetConfigDir)}
        >
          {#each installableProfiles as p (p.id)}
            <option value={p.configDir}>{p.name}</option>
          {/each}
        </select>
      </div>
    {/if}

    <div class="hub-mp-results">
      {#if browseError}
        <div class="hub-mp-error">{browseError}</div>
      {/if}
      {#if loading}
        <div class="hub-mp-empty">Loading...</div>
      {:else if packages.length === 0}
        <div class="hub-mp-empty">No packages found</div>
      {:else}
        <div class="hub-mp-grid">
          {#each packages as pkg (pkg.id)}
            <button
              class="hub-mp-card"
              class:selected={selectedPkg?.id === pkg.id}
              onclick={() => (selectedPkg = pkg)}
            >
              <div class="hub-mp-card-header">
                <span
                  class="hub-mp-card-type"
                  class:agent={pkg.type === 'agent'}
                  class:skill={pkg.type === 'skill'}
                >{pkg.type}</span>
                {#if installedNames.has(pkg.name)}
                  <span class="hub-mp-installed-badge">installed</span>
                {/if}
                {#if pkg.verified}
                  <span class="hub-mp-verified" title="Verified publisher">✓</span>
                {/if}
              </div>
              <h3 class="hub-mp-card-name">{pkg.name}</h3>
              <p class="hub-mp-card-desc">{pkg.description}</p>
              <div class="hub-mp-card-meta">
                <span>{formatRating(pkg.rating)}</span>
                <span>{formatDownloads(pkg.downloads)} installs</span>
                <span>v{pkg.version}</span>
              </div>
              <div class="hub-mp-card-tags">
                {#each pkg.tags.slice(0, 3) as tag}
                  <span class="hub-mp-tag">{tag}</span>
                {/each}
              </div>
            </button>
          {/each}
        </div>

        {#if totalPages > 1}
          <div class="hub-mp-pagination">
            <button class="hub-mp-page-btn" disabled={page === 0} onclick={prevPage}>← Prev</button>
            <span class="hub-mp-page-info">{page + 1} / {totalPages}</span>
            <button class="hub-mp-page-btn" disabled={page >= totalPages - 1} onclick={nextPage}>Next →</button>
          </div>
        {/if}
      {/if}
    </div>

    {#if selectedPkg}
      <div class="hub-mp-detail">
        <div class="hub-mp-detail-header">
          <h2>{selectedPkg.name}</h2>
          <span class="hub-mp-detail-version">v{selectedPkg.version}</span>
          {#if installedNames.has(selectedPkg.name)}
            <span class="hub-mp-installed-badge">installed</span>
          {/if}
        </div>
        <p class="hub-mp-detail-desc">{selectedPkg.description}</p>
        <div class="hub-mp-detail-stats">
          <span>{formatRating(selectedPkg.rating)} ({selectedPkg.ratingCount} ratings)</span>
          <span>{formatDownloads(selectedPkg.downloads)} installs</span>
          <span>by {selectedPkg.author?.displayName}</span>
        </div>
        <div class="hub-mp-detail-tags">
          {#each selectedPkg.tags as tag}
            <span class="hub-mp-tag">{tag}</span>
          {/each}
        </div>
        <div class="hub-mp-detail-actions">
          {#if selectedPkg}
            {@const existingEntry = installTargetConfigDir
              ? lockfileStore.entryForResource(
                  installTargetConfigDir,
                  lockfileIdFor(mapPackageKind(selectedPkg.type), selectedPkg.name),
                )
              : null}
            {@const existingVersion = existingEntry?.version ?? null}
            {@const isDifferentVersion =
              existingVersion !== null && existingVersion !== selectedPkg.version}
            <button
              class="hub-mp-install-btn"
              class:installing={installing === selectedPkg.id}
              class:success={installSuccess === selectedPkg.id}
              onclick={() => selectedPkg && installPackageLegacy(selectedPkg)}
              disabled={installing !== null || !installTargetConfigDir}
            >
              {#if installSuccess === selectedPkg.id}
                Installed ✓
              {:else if installing === selectedPkg.id}
                Installing...
              {:else if isDifferentVersion}
                Update from v{existingVersion}
              {:else if existingEntry}
                Reinstall
              {:else}
                Install
              {/if}
            </button>
            {#if authStore.user}
              <button class="hub-mp-rate-btn" onclick={() => openRating(selectedPkg!)}>Rate</button>
            {/if}
          {/if}
        </div>
      </div>
    {/if}

  {:else if view === 'rate' && selectedPkg}
    <div class="hub-mp-rate-view">
      <h3>Rate {selectedPkg.name}</h3>
      <p class="hub-mp-rate-hint">How useful is this package?</p>
      <div class="hub-mp-stars">
        {#each [1, 2, 3, 4, 5] as star}
          <button
            class="hub-mp-star"
            class:filled={star <= (ratingHover || ratingValue)}
            onmouseenter={() => (ratingHover = star)}
            onmouseleave={() => (ratingHover = 0)}
            onclick={() => (ratingValue = star)}
          >★</button>
        {/each}
      </div>
      {#if ratingError}<p class="hub-mp-pub-error">{ratingError}</p>{/if}
      {#if ratingDone}
        <p class="hub-mp-rate-done">Thanks for your rating!</p>
      {:else}
        <button
          class="hub-mp-install-btn"
          disabled={ratingValue < 1 || ratingSubmitting}
          onclick={submitRating}
        >
          {ratingSubmitting ? 'Submitting...' : 'Submit rating'}
        </button>
      {/if}
      <button class="hub-mp-back-btn" onclick={() => (view = 'browse')}>← Back</button>
    </div>

  {:else if view === 'publish'}
    <div class="hub-mp-publish">
      {#if !authStore.user}
        <div class="hub-mp-empty">Sign in to publish packages.</div>
      {:else}
        <div class="hub-mp-pub-form">
          <label class="hub-mp-pub-row">
            <span>Name</span>
            <input type="text" bind:value={pubName} placeholder="my-agent" />
          </label>
          <label class="hub-mp-pub-row">
            <span>Description</span>
            <input type="text" bind:value={pubDesc} placeholder="What does this do?" />
          </label>
          <div class="hub-mp-pub-row hub-mp-pub-half">
            <label>
              <span>Type</span>
              <Select
                value={pubType}
                onchange={(v) => { pubType = v as 'agent' | 'skill'; }}
                options={[
                  { value: 'agent', label: 'Agent' },
                  { value: 'skill', label: 'Skill' },
                ]}
              />
            </label>
            <label>
              <span>Category</span>
              <Select
                value={pubCategory}
                onchange={(v) => { pubCategory = v; }}
                options={categories.filter((c) => c).map((c) => ({ value: c, label: c }))}
              />
            </label>
          </div>
          <div class="hub-mp-pub-row hub-mp-pub-half">
            <label>
              <span>Version</span>
              <input type="text" bind:value={pubVersion} placeholder="1.0.0" />
            </label>
            <label>
              <span>License</span>
              <input type="text" bind:value={pubLicense} placeholder="Apache-2.0" />
            </label>
          </div>
          <label class="hub-mp-pub-row">
            <span>Tags (comma-separated)</span>
            <input type="text" bind:value={pubTags} placeholder="nestjs, auth, backend" />
          </label>
          <label class="hub-mp-pub-row">
            <span>YAML Content</span>
            <textarea bind:value={pubContent} rows="10" placeholder="Paste your agent or skill YAML here..."></textarea>
          </label>
          {#if publishError}<p class="hub-mp-pub-error">{publishError}</p>{/if}
          {#if publishSuccess}<p class="hub-mp-pub-success">Published successfully!</p>{/if}
          <button class="hub-mp-install-btn" disabled={publishing} onclick={publish}>
            {publishing ? 'Publishing...' : 'Publish'}
          </button>
        </div>
      {/if}
    </div>
  {/if}
</div>

{#if detailPackId}
  <FederatedPackDetail packId={detailPackId} onclose={closePackDetail} />
{/if}

<style>
  .hub-mp {
    width: 100%;
    height: 100%;
    padding: 32px 40px;
    overflow-y: auto;
    background: var(--weplex-bg);
  }

  .hub-mp-tabs {
    display: flex;
    gap: 2px;
    margin-bottom: 12px;
    border-bottom: 1px solid var(--weplex-border);
    padding-bottom: 8px;
  }
  .hub-mp-tab {
    padding: 4px 14px;
    font-size: var(--weplex-text-sm);
    font-weight: 500;
    border: none;
    background: none;
    color: var(--weplex-text-muted);
    cursor: pointer;
    border-radius: var(--weplex-radius-sm);
  }
  .hub-mp-tab:hover { color: var(--weplex-text); }
  .hub-mp-tab.active {
    color: var(--weplex-text);
    background: var(--weplex-surface);
    font-weight: 600;
  }

  .hub-mp-toolbar {
    display: flex;
    gap: 8px;
    margin-bottom: 12px;
  }
  .hub-mp-search {
    flex: 1;
    padding: 6px 10px;
    background: var(--weplex-bg);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-sm);
    color: var(--weplex-text);
    font-size: var(--weplex-text-sm);
    outline: none;
  }
  .hub-mp-search:focus { border-color: var(--weplex-accent); }
  .hub-mp-toolbar :global(.hub-mp-filter) {
    font-size: var(--weplex-text-xs);
  }

  .hub-mp-banner {
    padding: 6px 10px;
    margin-bottom: 8px;
    border-radius: var(--weplex-radius-sm);
    font-size: var(--weplex-text-xs);
  }
  .hub-mp-banner-offline {
    color: var(--weplex-error);
    background: color-mix(in srgb, var(--weplex-error) 8%, transparent);
    border: 1px solid color-mix(in srgb, var(--weplex-error) 25%, transparent);
  }
  .hub-mp-banner-stale {
    color: var(--weplex-warning, #f59e0b);
    background: color-mix(in srgb, var(--weplex-warning, #f59e0b) 8%, transparent);
    border: 1px solid color-mix(in srgb, var(--weplex-warning, #f59e0b) 25%, transparent);
  }

  .hub-mp-error {
    padding: 8px 12px;
    margin-bottom: 8px;
    background: color-mix(in srgb, var(--weplex-error) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--weplex-error) 25%, transparent);
    border-radius: var(--weplex-radius-sm);
    color: var(--weplex-error);
    font-size: var(--weplex-text-xs);
  }

  .hub-mp-results { max-height: 460px; overflow-y: auto; }
  .hub-mp-empty {
    padding: 40px;
    text-align: center;
    color: var(--weplex-text-muted);
    font-size: var(--weplex-text-sm);
  }
  .hub-mp-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 8px;
  }

  .hub-mp-pagination {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 12px;
    margin-top: 12px;
    padding-top: 8px;
    border-top: 1px solid var(--weplex-border);
  }
  .hub-mp-page-btn {
    padding: 4px 10px;
    font-size: var(--weplex-text-xs);
    background: var(--weplex-surface);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-sm);
    color: var(--weplex-text);
    cursor: pointer;
  }
  .hub-mp-page-btn:hover:not(:disabled) { border-color: var(--weplex-accent); }
  .hub-mp-page-btn:disabled { opacity: 0.4; cursor: not-allowed; }
  .hub-mp-page-info { font-size: var(--weplex-text-xs); color: var(--weplex-text-muted); }

  /* ── Legacy "Browse" tab styles ────────────────────────────────── */
  .hub-mp-install-target {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 12px;
    padding: 6px 10px;
    background: var(--weplex-surface);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-sm);
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
  }
  .hub-mp-install-target label { font-weight: 600; }
  .hub-mp-install-target-select {
    padding: 4px 8px;
    background: var(--weplex-bg);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-sm);
    color: var(--weplex-text);
    font-size: var(--weplex-text-xs);
    font-family: inherit;
    outline: none;
    cursor: pointer;
  }

  .hub-mp-card {
    padding: 10px;
    background: var(--weplex-surface);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-md);
    cursor: pointer;
    text-align: left;
    color: var(--weplex-text);
    width: 100%;
    font-family: inherit;
    font-size: inherit;
  }
  .hub-mp-card:hover { border-color: var(--weplex-border-active); }
  .hub-mp-card.selected { border-color: var(--weplex-accent); }
  .hub-mp-card-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 4px;
    gap: 4px;
  }
  .hub-mp-card-type {
    font-size: 9px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    padding: 1px 4px;
    border-radius: 2px;
  }
  .hub-mp-card-type.agent {
    color: var(--weplex-accent);
    background: color-mix(in srgb, var(--weplex-accent) 15%, transparent);
  }
  .hub-mp-card-type.skill {
    color: var(--weplex-active);
    background: color-mix(in srgb, var(--weplex-active) 15%, transparent);
  }
  .hub-mp-verified { color: var(--weplex-active); font-size: 12px; }
  .hub-mp-installed-badge {
    font-size: 9px;
    font-weight: 600;
    text-transform: uppercase;
    padding: 1px 5px;
    border-radius: 2px;
    letter-spacing: 0.04em;
    color: var(--weplex-active);
    background: color-mix(in srgb, var(--weplex-active) 12%, transparent);
  }
  .hub-mp-card-name {
    font-size: var(--weplex-text-sm);
    font-weight: 600;
    margin: 0 0 4px;
  }
  .hub-mp-card-desc {
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
    margin: 0 0 6px;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .hub-mp-card-meta {
    display: flex;
    gap: 8px;
    font-size: 10px;
    color: var(--weplex-text-muted);
  }
  .hub-mp-card-tags {
    display: flex;
    gap: 3px;
    margin-top: 6px;
    flex-wrap: wrap;
  }
  .hub-mp-tag {
    font-size: 9px;
    padding: 1px 4px;
    background: var(--weplex-surface-hover);
    border-radius: 2px;
    color: var(--weplex-text-muted);
  }

  .hub-mp-detail {
    margin-top: 12px;
    padding: 12px;
    border-top: 1px solid var(--weplex-border);
  }
  .hub-mp-detail-header {
    display: flex;
    align-items: baseline;
    gap: 8px;
    margin-bottom: 6px;
  }
  .hub-mp-detail-header h2 {
    font-size: var(--weplex-text-md);
    font-weight: 600;
    margin: 0;
  }
  .hub-mp-detail-version { font-size: var(--weplex-text-xs); color: var(--weplex-text-muted); }
  .hub-mp-detail-desc { font-size: var(--weplex-text-sm); color: var(--weplex-text-secondary); margin: 0 0 8px; }
  .hub-mp-detail-stats {
    display: flex;
    gap: 12px;
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
    margin-bottom: 8px;
  }
  .hub-mp-detail-tags {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
    margin-bottom: 12px;
  }
  .hub-mp-detail-actions { display: flex; gap: 8px; }

  .hub-mp-install-btn {
    padding: 8px 20px;
    background: var(--weplex-accent);
    color: white;
    border: none;
    border-radius: var(--weplex-radius-sm);
    font-size: var(--weplex-text-sm);
    font-weight: 600;
    cursor: pointer;
  }
  .hub-mp-install-btn:hover:not(:disabled) { background: var(--weplex-accent-hover); }
  .hub-mp-install-btn.installing { opacity: 0.6; cursor: wait; }
  .hub-mp-install-btn.success { background: var(--weplex-active); }
  .hub-mp-install-btn:disabled { cursor: not-allowed; opacity: 0.5; }

  .hub-mp-rate-btn {
    padding: 8px 16px;
    background: var(--weplex-surface);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-sm);
    font-size: var(--weplex-text-sm);
    color: var(--weplex-text);
    cursor: pointer;
  }
  .hub-mp-rate-btn:hover { border-color: var(--weplex-accent); }

  /* ── Rating view ────────────────────────────────────────────── */
  .hub-mp-rate-view { text-align: center; padding: 20px 0; }
  .hub-mp-rate-view h3 { margin: 0 0 4px; font-size: var(--weplex-text-md); }
  .hub-mp-rate-hint { font-size: var(--weplex-text-sm); color: var(--weplex-text-muted); margin: 0 0 16px; }
  .hub-mp-stars { display: flex; justify-content: center; gap: 4px; margin-bottom: 16px; }
  .hub-mp-star {
    font-size: 28px;
    background: none;
    border: none;
    color: var(--weplex-border);
    cursor: pointer;
    transition: color 0.1s;
  }
  .hub-mp-star.filled { color: var(--weplex-warning, #f59e0b); }
  .hub-mp-star:hover { transform: scale(1.15); }
  .hub-mp-rate-done { color: var(--weplex-active); font-size: var(--weplex-text-sm); margin-bottom: 12px; }
  .hub-mp-back-btn {
    margin-top: 12px;
    padding: 4px 12px;
    font-size: var(--weplex-text-xs);
    background: none;
    border: none;
    color: var(--weplex-text-muted);
    cursor: pointer;
  }
  .hub-mp-back-btn:hover { color: var(--weplex-text); }

  /* ── Publish view ───────────────────────────────────────────── */
  .hub-mp-publish { padding: 4px 0; }
  .hub-mp-pub-form { display: flex; flex-direction: column; gap: 10px; }
  .hub-mp-pub-row { display: flex; flex-direction: column; gap: 3px; }
  .hub-mp-pub-row > span,
  .hub-mp-pub-half label > span {
    font-size: var(--weplex-text-xs);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--weplex-text-muted);
  }
  .hub-mp-pub-row input,
  .hub-mp-pub-row textarea {
    padding: 6px 10px;
    background: var(--weplex-bg);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-sm);
    color: var(--weplex-text);
    font-size: var(--weplex-text-sm);
    font-family: inherit;
    outline: none;
  }
  .hub-mp-pub-row input:focus,
  .hub-mp-pub-row textarea:focus { border-color: var(--weplex-accent); }
  .hub-mp-pub-row textarea {
    font-family: var(--weplex-font-mono);
    font-size: 11px;
    resize: vertical;
  }
  .hub-mp-pub-half { flex-direction: row; gap: 10px; }
  .hub-mp-pub-half > label { flex: 1; display: flex; flex-direction: column; gap: 3px; }
  .hub-mp-pub-error { color: var(--weplex-error); font-size: var(--weplex-text-xs); margin: 0; }
  .hub-mp-pub-success { color: var(--weplex-active); font-size: var(--weplex-text-sm); margin: 0; }
</style>

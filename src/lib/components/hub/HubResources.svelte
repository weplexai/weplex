<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { Button, Modal } from '../ui';
  import { resourceStore, type Resource, type ResourceType } from '../../stores/resourceStore.svelte';
  import { profileStore } from '../../stores/profileStore.svelte';
  import { Plus, Share2, Trash2, AlertTriangle, Package, RefreshCw } from 'lucide-svelte';
  import { modelShort, modelClass, initial } from '../overlays/helpers';
  import AgentDetail from '../overlays/AgentDetail.svelte';
  import type { AgentConfig } from '../overlays/types';

  // Active tab
  let activeTab = $state<ResourceType>('agent');

  // Selected resource
  let selectedResource = $state<Resource | null>(null);

  // Agent detail (loaded on selection for full view)
  let selectedAgentConfig = $state<AgentConfig | null>(null);

  // Editor state
  let editMode = $state<'view' | 'new'>('view');
  let editName = $state('');
  let editDescription = $state('');
  let editContent = $state('');
  let editError = $state<string | null>(null);

  // Delete confirmation
  let confirmDelete = $state<Resource | null>(null);

  // Operation state (prevents double-clicks, shows feedback)
  let operating = $state(false);
  let toast = $state<{ type: 'success' | 'error'; text: string } | null>(null);

  // Multiple profiles available for sharing
  let hasMultipleProfiles = $derived(profileStore.profiles.length > 1);

  // Filtered resources by active tab
  let tabResources = $derived(
    resourceStore.resources.filter((r) => r.resourceType === activeTab),
  );

  // Grouped
  let sharedResources = $derived(
    tabResources.filter((r) => r.origin === 'weplex-managed'),
  );
  let marketplaceResources = $derived(
    tabResources.filter((r) => r.origin === 'marketplace'),
  );
  let profileLocalResources = $derived(
    tabResources.filter((r) => r.origin === 'profile-local'),
  );

  // Group profile-local by profile
  let profileGroups = $derived.by(() => {
    const groups: Record<string, Resource[]> = {};
    for (const r of profileLocalResources) {
      const key = r.profileName || 'Unknown';
      if (!groups[key]) groups[key] = [];
      groups[key].push(r);
    }
    return Object.entries(groups).sort(([a], [b]) => a.localeCompare(b));
  });

  // Tab labels
  const tabs: { id: ResourceType; label: string }[] = [
    { id: 'agent', label: 'Agents' },
    { id: 'rule', label: 'Rules' },
    { id: 'skill', label: 'Skills' },
  ];

  onMount(() => {
    resourceStore.discover();
  });

  function selectResource(resource: Resource) {
    selectedResource = resource;
    editMode = 'view';
    selectedAgentConfig = null;

    // For agents, load full config for detail view
    if (resource.resourceType === 'agent') {
      loadAgentDetail(resource.filePath);
    }
  }

  async function loadAgentDetail(filePath: string) {
    try {
      // Read file content and parse as agent
      const agents = await invoke<AgentConfig[]>('list_agents');
      const weplexAgents = await invoke<AgentConfig[]>('list_weplex_agents');
      const all = [...agents, ...weplexAgents];
      selectedAgentConfig = all.find((a) => a.file_path === filePath) || null;
    } catch {
      selectedAgentConfig = null;
    }
  }

  function startNew() {
    selectedResource = null;
    selectedAgentConfig = null;
    editMode = 'new';
    editName = '';
    editDescription = '';
    editContent = activeTab === 'agent'
      ? '---\nname: \ndescription: \nmodel: sonnet\ntools: [Read, Grep, Glob, Edit, Write, Bash]\n---\n\nYour instructions here...'
      : '---\nname: \ndescription: \n---\n\nContent here...';
    editError = null;
  }

  function showToast(type: 'success' | 'error', text: string) {
    toast = { type, text };
    setTimeout(() => { toast = null; }, 3000);
  }

  /** Refresh selectedResource reference after discover (prevents stale data). */
  function refreshSelection() {
    if (selectedResource) {
      const updated = resourceStore.resources.find(
        (r) => r.filePath === selectedResource!.filePath,
      );
      if (updated) {
        selectedResource = updated;
      } else {
        selectedResource = null;
        selectedAgentConfig = null;
      }
    }
  }

  async function saveNew() {
    if (!editName.trim()) {
      editError = 'Name is required';
      return;
    }
    operating = true;
    try {
      await resourceStore.create(activeTab, editName.trim(), editContent);
      editMode = 'view';
      editError = null;
      // Name may be sanitized by Rust — find by type in latest resources
      const sanitized = editName.trim().replace(/[^a-zA-Z0-9\-_]/g, (c) => c === ' ' ? '-' : '_');
      const found = resourceStore.resources.find(
        (r) => r.name === sanitized && r.resourceType === activeTab,
      );
      if (found) selectResource(found);
      showToast('success', 'Created');
    } catch (e: unknown) {
      editError = e instanceof Error ? e.message : String(e);
    } finally {
      operating = false;
    }
  }

  async function shareResource(resource: Resource) {
    if (operating) return;
    operating = true;
    try {
      await resourceStore.share(resource);
      refreshSelection();
      const count = profileStore.profiles.length;
      showToast('success', `Shared to ${count} profile${count > 1 ? 's' : ''}`);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      showToast('error', `Share failed: ${msg}`);
    } finally {
      operating = false;
    }
  }

  async function deleteResource() {
    if (!confirmDelete || operating) return;
    operating = true;
    try {
      await resourceStore.delete(confirmDelete.name, confirmDelete.resourceType);
      if (selectedResource?.name === confirmDelete.name) {
        selectedResource = null;
        selectedAgentConfig = null;
      }
      showToast('success', 'Deleted');
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      showToast('error', `Delete failed: ${msg}`);
    } finally {
      confirmDelete = null;
      operating = false;
    }
  }

  function resourceIcon(r: Resource): string {
    if (r.origin === 'marketplace') return '⬡';
    if (r.origin === 'weplex-managed') return '◆';
    return initial(r.name);
  }

  function resourceIconClass(r: Resource): string {
    if (r.origin === 'marketplace') return 'marketplace';
    if (r.origin === 'weplex-managed') return 'shared';
    return '';
  }
</script>

<div class="resources-layout">
  <!-- Sidebar -->
  <div class="resources-sidebar">
    <div class="resources-tabs">
      {#each tabs as tab}
        <button
          class="tab-btn"
          class:active={activeTab === tab.id}
          onclick={() => { activeTab = tab.id; selectedResource = null; selectedAgentConfig = null; editMode = 'view'; }}
        >
          {tab.label}
        </button>
      {/each}
    </div>

    <nav class="resources-nav">
      {#if resourceStore.loading}
        <div class="resources-empty">Loading...</div>
      {:else}
        <!-- Shared resources -->
        {#if sharedResources.length > 0}
          <div class="nav-section-label">Shared</div>
          {#each sharedResources as r}
            <button
              class="resource-row"
              class:selected={selectedResource?.filePath === r.filePath && editMode === 'view'}
              onclick={() => selectResource(r)}
            >
              <span class="row-icon shared">{resourceIcon(r)}</span>
              <span class="row-name">{r.name}</span>
              {#if r.isOutdated}
                <span class="row-badge drift" title="Modified locally">!</span>
              {/if}
            </button>
          {/each}
        {/if}

        <!-- Profile-local resources (grouped by profile) -->
        {#each profileGroups as [profileName, resources]}
          <div class="nav-section-label">{profileName} only</div>
          {#each resources as r}
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div
              class="resource-row"
              class:selected={selectedResource?.filePath === r.filePath && editMode === 'view'}
              onclick={() => selectResource(r)}
              onkeydown={(e) => { if (e.key === 'Enter') selectResource(r); }}
              role="button"
              tabindex="0"
            >
              <span class="row-icon">{initial(r.name)}</span>
              <span class="row-name">{r.name}</span>
              {#if hasMultipleProfiles}
                <button
                  class="row-action share-btn"
                  title="Share to all profiles"
                  disabled={operating}
                  onclick|stopPropagation={() => shareResource(r)}
                >
                  <Share2 size={11} />
                </button>
              {/if}
            </div>
          {/each}
        {/each}

        <!-- Marketplace resources -->
        {#if marketplaceResources.length > 0}
          <div class="nav-section-label">Marketplace</div>
          {#each marketplaceResources as r}
            <button
              class="resource-row"
              class:selected={selectedResource?.filePath === r.filePath && editMode === 'view'}
              onclick={() => selectResource(r)}
            >
              <span class="row-icon marketplace">⬡</span>
              <span class="row-name">{r.name}</span>
            </button>
          {/each}
        {/if}

        {#if tabResources.length === 0}
          <div class="resources-empty">
            No {tabs.find((t) => t.id === activeTab)?.label.toLowerCase()} yet.
          </div>
        {/if}
      {/if}
    </nav>

    <div class="resources-footer">
      <button class="resources-action-btn" onclick={startNew}>
        <Plus size={13} />
        <span>New {tabs.find((t) => t.id === activeTab)?.label.slice(0, -1)}</span>
      </button>
      <button class="resources-action-btn" onclick={() => resourceStore.discover()}>
        <RefreshCw size={13} />
        <span>Refresh</span>
      </button>
    </div>
  </div>

  <!-- Main content -->
  <div class="resources-main">
    {#if resourceStore.loading}
      <div class="resources-center-msg">Loading...</div>

    {:else if editMode === 'new'}
      <div class="editor">
        <div class="editor-header">
          <h2>New {tabs.find((t) => t.id === activeTab)?.label.slice(0, -1)}</h2>
          <div class="editor-actions">
            <Button variant="secondary" onclick={() => { editMode = 'view'; editError = null; }}>Cancel</Button>
            <Button variant="primary" onclick={saveNew}>Save</Button>
          </div>
        </div>

        {#if editError}
          <div class="editor-error"><AlertTriangle size={13} />{editError}</div>
        {/if}

        <div class="editor-form">
          <div class="form-row">
            <label>Name
              <input type="text" bind:value={editName} placeholder="my-{activeTab}" />
            </label>
          </div>
          <div class="form-row">
            <label>Content (.md)
              <textarea bind:value={editContent} rows={20} placeholder="---\nname: ...\n---\n\nInstructions..."></textarea>
            </label>
          </div>
        </div>
      </div>

    {:else if selectedResource}
      <div class="detail">
        <div class="detail-header">
          <div class="detail-title-row">
            <span class="detail-icon {resourceIconClass(selectedResource)}">{resourceIcon(selectedResource)}</span>
            <h2>{selectedResource.name}</h2>
            {#if selectedResource.origin === 'weplex-managed'}
              <span class="detail-badge shared">Shared</span>
            {:else if selectedResource.origin === 'marketplace'}
              <span class="detail-badge marketplace">Marketplace</span>
            {:else}
              <span class="detail-badge local">{selectedResource.profileName} only</span>
            {/if}
          </div>
          <div class="detail-actions">
            {#if selectedResource.origin === 'profile-local' && hasMultipleProfiles}
              <Button variant="secondary" disabled={operating} onclick={() => shareResource(selectedResource!)}>
                <Share2 size={13} /> Share
              </Button>
            {/if}
            {#if selectedResource.origin === 'weplex-managed'}
              <Button variant="danger" disabled={operating} onclick={() => (confirmDelete = selectedResource)}>
                <Trash2 size={13} /> Delete
              </Button>
            {/if}
          </div>
        </div>

        {#if selectedResource.isOutdated}
          <div class="drift-warning">
            <AlertTriangle size={14} />
            <span>This resource has been modified locally in a profile.</span>
          </div>
        {/if}

        {#if selectedResource.description}
          <p class="detail-description">{selectedResource.description}</p>
        {/if}

        <div class="detail-meta">
          <div class="meta-item">
            <span class="meta-label">Type</span>
            <span class="meta-value">{selectedResource.resourceType}</span>
          </div>
          <div class="meta-item">
            <span class="meta-label">Path</span>
            <span class="meta-value mono">{selectedResource.filePath}</span>
          </div>
          {#if selectedResource.marketplaceId}
            <div class="meta-item">
              <span class="meta-label">Package</span>
              <span class="meta-value">{selectedResource.marketplaceId}</span>
            </div>
          {/if}
        </div>

        <!-- For agents, show full detail if loaded -->
        {#if selectedAgentConfig && activeTab === 'agent'}
          <AgentDetail agent={selectedAgentConfig} />
        {/if}
      </div>

    {:else}
      <div class="resources-center-msg">
        Select a {tabs.find((t) => t.id === activeTab)?.label.slice(0, -1).toLowerCase()} to view details
      </div>
    {/if}
  </div>
</div>

{#if confirmDelete}
  <Modal onclose={() => (confirmDelete = null)} position="center" label="Confirm deletion" class="confirm-dialog">
    <p class="confirm-text">Delete <strong>{confirmDelete.name}</strong>?</p>
    <p class="confirm-hint">This will remove the resource from Weplex and all profiles.</p>
    <div class="confirm-actions">
      <Button variant="secondary" onclick={() => (confirmDelete = null)}>Cancel</Button>
      <Button variant="danger" disabled={operating} onclick={deleteResource}>Delete</Button>
    </div>
  </Modal>
{/if}

{#if toast}
  <div class="toast toast-{toast.type}">{toast.text}</div>
{/if}

<style>
  .resources-layout {
    display: flex;
    width: 100%;
    height: 100%;
    background: var(--weplex-bg);
    overflow: hidden;
  }

  /* Sidebar */
  .resources-sidebar {
    width: 240px;
    min-width: 240px;
    display: flex;
    flex-direction: column;
    border-right: 1px solid var(--weplex-border);
    background: var(--weplex-sidebar-bg);
    position: relative;
  }

  .resources-sidebar::before {
    content: '';
    position: absolute;
    inset: 0;
    background-image: radial-gradient(circle, rgba(255, 255, 255, 0.06) 0.5px, transparent 0.5px);
    background-size: 12px 12px;
    pointer-events: none;
  }

  /* Tabs */
  .resources-tabs {
    display: flex;
    padding: 12px 10px 0;
    gap: 2px;
    flex-shrink: 0;
    position: relative;
    z-index: 1;
  }

  .tab-btn {
    flex: 1;
    padding: 6px 0;
    border: none;
    background: transparent;
    color: var(--weplex-text-muted);
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
    border-bottom: 2px solid transparent;
    transition: all var(--weplex-duration-fast);
  }
  .tab-btn:hover { color: var(--weplex-text); }
  .tab-btn.active {
    color: var(--weplex-accent);
    border-bottom-color: var(--weplex-accent);
  }

  /* Nav */
  .resources-nav {
    flex: 1;
    overflow-y: auto;
    padding: 8px 0 0;
    position: relative;
    z-index: 1;
  }

  .resources-empty {
    padding: 16px;
    color: var(--weplex-text-muted);
    font-size: 12px;
    line-height: 1.5;
  }

  .nav-section-label {
    font-size: 9px;
    font-weight: 600;
    color: var(--weplex-text-muted);
    letter-spacing: 0.06em;
    text-transform: uppercase;
    padding: 10px 16px 4px;
    opacity: 0.6;
  }

  .resource-row {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 6px 14px 6px 16px;
    border: none;
    background: transparent;
    cursor: pointer;
    text-align: left;
    transition: background var(--weplex-duration-fast);
    position: relative;
  }
  .resource-row:hover { background: var(--weplex-surface-hover); }
  .resource-row.selected { background: color-mix(in srgb, var(--weplex-accent) 10%, transparent); }

  .row-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    border-radius: 6px;
    flex-shrink: 0;
    font-size: 11px;
    font-weight: 700;
    font-family: var(--weplex-font-mono);
    background: var(--weplex-surface-hover);
    color: var(--weplex-text-muted);
  }
  .row-icon.shared { background: color-mix(in srgb, var(--weplex-accent) 12%, transparent); color: var(--weplex-accent); }
  .row-icon.marketplace { background: color-mix(in srgb, var(--weplex-active) 12%, transparent); color: var(--weplex-active); }

  .row-name {
    flex: 1;
    font-size: 13px;
    font-weight: 500;
    color: var(--weplex-text);
    font-family: var(--weplex-font-mono);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .resource-row.selected .row-name { color: var(--weplex-accent); }

  .row-badge.drift {
    font-size: 10px;
    font-weight: 700;
    color: var(--weplex-warning, #f59e0b);
    flex-shrink: 0;
  }

  .row-action.share-btn {
    opacity: 0;
    border: none;
    background: transparent;
    color: var(--weplex-text-muted);
    cursor: pointer;
    padding: 2px;
    flex-shrink: 0;
    transition: all var(--weplex-duration-fast);
  }
  .resource-row:hover .share-btn { opacity: 0.6; }
  .share-btn:hover { opacity: 1 !important; color: var(--weplex-accent); }

  /* Footer */
  .resources-footer {
    padding: 8px;
    border-top: 1px solid var(--weplex-border);
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
    position: relative;
    z-index: 1;
  }

  .resources-action-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 7px 10px;
    border: 1px dashed var(--weplex-border);
    border-radius: var(--weplex-radius-md);
    background: transparent;
    color: var(--weplex-text-muted);
    font-size: 12px;
    cursor: pointer;
    transition: all var(--weplex-duration-fast);
  }
  .resources-action-btn:hover {
    border-color: var(--weplex-accent);
    color: var(--weplex-accent);
    border-style: solid;
  }

  /* Main content */
  .resources-main {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    overflow: hidden;
  }

  .resources-center-msg {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--weplex-text-muted);
    font-size: 13px;
  }

  /* Detail view */
  .detail {
    flex: 1;
    overflow-y: auto;
    padding: 24px 32px;
  }

  .detail-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 20px;
  }

  .detail-title-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .detail-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 36px;
    height: 36px;
    border-radius: 8px;
    font-size: 14px;
    font-weight: 700;
    font-family: var(--weplex-font-mono);
    background: var(--weplex-surface-hover);
    color: var(--weplex-text-muted);
  }
  .detail-icon.shared { background: color-mix(in srgb, var(--weplex-accent) 12%, transparent); color: var(--weplex-accent); }
  .detail-icon.marketplace { background: color-mix(in srgb, var(--weplex-active) 12%, transparent); color: var(--weplex-active); }

  .detail-header h2 {
    font-size: 16px;
    font-weight: 700;
    color: var(--weplex-text);
    font-family: var(--weplex-font-mono);
    margin: 0;
  }

  .detail-badge {
    font-size: 10px;
    font-weight: 600;
    padding: 2px 8px;
    border-radius: var(--weplex-radius-full, 999px);
  }
  .detail-badge.shared { background: color-mix(in srgb, var(--weplex-accent) 12%, transparent); color: var(--weplex-accent); }
  .detail-badge.marketplace { background: color-mix(in srgb, var(--weplex-active) 12%, transparent); color: var(--weplex-active); }
  .detail-badge.local { background: var(--weplex-surface-hover); color: var(--weplex-text-muted); }

  .detail-actions { display: flex; gap: 6px; }

  .drift-warning {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    margin-bottom: 16px;
    border-radius: 8px;
    background: color-mix(in srgb, var(--weplex-warning, #f59e0b) 8%, transparent);
    border: 1px solid color-mix(in srgb, var(--weplex-warning, #f59e0b) 20%, transparent);
    color: var(--weplex-warning, #f59e0b);
    font-size: 12px;
  }

  .detail-description {
    font-size: 13px;
    color: var(--weplex-text-secondary);
    margin: 0 0 20px;
    line-height: 1.5;
  }

  .detail-meta {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-bottom: 20px;
  }

  .meta-item {
    display: flex;
    gap: 12px;
    font-size: 12px;
  }

  .meta-label {
    width: 70px;
    flex-shrink: 0;
    color: var(--weplex-text-muted);
    font-weight: 500;
  }

  .meta-value {
    color: var(--weplex-text);
  }
  .meta-value.mono {
    font-family: var(--weplex-font-mono);
    font-size: 11px;
    opacity: 0.7;
  }

  /* Editor */
  .editor {
    flex: 1;
    overflow-y: auto;
    padding: 24px 32px;
    display: flex;
    flex-direction: column;
  }
  .editor-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 20px;
  }
  .editor-header h2 {
    font-size: 15px;
    font-weight: 700;
    color: var(--weplex-text);
    font-family: var(--weplex-font-mono);
    margin: 0;
  }
  .editor-actions { display: flex; gap: 6px; }
  .editor-error {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 16px;
    padding: 8px 12px;
    border-radius: 6px;
    background: color-mix(in srgb, var(--weplex-error) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--weplex-error) 20%, transparent);
    color: var(--weplex-error);
    font-size: 12px;
  }
  .editor-form { display: flex; flex-direction: column; gap: 14px; }
  .form-row { display: flex; flex-direction: column; gap: 5px; }
  .form-row label {
    display: flex;
    flex-direction: column;
    gap: 5px;
    font-size: 11px;
    font-weight: 600;
    color: var(--weplex-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .form-row input[type='text'], .form-row textarea {
    width: 100%;
    padding: 8px 10px;
    border: 1px solid var(--weplex-border);
    border-radius: 6px;
    background: var(--weplex-surface);
    color: var(--weplex-text);
    font-size: 13px;
    font-family: var(--weplex-font-mono);
    outline: none;
    transition: border-color var(--weplex-duration-fast);
  }
  .form-row input:focus, .form-row textarea:focus { border-color: var(--weplex-accent); }
  .form-row textarea { resize: vertical; line-height: 1.5; }

  /* Delete dialog */
  :global(.confirm-dialog) {
    width: 340px;
    padding: 20px;
    background: var(--weplex-surface);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-xl);
    box-shadow: var(--weplex-shadow-overlay);
  }
  .confirm-text { font-size: 14px; margin: 0 0 6px; }
  .confirm-hint { font-size: 12px; color: var(--weplex-text-muted); margin: 0 0 16px; }
  .confirm-actions { display: flex; gap: 8px; justify-content: flex-end; }

  /* Toast */
  .toast {
    position: fixed;
    bottom: 24px;
    left: 50%;
    transform: translateX(-50%);
    padding: 8px 20px;
    border-radius: var(--weplex-radius-full, 999px);
    font-size: 12px;
    font-weight: 500;
    z-index: 9999;
    animation: toast-in 0.2s ease-out;
    pointer-events: none;
  }
  .toast-success {
    background: var(--weplex-success, #10b981);
    color: white;
  }
  .toast-error {
    background: var(--weplex-error, #ef4444);
    color: white;
  }
  @keyframes toast-in {
    from { opacity: 0; transform: translateX(-50%) translateY(8px); }
    to { opacity: 1; transform: translateX(-50%) translateY(0); }
  }
</style>

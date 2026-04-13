<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { Button, Modal } from '../ui';
  import {
    resourceStore,
    type UnifiedResource,
    type ResourceType,
  } from '../../stores/resourceStore.svelte';
  import { profileStore } from '../../stores/profileStore.svelte';
  import { Plus, Copy, Trash2, AlertTriangle, RefreshCw, FileText, FolderOpen } from 'lucide-svelte';
  import { initial } from '../overlays/helpers';

  // ─── State ────────────────────────────────────────────────────────────

  let activeTab = $state<ResourceType>('agent');
  let selectedResource = $state<UnifiedResource | null>(null);
  let selectedProfileTab = $state<string | null>(null);

  // Editor
  let editMode = $state<'view' | 'new'>('view');
  let editName = $state('');
  let editContent = $state('');
  let editProfile = $state('');
  let editError = $state<string | null>(null);

  // Operations
  let operating = $state(false);
  let toast = $state<{ type: 'success' | 'error'; text: string } | null>(null);
  let confirmDelete = $state<{ name: string; filePath: string } | null>(null);
  let confirmOverwrite = $state<{
    resource: UnifiedResource;
    sourceProfileId: string;
    targetConfigDir: string;
    targetName: string;
  } | null>(null);

  // ─── Derived ──────────────────────────────────────────────────────────

  let hasMultipleProfiles = $derived(profileStore.profiles.length > 1);

  let tabResources = $derived(
    resourceStore.resources.filter((r) => r.resourceType === activeTab),
  );

  const tabs: { id: ResourceType; label: string; singular: string }[] = [
    { id: 'agent', label: 'Agents', singular: 'Agent' },
    { id: 'rule', label: 'Rules', singular: 'Rule' },
    { id: 'skill', label: 'Skills', singular: 'Skill' },
  ];

  let activeTabInfo = $derived(tabs.find((t) => t.id === activeTab)!);

  // ─── Lifecycle ────────────────────────────────────────────────────────

  onMount(() => {
    resourceStore.discover();
  });

  // ─── Actions ──────────────────────────────────────────────────────────

  function showToast(type: 'success' | 'error', text: string) {
    toast = { type, text };
    setTimeout(() => { toast = null; }, 3000);
  }

  function selectResource(r: UnifiedResource) {
    selectedResource = r;
    selectedProfileTab = r.profiles[0]?.profileId ?? null;
    editMode = 'view';
  }

  function startNew() {
    selectedResource = null;
    editMode = 'new';
    editName = '';
    editProfile = profileStore.profiles[0]?.configDir ?? '';
    editContent = activeTab === 'agent'
      ? `---\nname: \ndescription: \n---\n\nInstructions for this agent.\n`
      : `# New ${activeTabInfo.singular}\n\nContent here.\n`;
    editError = null;
  }

  async function saveNew() {
    if (!editName.trim()) { editError = 'Name is required'; return; }
    operating = true;
    try {
      await resourceStore.create(editProfile, activeTab, editName.trim(), editContent);
      editMode = 'view';
      editError = null;
      showToast('success', `Created ${editName.trim()}`);
    } catch (e: unknown) {
      editError = e instanceof Error ? e.message : String(e);
    } finally {
      operating = false;
    }
  }

  async function copyToProfile(resource: UnifiedResource, sourceProfile: string, targetConfigDir: string) {
    if (operating) return;
    const source = resource.profiles.find((p) => p.profileId === sourceProfile);
    if (!source) return;
    operating = true;
    try {
      const copied = await resourceStore.copyTo(
        source.filePath,
        targetConfigDir,
        resource.resourceType,
        resource.name,
        false,
      );
      if (copied) {
        showToast('success', `Copied ${resource.name}`);
      } else {
        showToast('success', `Already identical`);
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      if (msg.includes('different content')) {
        const targetProfile = profileStore.profiles.find((p) => (p.configDir ?? '') === targetConfigDir);
        confirmOverwrite = {
          resource,
          sourceProfileId: sourceProfile,
          targetConfigDir,
          targetName: targetProfile?.name ?? 'profile',
        };
      } else {
        showToast('error', `Copy failed: ${msg}`);
      }
    } finally {
      operating = false;
    }
  }

  async function overwriteConfirmed() {
    if (!confirmOverwrite || operating) return;
    const { resource, sourceProfileId, targetConfigDir } = confirmOverwrite;
    const source = resource.profiles.find((p) => p.profileId === sourceProfileId);
    if (!source) return;
    confirmOverwrite = null;
    operating = true;
    try {
      await resourceStore.copyTo(
        source.filePath,
        targetConfigDir,
        resource.resourceType,
        resource.name,
        true, // overwrite
      );
      showToast('success', `Overwrote ${resource.name}`);
    } catch (e) {
      showToast('error', `Overwrite failed: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      operating = false;
    }
  }

  async function deleteConfirmed() {
    if (!confirmDelete || operating) return;
    operating = true;
    try {
      await resourceStore.delete(confirmDelete.filePath);
      if (selectedResource?.name === confirmDelete.name) {
        selectedResource = null;
      }
      showToast('success', `Deleted ${confirmDelete.name}`);
    } catch (e) {
      showToast('error', `Delete failed: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      confirmDelete = null;
      operating = false;
    }
  }

  function profileBadgeText(r: UnifiedResource): string {
    const allProfileIds = new Set(profileStore.profiles.map((p) => p.id));
    const resourceProfileIds = new Set(r.profiles.map((p) => p.profileId));
    if (resourceProfileIds.size === allProfileIds.size) return 'All profiles';
    return r.profiles.map((p) => p.profileName).join(', ');
  }

  function missingProfiles(r: UnifiedResource) {
    const has = new Set(r.profiles.map((p) => p.profileId));
    return profileStore.profiles.filter((p) => !has.has(p.id));
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
          onclick={() => { activeTab = tab.id; selectedResource = null; editMode = 'view'; }}
        >
          {tab.label}
        </button>
      {/each}
    </div>

    <nav class="resources-nav">
      {#if resourceStore.loading}
        <div class="resources-empty">Loading...</div>
      {:else if tabResources.length === 0}
        <div class="resources-empty">
          No {activeTabInfo.label.toLowerCase()} yet.
        </div>
      {:else}
        {#each tabResources as r}
          <button
            class="resource-row"
            class:selected={selectedResource?.name === r.name && editMode === 'view'}
            onclick={() => selectResource(r)}
          >
            <span class="row-icon">{initial(r.name)}</span>
            <div class="row-content">
              <span class="row-name">{r.name}</span>
              {#if hasMultipleProfiles}
                <span class="row-badges">
                  <span class="badge" class:differs={r.differs}>
                    {profileBadgeText(r)}
                  </span>
                  {#if r.differs}
                    <span class="differs-icon" title="Content differs between profiles">⚠️</span>
                  {/if}
                </span>
              {/if}
            </div>
          </button>
        {/each}
      {/if}
    </nav>

    <div class="resources-footer">
      <button class="resources-action-btn" onclick={startNew}>
        <Plus size={13} />
        <span>New {activeTabInfo.singular}</span>
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
          <h2>New {activeTabInfo.singular}</h2>
          <div class="editor-actions">
            <Button variant="secondary" onclick={() => { editMode = 'view'; editError = null; }}>Cancel</Button>
            <Button variant="primary" disabled={operating} onclick={saveNew}>Create</Button>
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
          {#if hasMultipleProfiles}
            <div class="form-row">
              <label>Profile
                <select bind:value={editProfile}>
                  {#each profileStore.profiles as p}
                    <option value={p.configDir ?? ''}>{p.name}</option>
                  {/each}
                </select>
              </label>
            </div>
          {/if}
          <div class="form-row">
            <label>Content
              <textarea bind:value={editContent} rows={16} placeholder="---&#10;name: ...&#10;---"></textarea>
            </label>
          </div>
        </div>
      </div>

    {:else if selectedResource}
      <div class="detail">
        <div class="detail-header">
          <div class="detail-title-row">
            <span class="detail-icon">{initial(selectedResource.name)}</span>
            <h2>{selectedResource.name}</h2>
            {#if hasMultipleProfiles}
              <span class="detail-badge" class:differs={selectedResource.differs}>
                {profileBadgeText(selectedResource)}
              </span>
            {/if}
          </div>
          <div class="detail-actions">
            {#if hasMultipleProfiles && missingProfiles(selectedResource).length > 0}
              {#each missingProfiles(selectedResource) as target}
                <Button
                  variant="secondary"
                  disabled={operating}
                  onclick={() => copyToProfile(
                    selectedResource!,
                    selectedResource!.profiles[0].profileId,
                    target.configDir ?? '',
                  )}
                >
                  <Copy size={13} /> Copy to {target.name}
                </Button>
              {/each}
            {/if}
          </div>
        </div>

        {#if selectedResource.differs && selectedResource.profiles.length > 1}
          <div class="profile-tabs">
            {#each selectedResource.profiles as p}
              <button
                class="profile-tab"
                class:active={selectedProfileTab === p.profileId}
                onclick={() => (selectedProfileTab = p.profileId)}
              >
                {p.profileName}
              </button>
            {/each}
          </div>
        {/if}

        {#if selectedResource.description}
          <p class="detail-description">{selectedResource.description}</p>
        {/if}

        {#each [selectedResource.profiles.find((p) => p.profileId === selectedProfileTab) ?? selectedResource.profiles[0]] as activeProfile}
          {#if activeProfile}
            <div class="detail-meta">
              <span class="meta-path">{activeProfile.filePath}</span>
              <div class="meta-actions">
                <button
                  class="meta-btn"
                  title="Delete"
                  disabled={operating}
                  onclick={() => (confirmDelete = { name: selectedResource!.name, filePath: activeProfile.filePath })}
                >
                  <Trash2 size={13} />
                </button>
              </div>
            </div>
          {/if}
        {/each}
      </div>

    {:else}
      <div class="resources-center-msg">
        {#if tabResources.length === 0}
          <div class="empty-state">
            <FileText size={40} strokeWidth={1} />
            <h3>No {activeTabInfo.label.toLowerCase()} yet</h3>
            <p>Create one or browse the Marketplace.</p>
            <div class="empty-actions">
              <Button variant="secondary" onclick={startNew}>
                <Plus size={13} /> New {activeTabInfo.singular}
              </Button>
            </div>
          </div>
        {:else}
          Select a {activeTabInfo.singular.toLowerCase()} to view details
        {/if}
      </div>
    {/if}
  </div>
</div>

{#if confirmDelete}
  <Modal onclose={() => (confirmDelete = null)} position="center" label="Confirm deletion" class="confirm-dialog">
    <p class="confirm-text">Delete <strong>{confirmDelete.name}</strong>?</p>
    <p class="confirm-hint">{confirmDelete.filePath}</p>
    <div class="confirm-actions">
      <Button variant="secondary" onclick={() => (confirmDelete = null)}>Cancel</Button>
      <Button variant="danger" disabled={operating} onclick={deleteConfirmed}>Delete</Button>
    </div>
  </Modal>
{/if}

{#if confirmOverwrite}
  <Modal onclose={() => (confirmOverwrite = null)} position="center" label="Overwrite resource" class="confirm-dialog">
    <p class="confirm-text">Overwrite <strong>{confirmOverwrite.resource.name}</strong> in {confirmOverwrite.targetName}?</p>
    <p class="confirm-hint">This profile already has a different version of this resource. Copying will replace it.</p>
    <div class="confirm-actions">
      <Button variant="secondary" onclick={() => (confirmOverwrite = null)}>Cancel</Button>
      <Button variant="primary" disabled={operating} onclick={overwriteConfirmed}>Overwrite</Button>
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
    width: 260px;
    min-width: 260px;
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
  .tab-btn.active { color: var(--weplex-accent); border-bottom-color: var(--weplex-accent); }

  .resources-nav {
    flex: 1;
    overflow-y: auto;
    padding: 8px 0;
    position: relative;
    z-index: 1;
  }
  .resources-empty {
    padding: 16px;
    color: var(--weplex-text-muted);
    font-size: 12px;
    line-height: 1.5;
  }

  .resource-row {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 6px 14px;
    border: none;
    background: transparent;
    cursor: pointer;
    text-align: left;
    transition: background var(--weplex-duration-fast);
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

  .row-content {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .row-name {
    font-size: 13px;
    font-weight: 500;
    color: var(--weplex-text);
    font-family: var(--weplex-font-mono);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .resource-row.selected .row-name { color: var(--weplex-accent); }

  .row-badges {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .badge {
    font-size: 10px;
    color: var(--weplex-text-muted);
    white-space: nowrap;
  }
  .differs-icon {
    font-size: 11px;
    line-height: 1;
  }

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

  /* Main */
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

  /* Empty state */
  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    text-align: center;
    color: var(--weplex-text-muted);
  }
  .empty-state h3 {
    font-size: 16px;
    font-weight: 600;
    color: var(--weplex-text);
    margin: 0;
  }
  .empty-state p {
    font-size: 13px;
    margin: 0;
  }
  .empty-actions { margin-top: 8px; }

  /* Detail */
  .detail {
    flex: 1;
    overflow-y: auto;
    padding: 24px 32px;
  }
  .detail-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    margin-bottom: 16px;
    gap: 12px;
  }
  .detail-title-row {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
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
    background: var(--weplex-surface-hover);
    color: var(--weplex-text-muted);
  }
  .detail-badge.differs {
    background: color-mix(in srgb, var(--weplex-warning, #f59e0b) 12%, transparent);
    color: var(--weplex-warning, #f59e0b);
  }
  .detail-actions {
    display: flex;
    gap: 6px;
    flex-shrink: 0;
    flex-wrap: wrap;
  }

  /* Profile tabs (for differs) */
  .profile-tabs {
    display: flex;
    gap: 2px;
    margin-bottom: 16px;
    border-bottom: 1px solid var(--weplex-border);
  }
  .profile-tab {
    padding: 6px 12px;
    border: none;
    background: transparent;
    color: var(--weplex-text-muted);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    border-bottom: 2px solid transparent;
    transition: all var(--weplex-duration-fast);
  }
  .profile-tab:hover { color: var(--weplex-text); }
  .profile-tab.active { color: var(--weplex-accent); border-bottom-color: var(--weplex-accent); }

  .detail-description {
    font-size: 13px;
    color: var(--weplex-text-secondary);
    margin: 0 0 16px;
    line-height: 1.5;
  }
  .detail-meta {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 12px;
    background: var(--weplex-surface);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-md);
  }
  .meta-path {
    font-family: var(--weplex-font-mono);
    font-size: 11px;
    color: var(--weplex-text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta-actions { display: flex; gap: 4px; }
  .meta-btn {
    border: none;
    background: transparent;
    color: var(--weplex-text-muted);
    padding: 4px;
    cursor: pointer;
    border-radius: var(--weplex-radius-sm);
    transition: all var(--weplex-duration-fast);
  }
  .meta-btn:hover { color: var(--weplex-error); background: color-mix(in srgb, var(--weplex-error) 10%, transparent); }
  .meta-btn:disabled { opacity: 0.4; cursor: not-allowed; }

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
  .form-row input[type='text'],
  .form-row textarea,
  .form-row select {
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
  .form-row input:focus,
  .form-row textarea:focus,
  .form-row select:focus { border-color: var(--weplex-accent); }
  .form-row textarea { resize: vertical; line-height: 1.5; }

  /* Confirm dialog */
  :global(.confirm-dialog) {
    width: 380px;
    padding: 20px;
    background: var(--weplex-surface);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-xl);
    box-shadow: var(--weplex-shadow-overlay);
  }
  .confirm-text { font-size: 14px; margin: 0 0 6px; }
  .confirm-hint {
    font-size: 11px;
    color: var(--weplex-text-muted);
    font-family: var(--weplex-font-mono);
    margin: 0 0 16px;
    word-break: break-all;
  }
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
  .toast-success { background: var(--weplex-active, #10b981); color: white; }
  .toast-error { background: var(--weplex-error, #ef4444); color: white; }
  @keyframes toast-in {
    from { opacity: 0; transform: translateX(-50%) translateY(8px); }
    to { opacity: 1; transform: translateX(-50%) translateY(0); }
  }
</style>

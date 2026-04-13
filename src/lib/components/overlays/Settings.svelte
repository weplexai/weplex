<script lang="ts">
  import { Select, Input, Tabs } from '../ui';
  import { invoke } from '@tauri-apps/api/core';
  import { getVersion } from '@tauri-apps/api/app';
  import { settingsStore } from '../../stores/settingsStore';
  import { profileStore } from '../../stores/profileStore';
  import { resourceStore } from '../../stores/resourceStore.svelte';
  import { uiStore } from '../../stores/uiStore';
  import TeamSettings from './TeamSettings.svelte';
  import ImportProfileDialog from './ImportProfileDialog.svelte';
  import {
    checkForUpdates,
    updateState,
    installUpdate,
    restartToUpdate,
  } from '../../utils/updater';
  import type { DiscoveredProfile } from '../../types';

  let appVersion = $state('');
  getVersion().then((v) => (appVersion = v));

  let updateChecking = $state(false);
  async function handleCheckUpdates() {
    updateChecking = true;
    await checkForUpdates();
    updateChecking = false;
  }

  let settings = $derived(settingsStore.settings);
  let activeTab = $state('appearance');

  // Profile discovery state
  let discoveredProfiles = $state<DiscoveredProfile[]>([]);
  let discoveryLoading = $state(false);
  let discoveryDone = $state(false);

  $effect(() => {
    if (activeTab === 'profiles' && !discoveryDone) {
      runDiscovery();
    }
  });

  async function runDiscovery() {
    discoveryLoading = true;
    try {
      const all = await invoke<DiscoveredProfile[]>('discover_profiles');
      discoveredProfiles = all.filter((d) => !profileStore.hasConfigDir(d.path));
    } catch {
      discoveredProfiles = [];
    } finally {
      discoveryLoading = false;
      discoveryDone = true;
    }
  }

  // Import profile dialog state
  let importTarget = $state<DiscoveredProfile | null>(null);
  let importCounts = $derived({
    agents: resourceStore.shared.filter((r) => r.resourceType === 'agent').length,
    rules: resourceStore.shared.filter((r) => r.resourceType === 'rule').length,
    skills: resourceStore.shared.filter((r) => r.resourceType === 'skill').length,
  });

  function startImport(dp: DiscoveredProfile) {
    importTarget = dp;
  }

  async function confirmImport(sync: boolean) {
    if (!importTarget) return;
    const dp = importTarget;
    importTarget = null;

    // 1. Always promote unique resources from this profile to ~/.weplex/
    await invoke('promote_profile_resources', { configDir: dp.path }).catch((e) =>
      console.warn('[weplex] promote failed:', e),
    );

    // 2. Create profile (sync=user's choice: copy shared resources TO this profile)
    profileStore.create(dp.name, dp.path, sync);
    discoveredProfiles = discoveredProfiles.filter((d) => d.path !== dp.path);

    // 3. Refresh resource view
    resourceStore.discover();
  }

  // Profile editing state
  let editingProfileId = $state<string | null>(null);
  let profileName = $state('');
  let profileConfigDir = $state('');
  let profileEnvKey = $state('');
  let profileEnvVal = $state('');

  const tabs = [
    { id: 'general', label: 'General' },
    { id: 'appearance', label: 'Appearance' },
    { id: 'profiles', label: 'Profiles' },
    { id: 'team', label: 'Team' },
    { id: 'sessions', label: 'Sessions' },
    { id: 'about', label: 'About' },
  ];

  function startEditProfile(id: string) {
    const p = profileStore.getById(id);
    if (!p) return;
    editingProfileId = id;
    profileName = p.name;
    profileConfigDir = p.configDir || '';
    profileEnvKey = '';
    profileEnvVal = '';
  }

  function cancelEditProfile() {
    editingProfileId = null;
  }

  function saveProfile() {
    if (!editingProfileId || !profileName.trim()) return;
    profileStore.update(editingProfileId, {
      name: profileName.trim(),
      configDir: profileConfigDir.trim() || null,
    });
    editingProfileId = null;
  }

  function createProfile() {
    const p = profileStore.create('New Profile');
    startEditProfile(p.id);
  }

  function addEnvVar(profileId: string) {
    if (!profileEnvKey.trim()) return;
    const p = profileStore.getById(profileId);
    if (!p) return;
    profileStore.update(profileId, {
      envVars: { ...p.envVars, [profileEnvKey.trim()]: profileEnvVal },
    });
    profileEnvKey = '';
    profileEnvVal = '';
  }

  function removeEnvVar(profileId: string, key: string) {
    const p = profileStore.getById(profileId);
    if (!p) return;
    const { [key]: _, ...rest } = p.envVars;
    profileStore.update(profileId, { envVars: rest });
  }

</script>

<div class="settings-inner">
  <div class="settings-sidebar">
    <h2 class="settings-title">Settings</h2>
    <Tabs tabs={tabs} active={activeTab} onchange={(id) => (activeTab = id)} orientation="vertical" />
  </div>
  <div class="settings-content">
      {#if activeTab === 'general'}
        <h3 class="section-title">General</h3>
        <div class="setting">
          <label class="setting-label" for="set-dir">Default directory</label>
          <Input
            id="set-dir"
            class="setting-input"
            type="text"
            value={settings.defaultDirectory}
            onchange={(e) =>
              settingsStore.update({ defaultDirectory: (e.target as HTMLInputElement).value })}
          />
        </div>
      {:else if activeTab === 'appearance'}
        <h3 class="section-title">Appearance</h3>
        <div class="setting">
          <label class="setting-label" for="set-theme">Theme</label>
          <Select
            id="set-theme"
            value={settings.theme}
            options={[
              { value: 'dark', label: 'Dark' },
              { value: 'light', label: 'Light' },
            ]}
            onchange={(v) => settingsStore.update({ theme: v as 'dark' | 'light' })}
          />
        </div>

        <h4 class="subsection-title">Terminal</h4>
        <div class="setting">
          <label class="setting-label" for="set-font">Font</label>
          <Input
            id="set-font"
            class="setting-input"
            type="text"
            mono
            value={settings.fontFamily}
            onchange={(e) =>
              settingsStore.update({ fontFamily: (e.target as HTMLInputElement).value })}
          />
        </div>
        <div class="setting">
          <span class="setting-label">Font size</span>
          <div class="size-control" role="group" aria-label="Font size">
            <button
              class="size-btn"
              onclick={() =>
                settingsStore.update({ fontSize: Math.max(10, settings.fontSize - 1) })}>−</button
            >
            <span class="size-value">{settings.fontSize}px</span>
            <button
              class="size-btn"
              onclick={() =>
                settingsStore.update({ fontSize: Math.min(24, settings.fontSize + 1) })}>+</button
            >
          </div>
        </div>
      {:else if activeTab === 'profiles'}
        <h3 class="section-title">Profiles</h3>
        <p class="section-desc">
          Profiles let you use different agent accounts (personal, work) in different Spaces.
        </p>

        {#if discoveryLoading}
          <p class="discovery-status">Scanning for profiles...</p>
        {/if}

        {#if discoveredProfiles.length > 0}
          <div class="discovery-section">
            <span class="discovery-label">Discovered on this system</span>
            {#each discoveredProfiles as dp (dp.path)}
              <div class="profile-card discovered">
                <div class="profile-row">
                  <div class="profile-info">
                    <span class="profile-name">{dp.name}</span>
                    <span class="profile-meta">
                      {dp.path}
                      <span class="source-badge"
                        >{dp.source === 'shell_config' ? 'from .zshrc' : 'filesystem'}</span
                      >
                    </span>
                  </div>
                  <button class="btn-sm import" onclick={() => startImport(dp)}>Import</button>
                </div>
              </div>
            {/each}
          </div>
        {/if}

        {#each profileStore.profiles as profile (profile.id)}
          <div class="profile-card">
            {#if editingProfileId === profile.id}
              <div class="profile-edit">
                <label class="setting-label" for="pf-name">Name</label>
                <Input id="pf-name" class="setting-input" type="text" bind:value={profileName} />

                <label class="setting-label" for="pf-dir">Config directory</label>
                <Input
                  id="pf-dir"
                  class="setting-input"
                  type="text"
                  mono
                  bind:value={profileConfigDir}
                  placeholder="Leave empty for system default"
                />

                <div class="env-section">
                  <span class="setting-label">Environment variables</span>
                  {#each Object.entries(profile.envVars) as [key, value]}
                    <div class="env-row">
                      <span class="env-key">{key}</span>
                      <span class="env-val">{value}</span>
                      <button class="env-remove" onclick={() => removeEnvVar(profile.id, key)}
                        >x</button
                      >
                    </div>
                  {/each}
                  <div class="env-add">
                    <input
                      class="env-input"
                      type="text"
                      placeholder="KEY"
                      bind:value={profileEnvKey}
                    />
                    <input
                      class="env-input"
                      type="text"
                      placeholder="value"
                      bind:value={profileEnvVal}
                    />
                    <button class="env-add-btn" onclick={() => addEnvVar(profile.id)}>+</button>
                  </div>
                </div>

                <div class="profile-edit-actions">
                  <button class="btn-sm cancel" onclick={cancelEditProfile}>Cancel</button>
                  <button class="btn-sm save" onclick={saveProfile}>Save</button>
                </div>
              </div>
            {:else}
              <div class="profile-row">
                <div class="profile-info">
                  <span class="profile-name">
                    {profile.name}
                    {#if profile.isDefault}<span class="badge">default</span>{/if}
                  </span>
                  <span class="profile-meta">
                    {profile.configDir || 'System default'}
                    {#if Object.keys(profile.envVars).length > 0}
                      &middot; {Object.keys(profile.envVars).length} env vars
                    {/if}
                  </span>
                </div>
                <div class="profile-actions">
                  <button class="btn-sm" onclick={() => startEditProfile(profile.id)}>Edit</button>
                  {#if !profile.isDefault}
                    <button class="btn-sm delete" onclick={() => profileStore.remove(profile.id)}
                      >Delete</button
                    >
                  {/if}
                </div>
              </div>
            {/if}
          </div>
        {/each}

        <button class="btn-add-profile" onclick={createProfile}>+ New Profile</button>
      {:else if activeTab === 'team'}
        <TeamSettings />
      {:else if activeTab === 'sessions'}
        <h3 class="section-title">Sessions</h3>
        <div class="setting">
          <label class="setting-label" for="set-persist">Persist sessions across restarts</label>
          <input
            id="set-persist"
            type="checkbox"
            checked={settings.persistSessions}
            onchange={(e) =>
              settingsStore.update({ persistSessions: (e.target as HTMLInputElement).checked })}
          />
        </div>
      {:else if activeTab === 'about'}
        <h3 class="section-title">About</h3>
        <p class="about-text"><strong>Weplex</strong> v{appVersion}</p>
        <p class="about-text muted">
          The terminal for AI coding agents.
        </p>
        <p class="about-text muted">Apache 2.0 License</p>

        <div class="about-divider"></div>
        <h3 class="section-title">Updates</h3>
        {#if updateState.readyToRestart}
          <p class="about-text">Update downloaded. Restart to apply.</p>
          <button class="btn-sm save" onclick={restartToUpdate}>Restart Now</button>
        {:else if updateState.downloading}
          <p class="about-text">Downloading update... {updateState.progress}%</p>
        {:else if updateState.available}
          <p class="about-text">Update available: <strong>v{updateState.version}</strong></p>
          <button class="btn-sm save" onclick={installUpdate}>Download Update</button>
        {:else}
          <p class="about-text muted">You're up to date.</p>
        {/if}
        <button
          class="btn-sm"
          onclick={handleCheckUpdates}
          disabled={updateChecking || updateState.downloading}
          style="margin-top: 8px;"
        >
          {updateChecking ? 'Checking...' : 'Check for Updates'}
        </button>

        <div class="about-divider"></div>
        <footer class="about-footer">
          <a class="about-footer-link" href="https://weplex.ai" target="_blank" rel="noopener">weplex.ai</a>
          <span class="about-footer-copy">&copy; 2026</span>
        </footer>
      {/if}
  </div>
</div>

{#if importTarget}
  <ImportProfileDialog
    profileName={importTarget.name}
    counts={importCounts}
    onconfirm={confirmImport}
    onclose={() => (importTarget = null)}
  />
{/if}

<style>
  .settings-inner {
    display: flex;
    width: 100%;
    height: 100%;
    background: var(--weplex-bg);
    overflow: hidden;
  }

  .settings-sidebar {
    width: 200px;
    position: relative;
    background: var(--weplex-sidebar-bg);
    border-right: 1px solid var(--weplex-border);
    padding: 16px 8px;
  }

  .settings-sidebar::before {
    content: '';
    position: absolute;
    inset: 0;
    background-image: radial-gradient(circle, rgba(255, 255, 255, 0.06) 0.5px, transparent 0.5px);
    background-size: 12px 12px;
    pointer-events: none;
  }

  .settings-title {
    font-size: var(--weplex-text-md);
    font-weight: 600;
    padding: 0 8px 12px;
  }

  .settings-content {
    flex: 1;
    padding: 20px;
    overflow-y: auto;
  }

  .section-title {
    font-size: var(--weplex-text-lg);
    font-weight: 600;
    margin-bottom: 16px;
  }

  .setting {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 16px;
    padding: 10px 0;
    border-bottom: 1px solid var(--weplex-border);
  }

  .subsection-title {
    font-size: var(--weplex-text-xs);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--weplex-text-muted);
    margin: 16px 0 4px;
  }

  .setting-label {
    font-size: var(--weplex-text-sm);
    color: var(--weplex-text-secondary);
    white-space: nowrap;
  }

  :global(.setting-input) {
    width: 200px;
  }

  .size-control {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .size-btn {
    width: 26px;
    height: 26px;
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-sm);
    background: transparent;
    color: var(--weplex-text);
    font-size: var(--weplex-text-md);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .size-btn:hover {
    background: var(--weplex-surface-hover);
  }

  .size-value {
    font-family: var(--weplex-font-mono);
    font-size: var(--weplex-text-sm);
    min-width: 36px;
    text-align: center;
  }

  .about-text {
    font-size: var(--weplex-text-base);
    margin-bottom: 6px;
  }

  .about-text.muted {
    color: var(--weplex-text-muted);
    font-size: var(--weplex-text-sm);
  }

  .about-divider {
    height: 1px;
    background: var(--weplex-border);
    margin: 10px 0;
  }

  /* ═══ About footer ═══ */
  .about-footer {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 8px 0 0;
  }

  .about-footer-link {
    font-size: 12px;
    color: var(--weplex-text-muted);
    text-decoration: none;
  }

  .about-footer-link:hover {
    color: var(--weplex-accent);
  }

  .about-footer-copy {
    font-size: 12px;
    color: var(--weplex-text-muted);
    opacity: 0.5;
  }

  /* Profiles */
  .section-desc {
    font-size: var(--weplex-text-sm);
    color: var(--weplex-text-muted);
    margin-bottom: 16px;
    line-height: 1.4;
  }

  .profile-card {
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-lg);
    padding: 12px;
    margin-bottom: 8px;
  }

  .profile-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .profile-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .profile-name {
    font-size: var(--weplex-text-sm);
    font-weight: 500;
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .badge {
    font-size: var(--weplex-text-xs);
    padding: 1px 6px;
    border-radius: var(--weplex-radius-full);
    background: color-mix(in srgb, var(--weplex-accent) 15%, transparent);
    color: var(--weplex-accent);
    font-weight: 400;
  }

  .profile-meta {
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
    font-family: var(--weplex-font-mono);
  }

  .profile-actions {
    display: flex;
    gap: 4px;
  }

  .profile-edit {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .profile-edit .setting-label {
    margin-top: 8px;
  }

  .profile-edit-actions {
    display: flex;
    gap: 6px;
    justify-content: flex-end;
    margin-top: 10px;
  }

  .btn-sm {
    padding: 4px 10px;
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-sm);
    background: transparent;
    color: var(--weplex-text-secondary);
    font-size: var(--weplex-text-xs);
    cursor: pointer;
  }

  .btn-sm:hover {
    background: var(--weplex-surface-hover);
  }

  .btn-sm.save {
    background: var(--weplex-accent);
    border-color: var(--weplex-accent);
    color: white;
  }

  .btn-sm.save:hover {
    opacity: 0.9;
  }

  .btn-sm.delete {
    border-color: var(--weplex-error);
    color: var(--weplex-error);
  }

  .btn-sm.delete:hover {
    background: rgba(239, 68, 68, 0.1);
  }

  .env-section {
    margin-top: 8px;
  }

  .env-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 0;
    font-size: var(--weplex-text-xs);
    font-family: var(--weplex-font-mono);
  }

  .env-key {
    color: var(--weplex-accent);
    min-width: 80px;
  }

  .env-val {
    color: var(--weplex-text-secondary);
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .env-remove {
    width: 18px;
    height: 18px;
    border: none;
    border-radius: var(--weplex-radius-sm);
    background: transparent;
    color: var(--weplex-text-muted);
    font-size: var(--weplex-text-xs);
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .env-remove:hover {
    background: rgba(239, 68, 68, 0.1);
    color: var(--weplex-error);
  }

  .env-add {
    display: flex;
    gap: 4px;
    margin-top: 4px;
  }

  .env-input {
    flex: 1;
    padding: 4px 6px;
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-sm);
    background: var(--weplex-bg);
    color: var(--weplex-text);
    font-size: var(--weplex-text-xs);
    font-family: var(--weplex-font-mono);
    outline: none;
  }

  .env-input:focus {
    border-color: var(--weplex-accent);
  }

  .env-add-btn {
    width: 26px;
    height: 26px;
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-sm);
    background: transparent;
    color: var(--weplex-text-muted);
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .env-add-btn:hover {
    border-color: var(--weplex-accent);
    color: var(--weplex-accent);
  }

  .btn-add-profile {
    width: 100%;
    padding: 8px;
    border: 1px dashed var(--weplex-border);
    border-radius: var(--weplex-radius-md);
    background: transparent;
    color: var(--weplex-text-muted);
    font-size: var(--weplex-text-sm);
    cursor: pointer;
    transition: all var(--weplex-duration-fast) var(--weplex-easing);
  }

  .btn-add-profile:hover {
    border-color: var(--weplex-accent);
    color: var(--weplex-accent);
  }

  /* Discovery */
  .discovery-section {
    margin-bottom: 16px;
  }

  .discovery-label {
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    display: block;
    margin-bottom: 8px;
  }

  .discovery-status {
    font-size: var(--weplex-text-sm);
    color: var(--weplex-text-muted);
    margin-bottom: 12px;
  }

  .profile-card.discovered {
    border-style: dashed;
    border-color: var(--weplex-accent);
    background: color-mix(in srgb, var(--weplex-accent) 4%, transparent);
  }

  .source-badge {
    margin-left: 6px;
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
    font-style: italic;
  }

  .btn-sm.import {
    border-color: var(--weplex-accent);
    color: var(--weplex-accent);
  }

  .btn-sm.import:hover {
    background: color-mix(in srgb, var(--weplex-accent) 10%, transparent);
  }
</style>

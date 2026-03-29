<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { settingsStore } from '../../stores/settingsStore';
  import { profileStore } from '../../stores/profileStore';
  import { authStore } from '../../stores/authStore.svelte';
  import { uiStore } from '../../stores/uiStore';
  import { checkForUpdates, updateState, installUpdate } from '../../utils/updater';

  let updateChecking = $state(false);
  async function handleCheckUpdates() {
    updateChecking = true;
    await checkForUpdates();
    updateChecking = false;
  }
  import type { DiscoveredProfile } from '../../types';

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

  function importProfile(dp: DiscoveredProfile) {
    profileStore.create(dp.name, dp.path);
    discoveredProfiles = discoveredProfiles.filter((d) => d.path !== dp.path);
  }

  // Profile editing state
  let editingProfileId = $state<string | null>(null);
  let profileName = $state('');
  let profileConfigDir = $state('');
  let profileEnvKey = $state('');
  let profileEnvVal = $state('');

  // Account tab state
  let authEmail = $state('');
  let authPassword = $state('');
  let authMode = $state<'login' | 'register'>('login');
  let editDisplayName = $state('');
  let editingName = $state(false);

  const tabs = [
    { id: 'general', label: 'General' },
    { id: 'appearance', label: 'Appearance' },
    { id: 'profiles', label: 'Profiles' },
    { id: 'sessions', label: 'Sessions' },
    { id: 'account', label: 'Account' },
    { id: 'about', label: 'About' },
  ];

  // Client-side validation helpers
  const EMAIL_RE = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

  function validateAuthFields(): string | null {
    if (!EMAIL_RE.test(authEmail)) return 'Please enter a valid email address';
    if (authMode === 'register' && authPassword.length < 8)
      return 'Password must be at least 8 characters';
    return null;
  }

  let authValidationError = $state<string | null>(null);

  async function handleAuthSubmit() {
    authValidationError = validateAuthFields();
    if (authValidationError) return;
    try {
      if (authMode === 'login') {
        await authStore.login(authEmail, authPassword);
      } else {
        await authStore.register(authEmail, authPassword);
      }
      authEmail = '';
      authPassword = '';
    } catch {
      // Error is set in authStore
    }
  }

  async function handleOAuth(provider: 'github' | 'google') {
    try {
      await authStore.oauthLogin(provider);
    } catch {
      // Error is set in authStore
    }
  }

  function startEditDisplayName() {
    editDisplayName = authStore.user?.displayName || '';
    editingName = true;
  }

  async function saveDisplayName() {
    try {
      await authStore.updateProfile({ displayName: editDisplayName.trim() || undefined });
      editingName = false;
    } catch {
      // Error is set in authStore
    }
  }

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

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') uiStore.closeOverlay();
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions a11y_no_noninteractive_element_interactions -->
<div
  class="overlay-backdrop"
  role="presentation"
  onclick={() => uiStore.closeOverlay()}
  onkeydown={handleKeydown}
>
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions a11y_no_noninteractive_element_interactions a11y_interactive_supports_focus -->
  <div
    class="settings"
    role="dialog"
    tabindex="-1"
    aria-label="Settings"
    onclick={(e) => e.stopPropagation()}
  >
    <div class="settings-sidebar">
      <h2 class="settings-title">Settings</h2>
      {#each tabs as tab (tab.id)}
        <button
          class="tab-btn"
          class:active={activeTab === tab.id}
          data-tab={tab.id}
          onclick={() => (activeTab = tab.id)}
        >
          {tab.label}
        </button>
      {/each}
    </div>

    <div class="settings-content">
      {#if activeTab === 'general'}
        <h3 class="section-title">General</h3>
        <div class="setting">
          <label class="setting-label" for="set-dir">Default directory</label>
          <input
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
          <select
            id="set-theme"
            class="setting-select"
            value={settings.theme}
            onchange={(e) =>
              settingsStore.update({
                theme: (e.target as HTMLSelectElement).value as 'dark' | 'light',
              })}
          >
            <option value="dark">Dark</option>
            <option value="light">Light</option>
          </select>
        </div>
        <div class="setting">
          <label class="setting-label" for="set-font">Font family</label>
          <input
            id="set-font"
            class="setting-input mono"
            type="text"
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
                  <button class="btn-sm import" onclick={() => importProfile(dp)}>Import</button>
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
                <input id="pf-name" class="setting-input" type="text" bind:value={profileName} />

                <label class="setting-label" for="pf-dir">Config directory</label>
                <input
                  id="pf-dir"
                  class="setting-input mono"
                  type="text"
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
      {:else if activeTab === 'account'}
        <h3 class="section-title">Account</h3>

        {#if authStore.isAuthenticated}
          <!-- Logged in -->
          <div class="account-card">
            <div class="account-row">
              <div class="account-info">
                {#if editingName}
                  <div class="inline-edit">
                    <input
                      class="setting-input"
                      type="text"
                      bind:value={editDisplayName}
                      placeholder="Display name"
                      onkeydown={(e) => e.key === 'Enter' && saveDisplayName()}
                    />
                    <button class="btn-sm save" onclick={saveDisplayName}>Save</button>
                    <button class="btn-sm cancel" onclick={() => (editingName = false)}
                      >Cancel</button
                    >
                  </div>
                {:else}
                  <span class="account-name" onclick={startEditDisplayName}>
                    {authStore.user?.displayName || authStore.user?.email || 'User'}
                  </span>
                {/if}
                <span class="profile-meta">{authStore.user?.email}</span>
              </div>
              <span class="badge">{authStore.user?.plan || 'Free'}</span>
            </div>
          </div>

          <div class="setting">
            <span class="setting-label">Sync status</span>
            <span class="sync-status" class:sync-error={authStore.syncStatus === 'error'}>
              {authStore.syncStatus}
            </span>
          </div>

          <div class="account-actions">
            <button class="btn-sm delete" onclick={() => authStore.logout()}>Sign Out</button>
          </div>
        {:else}
          <!-- Logged out -->
          {#if authValidationError}
            <div class="auth-error">{authValidationError}</div>
          {/if}
          {#if authStore.error}
            <div class="auth-error">{authStore.error}</div>
          {/if}

          <div class="auth-form">
            <input
              class="setting-input auth-input"
              type="email"
              placeholder="Email"
              bind:value={authEmail}
              onkeydown={(e) => e.key === 'Enter' && handleAuthSubmit()}
            />
            <input
              class="setting-input auth-input"
              type="password"
              placeholder="Password"
              bind:value={authPassword}
              onkeydown={(e) => e.key === 'Enter' && handleAuthSubmit()}
            />
            <div class="auth-buttons">
              <button
                class="btn-sm save"
                disabled={authStore.loading || !authEmail || !authPassword}
                onclick={handleAuthSubmit}
              >
                {authStore.loading ? '...' : authMode === 'login' ? 'Sign In' : 'Register'}
              </button>
              <button
                class="btn-sm"
                onclick={() => {
                  authMode = authMode === 'login' ? 'register' : 'login';
                  authValidationError = null;
                  authStore.clearError();
                }}
              >
                {authMode === 'login' ? 'Create account' : 'I have an account'}
              </button>
            </div>
          </div>

          <div class="oauth-divider">
            <span class="oauth-divider-text">or</span>
          </div>

          <div class="oauth-buttons">
            <button
              class="btn-sm oauth-btn"
              disabled={authStore.loading}
              onclick={() => handleOAuth('github')}
            >
              GitHub
            </button>
            <button
              class="btn-sm oauth-btn"
              disabled={authStore.loading}
              onclick={() => handleOAuth('google')}
            >
              Google
            </button>
          </div>

          <p class="section-desc" style="margin-top: 16px;">
            Sign in to sync settings across devices. Weplex works fully without an account.
          </p>
        {/if}
      {:else if activeTab === 'about'}
        <h3 class="section-title">About</h3>
        <p class="about-text"><strong>Weplex</strong> v0.2.0</p>
        <p class="about-text muted">
          The terminal with a built-in pipeline engine for AI coding agents.
        </p>
        <p class="about-text muted">Apache 2.0 License</p>

        <div class="about-divider"></div>
        <h3 class="section-title">Updates</h3>
        {#if updateState.downloading}
          <p class="about-text">Downloading update... {updateState.progress}%</p>
        {:else if updateState.available}
          <p class="about-text">Update available: <strong>v{updateState.version}</strong></p>
          <button class="btn-sm save" onclick={installUpdate}>Install Update</button>
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
        <footer class="shipooor-footer">
          <a class="shipooor-link" href="https://shipooor.xyz" target="_blank" rel="noopener">
            <span class="shipooor-stamp-outer">
              <span class="shipooor-stamp-inner">SHIPPED</span>
            </span>
            <span class="shipooor-ft">by <strong>shipooor</strong> · </span>
          </a>
          <a class="shipooor-x" href="https://x.com/shipooor" target="_blank" rel="noopener">X</a>
        </footer>
      {/if}
    </div>
  </div>
</div>

<style>
  .overlay-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    justify-content: center;
    align-items: center;
    z-index: 100;
  }

  .settings {
    width: 600px;
    height: 420px;
    background: var(--weplex-surface);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-xl);
    box-shadow: var(--weplex-shadow-overlay);
    display: flex;
    overflow: hidden;
  }

  .settings-sidebar {
    width: 160px;
    background: var(--weplex-sidebar-bg);
    border-right: 1px solid var(--weplex-border);
    padding: 16px 8px;
  }

  .settings-title {
    font-size: var(--weplex-text-md);
    font-weight: 600;
    padding: 0 8px 12px;
  }

  .tab-btn {
    display: block;
    width: 100%;
    padding: 7px 8px;
    border: none;
    border-radius: var(--weplex-radius-md);
    background: transparent;
    color: var(--weplex-text-secondary);
    font-size: var(--weplex-text-sm);
    text-align: left;
    transition: all var(--weplex-duration-fast) var(--weplex-easing);
  }

  .tab-btn:hover {
    background: var(--weplex-surface);
  }

  .tab-btn.active {
    background: var(--weplex-surface-hover);
    color: var(--weplex-text);
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
    padding: 10px 0;
    border-bottom: 1px solid var(--weplex-border);
  }

  .setting-label {
    font-size: var(--weplex-text-sm);
    color: var(--weplex-text-secondary);
  }

  .setting-input {
    padding: 5px 8px;
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-md);
    background: var(--weplex-bg);
    color: var(--weplex-text);
    font-size: var(--weplex-text-sm);
    width: 200px;
    outline: none;
  }

  .setting-input.mono {
    font-family: var(--weplex-font-mono);
    font-size: var(--weplex-text-xs);
  }

  .setting-input:focus {
    border-color: var(--weplex-accent);
  }

  .setting-select {
    padding: 5px 8px;
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-md);
    background: var(--weplex-bg);
    color: var(--weplex-text);
    font-size: var(--weplex-text-sm);
    outline: none;
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

  /* ═══ shipooor footer stamp (SM size) ═══ */
  .shipooor-footer {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 0;
    padding: 8px 0 0;
  }

  .shipooor-link {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    text-decoration: none;
    cursor: pointer;
  }

  .shipooor-stamp-outer {
    display: inline-block;
    transform: rotate(-5deg);
    border: 2px solid rgba(255, 255, 255, 0.7);
    border-radius: 3px;
    padding: 2px;
    position: relative;
    overflow: visible;
    transition:
      border-color 0.3s ease,
      box-shadow 0.3s ease;
  }

  .shipooor-stamp-inner {
    display: flex;
    align-items: center;
    justify-content: center;
    border: 0.75px solid rgba(255, 255, 255, 0.4);
    border-radius: 1.5px;
    height: 16px;
    padding: 0 6px;
    font-family: var(--weplex-font-mono);
    font-weight: 700;
    font-size: 8px;
    letter-spacing: 0.1em;
    line-height: 1;
    text-transform: uppercase;
    color: rgba(255, 255, 255, 0.85);
    position: relative;
    overflow: hidden;
    transition: border-color 0.3s ease;
  }

  .shipooor-stamp-inner::before {
    content: '';
    position: absolute;
    inset: 0;
    background: #fff;
    transform: translateY(100%);
    transition: transform 0.3s cubic-bezier(0.65, 0, 0.35, 1);
    z-index: 0;
  }

  .shipooor-stamp-outer::after {
    content: '';
    position: absolute;
    inset: -3px;
    border-radius: 5px;
    border: 1.5px solid transparent;
    pointer-events: none;
  }

  .shipooor-ft {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.3);
    transition: color 0.3s ease;
  }

  .shipooor-ft strong {
    color: rgba(255, 255, 255, 0.45);
    font-weight: 600;
  }

  .shipooor-x {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.2);
    text-decoration: none;
    transition: color 0.3s ease;
  }

  /* Hover: Combo */
  .shipooor-link:hover .shipooor-stamp-outer {
    border-color: #fff;
    box-shadow:
      0 0 16px rgba(255, 255, 255, 0.25),
      0 0 40px rgba(255, 255, 255, 0.08);
    animation: shipooor-slam 0.5s cubic-bezier(0.22, 1, 0.36, 1);
  }
  .shipooor-link:hover .shipooor-stamp-inner::before {
    transform: translateY(0);
  }
  .shipooor-link:hover .shipooor-stamp-inner {
    border-color: rgba(255, 255, 255, 0.7);
    color: #000;
  }
  .shipooor-link:hover .shipooor-stamp-outer::after {
    animation: shipooor-ring 0.6s ease-out forwards;
  }
  .shipooor-link:hover .shipooor-ft,
  .shipooor-link:hover .shipooor-ft strong {
    color: rgba(255, 255, 255, 0.8);
  }
  .shipooor-link:hover ~ .shipooor-x {
    color: rgba(255, 255, 255, 0.8);
  }

  @keyframes shipooor-slam {
    0% {
      transform: rotate(-5deg) scale(1);
    }
    15% {
      transform: rotate(-5deg) scale(1.3) translateY(-6px);
    }
    35% {
      transform: rotate(-3deg) scale(0.92) translateY(1px);
    }
    55% {
      transform: rotate(-5deg) scale(1.05);
    }
    100% {
      transform: rotate(-5deg) scale(1);
    }
  }

  @keyframes shipooor-ring {
    0% {
      inset: -3px;
      border-color: rgba(255, 255, 255, 0.5);
      opacity: 1;
    }
    100% {
      inset: -16px;
      border-color: rgba(255, 255, 255, 0);
      opacity: 0;
    }
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

  /* ═══ Account tab ═══ */
  .account-card {
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-lg);
    padding: 12px;
    margin-bottom: 12px;
  }

  .account-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .account-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .account-name {
    font-size: var(--weplex-text-sm);
    font-weight: 500;
    cursor: pointer;
  }

  .account-name:hover {
    color: var(--weplex-accent);
  }

  .account-actions {
    margin-top: 12px;
  }

  .inline-edit {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .inline-edit .setting-input {
    width: 160px;
  }

  .sync-status {
    font-size: var(--weplex-text-xs);
    font-family: var(--weplex-font-mono);
    color: var(--weplex-text-muted);
    text-transform: uppercase;
  }

  .sync-status.sync-error {
    color: var(--weplex-error);
  }

  .auth-form {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .auth-input {
    width: 100% !important;
    box-sizing: border-box;
  }

  .auth-buttons {
    display: flex;
    gap: 6px;
    margin-top: 4px;
  }

  .auth-error {
    font-size: var(--weplex-text-sm);
    color: var(--weplex-error);
    padding: 8px;
    border: 1px solid var(--weplex-error);
    border-radius: var(--weplex-radius-md);
    margin-bottom: 12px;
    background: rgba(239, 68, 68, 0.06);
  }

  .oauth-divider {
    display: flex;
    align-items: center;
    gap: 12px;
    margin: 16px 0;
  }

  .oauth-divider::before,
  .oauth-divider::after {
    content: '';
    flex: 1;
    height: 1px;
    background: var(--weplex-border);
  }

  .oauth-divider-text {
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .oauth-buttons {
    display: flex;
    gap: 8px;
  }

  .oauth-btn {
    flex: 1;
    padding: 6px 12px !important;
    text-align: center;
    border-color: var(--weplex-border);
  }

  .oauth-btn:hover {
    background: var(--weplex-surface-hover);
  }
</style>

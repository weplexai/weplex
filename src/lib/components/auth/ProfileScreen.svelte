<script lang="ts">
  import { authStore } from '../../stores/authStore.svelte';
  import { Button, Input } from '../ui';

  interface Props {
    switchScreen: (screen: string) => void;
  }

  let { switchScreen }: Props = $props();

  // Profile editing
  let editDisplayName = $state('');
  let editingName = $state(false);

  function startEditName() {
    editDisplayName = authStore.user?.displayName || '';
    editingName = true;
  }

  async function saveDisplayName() {
    try {
      await authStore.updateProfile({ displayName: editDisplayName.trim() || undefined });
      editingName = false;
    } catch {
      // Error set in authStore
    }
  }

  async function handleSignOut() {
    await authStore.logout();
    switchScreen('sign-in');
  }
</script>

{#if authStore.error}
  <div class="auth-error">{authStore.error}</div>
{/if}

<!-- Avatar + Name -->
<div class="profile-header">
  <div class="profile-avatar">
    {(authStore.user?.displayName || authStore.user?.email || '?')[0].toUpperCase()}
  </div>
  <div class="profile-header-info">
    <div class="profile-row">
      {#if editingName}
        <div class="inline-edit">
          <Input
            class="auth-input"
            type="text"
            bind:value={editDisplayName}
            placeholder="Display name"
            size="sm"
            onkeydown={(e) => e.key === 'Enter' && saveDisplayName()}
          />
          <Button variant="primary" size="sm" onclick={saveDisplayName}>Save</Button>
          <Button variant="secondary" size="sm" onclick={() => (editingName = false)}>Cancel</Button>
        </div>
      {:else}
        <span class="profile-name" onclick={startEditName}>
          {authStore.user?.displayName || 'Set display name'}
        </span>
      {/if}
    </div>
    <span class="profile-email">{authStore.user?.email}</span>
  </div>
</div>

<!-- Account Details -->
<div class="profile-group">
  <h4 class="profile-group-title">Account</h4>
  <div class="profile-meta-row">
    <span class="profile-detail-label">Email</span>
    <span class="profile-detail-value">
      {#if authStore.user?.emailVerified}
        <span class="badge badge-green">Verified</span>
      {:else}
        <span class="badge badge-yellow">Not verified</span>
        <Button variant="ghost" class="link-btn" onclick={() => { switchScreen('verify-email'); authStore.sendVerificationCode(); }}>Verify now</Button>
      {/if}
    </span>
  </div>
  <div class="profile-meta-row">
    <span class="profile-detail-label">Plan</span>
    <span class="profile-detail-value">
      <span class="badge">{authStore.user?.plan || 'Free'}</span>
    </span>
  </div>
  <div class="profile-meta-row">
    <span class="profile-detail-label">Sync</span>
    <span class="profile-detail-value">
      <span class="sync-status" class:sync-error={authStore.syncStatus === 'error'}>
        {authStore.syncStatus}
      </span>
    </span>
  </div>
</div>

<!-- Connected Accounts -->
<div class="profile-group">
  <h4 class="profile-group-title">Connected Accounts</h4>
  <div class="profile-meta-row">
    <span class="profile-detail-label">GitHub</span>
    <span class="profile-detail-value">
      {#if authStore.user?.githubId}
        <span class="badge badge-green">Connected</span>
      {:else}
        <button class="connect-btn" onclick={() => authStore.oauthLogin('github')}>Connect</button>
      {/if}
    </span>
  </div>
  <div class="profile-meta-row">
    <span class="profile-detail-label">Google</span>
    <span class="profile-detail-value">
      {#if authStore.user?.googleId}
        <span class="badge badge-green">Connected</span>
      {:else}
        <button class="connect-btn" onclick={() => authStore.oauthLogin('google')}>Connect</button>
      {/if}
    </span>
  </div>
</div>

<!-- Actions -->
<div class="profile-group profile-group-danger">
  <div class="profile-actions-row">
    <Button variant="secondary" size="sm" onclick={() => switchScreen('forgot-password')}>Change Password</Button>
    <Button variant="danger" size="sm" onclick={handleSignOut}>Sign Out</Button>
  </div>
</div>

<style>
  .auth-error {
    font-size: var(--weplex-text-sm);
    color: var(--weplex-error);
    padding: 8px;
    border: 1px solid var(--weplex-error);
    border-radius: var(--weplex-radius-md);
    margin-bottom: 8px;
    background: rgba(239, 68, 68, 0.06);
  }

  .profile-header {
    display: flex;
    align-items: center;
    gap: 16px;
    margin-bottom: 28px;
  }

  .profile-avatar {
    width: 56px;
    height: 56px;
    border-radius: 50%;
    background: color-mix(in srgb, var(--weplex-accent) 25%, transparent);
    border: 2px solid color-mix(in srgb, var(--weplex-accent) 40%, transparent);
    color: var(--weplex-accent);
    font-size: 22px;
    font-weight: 600;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .profile-header-info {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }

  .profile-email {
    font-size: var(--weplex-text-sm);
    color: var(--weplex-text-muted);
    font-family: var(--weplex-font-mono);
  }

  .profile-row {
    display: flex;
    align-items: center;
  }

  .profile-name {
    font-size: var(--weplex-text-md);
    font-weight: 600;
    cursor: pointer;
  }

  .profile-name:hover {
    color: var(--weplex-accent);
  }

  .inline-edit {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
  }

  .inline-edit :global(.auth-input) {
    flex: 1;
  }

  .profile-group {
    margin-bottom: 24px;
  }

  .profile-group-title {
    font-size: var(--weplex-text-xs);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--weplex-text-muted);
    margin: 0 0 10px;
  }

  .profile-group-danger {
    margin-top: 32px;
    padding-top: 20px;
    border-top: 1px solid var(--weplex-border);
  }

  .profile-meta-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 0;
    border-bottom: 1px solid var(--weplex-border);
  }

  .profile-detail-label {
    font-size: var(--weplex-text-sm);
    color: var(--weplex-text-secondary);
  }

  .profile-detail-value {
    font-size: var(--weplex-text-sm);
    color: var(--weplex-text);
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .badge {
    font-size: var(--weplex-text-xs);
    padding: 1px 6px;
    border-radius: var(--weplex-radius-full);
    background: color-mix(in srgb, var(--weplex-accent) 15%, transparent);
    color: var(--weplex-accent);
    font-weight: 400;
  }

  .badge-green {
    background: rgba(16, 185, 129, 0.15);
    color: #10b981;
  }

  .badge-yellow {
    background: rgba(245, 158, 11, 0.15);
    color: #f59e0b;
  }

  :global(.profile-detail-value .link-btn) {
    font-size: var(--weplex-text-xs);
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

  .connect-btn {
    font-size: var(--weplex-text-xs);
    padding: 3px 10px;
    border-radius: var(--weplex-radius-sm);
    border: 1px solid var(--weplex-border);
    background: transparent;
    color: var(--weplex-text-secondary);
    cursor: pointer;
    transition: all 0.15s;
  }

  .connect-btn:hover {
    border-color: var(--weplex-accent);
    color: var(--weplex-accent);
  }

  .profile-actions-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }
</style>

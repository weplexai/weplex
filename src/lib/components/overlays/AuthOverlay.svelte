<script lang="ts">
  import { authStore } from '../../stores/authStore.svelte';
  import { uiStore } from '../../stores/uiStore';
  import { Button, Modal, Input } from '../ui';

  type AuthScreen =
    | 'sign-in'
    | 'register'
    | 'verify-email'
    | 'forgot-password'
    | 'reset-password'
    | 'profile';

  // Determine initial screen based on auth state
  let screen = $state<AuthScreen>(authStore.isAuthenticated ? 'profile' : 'sign-in');

  // Form fields
  let email = $state('');
  let password = $state('');
  let displayName = $state('');
  let code = $state('');
  let newPassword = $state('');
  let successMessage = $state<string | null>(null);
  let validationError = $state<string | null>(null);

  // Profile editing
  let editDisplayName = $state('');
  let editingName = $state(false);

  // Resend cooldown
  let resendCooldown = $state(0);
  let resendTimer: ReturnType<typeof setInterval> | null = null;

  const EMAIL_RE = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

  function clearMessages() {
    validationError = null;
    successMessage = null;
    authStore.clearError();
  }

  function switchScreen(next: AuthScreen) {
    clearMessages();
    screen = next;
  }

  function startResendCooldown() {
    resendCooldown = 60;
    if (resendTimer) clearInterval(resendTimer);
    resendTimer = setInterval(() => {
      resendCooldown--;
      if (resendCooldown <= 0 && resendTimer) {
        clearInterval(resendTimer);
        resendTimer = null;
      }
    }, 1000);
  }

  // ── Sign In ──────────────────────────────────────────────────────────────

  async function handleSignIn() {
    clearMessages();
    if (!EMAIL_RE.test(email)) {
      validationError = 'Please enter a valid email address';
      return;
    }
    if (!password) {
      validationError = 'Password is required';
      return;
    }
    try {
      await authStore.login(email, password);
      email = '';
      password = '';
      uiStore.closeOverlay();
    } catch {
      // Error set in authStore
    }
  }

  // ── Register ─────────────────────────────────────────────────────────────

  async function handleRegister() {
    clearMessages();
    if (!EMAIL_RE.test(email)) {
      validationError = 'Please enter a valid email address';
      return;
    }
    if (password.length < 8) {
      validationError = 'Password must be at least 8 characters';
      return;
    }
    try {
      await authStore.register(email, password);
      if (displayName.trim()) {
        await authStore.updateProfile({ displayName: displayName.trim() }).catch(() => {});
      }
      password = '';
      displayName = '';
      switchScreen('verify-email');
    } catch {
      // Error set in authStore
    }
  }

  // ── OAuth ────────────────────────────────────────────────────────────────

  async function handleOAuth(provider: 'github' | 'google') {
    clearMessages();
    try {
      await authStore.oauthLogin(provider);
      uiStore.closeOverlay();
    } catch {
      // Error set in authStore
    }
  }

  // ── Verify Email ─────────────────────────────────────────────────────────

  async function handleVerifyEmail() {
    clearMessages();
    if (code.length !== 6) {
      validationError = 'Please enter a 6-digit code';
      return;
    }
    try {
      await authStore.verifyEmail(code);
      code = '';
      uiStore.closeOverlay();
    } catch {
      // Error set in authStore
    }
  }

  async function handleResendCode() {
    clearMessages();
    try {
      await authStore.sendVerificationCode();
      startResendCooldown();
      successMessage = 'Verification code sent';
    } catch {
      // Error set in authStore
    }
  }

  // ── Forgot Password ──────────────────────────────────────────────────────

  let forgotEmail = $state('');

  async function handleForgotPassword() {
    clearMessages();
    if (!EMAIL_RE.test(forgotEmail)) {
      validationError = 'Please enter a valid email address';
      return;
    }
    try {
      await authStore.forgotPassword(forgotEmail);
      switchScreen('reset-password');
    } catch {
      // Error set in authStore
    }
  }

  // ── Reset Password ───────────────────────────────────────────────────────

  async function handleResetPassword() {
    clearMessages();
    if (code.length !== 6) {
      validationError = 'Please enter a 6-digit code';
      return;
    }
    if (newPassword.length < 8) {
      validationError = 'Password must be at least 8 characters';
      return;
    }
    try {
      await authStore.resetPassword(forgotEmail, code, newPassword);
      code = '';
      newPassword = '';
      successMessage = 'Password reset successfully';
      switchScreen('sign-in');
      successMessage = 'Password reset successfully. Sign in with your new password.';
    } catch {
      // Error set in authStore
    }
  }

  // ── Profile ──────────────────────────────────────────────────────────────

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

  // Sync screen with auth state changes
  $effect(() => {
    if (authStore.isAuthenticated && screen === 'sign-in') {
      screen = 'profile';
    }
    if (!authStore.isAuthenticated && screen === 'profile') {
      screen = 'sign-in';
    }
  });
</script>

<Modal onclose={() => uiStore.closeOverlay()} position="center" label="Authentication" class="auth-card">
    {#if screen === 'sign-in'}
      <!-- ════════ Sign In ════════ -->
      <h2 class="auth-title">Sign In to Weplex</h2>

      {#if successMessage}
        <div class="auth-success">{successMessage}</div>
      {/if}
      {#if validationError}
        <div class="auth-error">{validationError}</div>
      {/if}
      {#if authStore.error}
        <div class="auth-error">{authStore.error}</div>
      {/if}

      <div class="auth-form">
        <Input
          class="auth-input"
          type="email"
          placeholder="Email"
          bind:value={email}
          onkeydown={(e) => e.key === 'Enter' && handleSignIn()}
        />
        <Input
          class="auth-input"
          type="password"
          placeholder="Password"
          bind:value={password}
          onkeydown={(e) => e.key === 'Enter' && handleSignIn()}
        />
        <Button
          variant="primary"
          class="auth-btn-full"
          disabled={authStore.loading || !email || !password}
          onclick={handleSignIn}
        >
          {authStore.loading ? 'Loading...' : 'Sign In'}
        </Button>
      </div>

      <Button variant="ghost" class="link-btn forgot" onclick={() => switchScreen('forgot-password')}>
        Forgot password?
      </Button>

      <div class="oauth-divider">
        <span class="oauth-divider-text">or</span>
      </div>

      <div class="oauth-buttons">
        <Button
          variant="secondary"
          class="btn-oauth"
          disabled={authStore.loading}
          onclick={() => handleOAuth('github')}
        >
          GitHub
        </Button>
        <Button
          variant="secondary"
          class="btn-oauth"
          disabled={authStore.loading}
          onclick={() => handleOAuth('google')}
        >
          Google
        </Button>
      </div>

      <p class="auth-footer-text">
        Don't have an account?
        <Button variant="ghost" class="link-btn" onclick={() => switchScreen('register')}>Register</Button>
      </p>
    {:else if screen === 'register'}
      <!-- ════════ Register ════════ -->
      <h2 class="auth-title">Create Account</h2>

      {#if validationError}
        <div class="auth-error">{validationError}</div>
      {/if}
      {#if authStore.error}
        <div class="auth-error">{authStore.error}</div>
      {/if}

      <div class="auth-form">
        <Input
          class="auth-input"
          type="text"
          placeholder="Display name (optional)"
          bind:value={displayName}
        />
        <Input
          class="auth-input"
          type="email"
          placeholder="Email"
          bind:value={email}
          onkeydown={(e) => e.key === 'Enter' && handleRegister()}
        />
        <Input
          class="auth-input"
          type="password"
          placeholder="Password (min 8 characters)"
          bind:value={password}
          onkeydown={(e) => e.key === 'Enter' && handleRegister()}
        />
        <Button
          variant="primary"
          class="auth-btn-full"
          disabled={authStore.loading || !email || !password}
          onclick={handleRegister}
        >
          {authStore.loading ? 'Loading...' : 'Create Account'}
        </Button>
      </div>

      <p class="auth-footer-text">
        Already have an account?
        <Button variant="ghost" class="link-btn" onclick={() => switchScreen('sign-in')}>Sign In</Button>
      </p>
    {:else if screen === 'verify-email'}
      <!-- ════════ Verify Email ════════ -->
      <h2 class="auth-title">Verify Your Email</h2>
      <p class="auth-subtitle">
        We sent a 6-digit code to {authStore.user?.email || email}
      </p>

      {#if successMessage}
        <div class="auth-success">{successMessage}</div>
      {/if}
      {#if validationError}
        <div class="auth-error">{validationError}</div>
      {/if}
      {#if authStore.error}
        <div class="auth-error">{authStore.error}</div>
      {/if}

      <div class="auth-form">
        <input
          class="auth-input code-input"
          type="text"
          placeholder="000000"
          maxlength={6}
          bind:value={code}
          onkeydown={(e) => e.key === 'Enter' && handleVerifyEmail()}
        />
        <Button
          variant="primary"
          class="auth-btn-full"
          disabled={authStore.loading || code.length !== 6}
          onclick={handleVerifyEmail}
        >
          {authStore.loading ? 'Loading...' : 'Verify'}
        </Button>
      </div>

      <div class="verify-actions">
        <Button
          variant="ghost"
          class="link-btn"
          disabled={resendCooldown > 0 || authStore.loading}
          onclick={handleResendCode}
        >
          {resendCooldown > 0 ? `Resend in ${resendCooldown}s` : "Didn't receive? Resend"}
        </Button>
        <Button variant="ghost" class="link-btn" onclick={() => uiStore.closeOverlay()}>Skip for now</Button>
      </div>
    {:else if screen === 'forgot-password'}
      <!-- ════════ Forgot Password ════════ -->
      <h2 class="auth-title">Reset Password</h2>
      <p class="auth-subtitle">Enter your email and we'll send a reset code.</p>

      {#if validationError}
        <div class="auth-error">{validationError}</div>
      {/if}
      {#if authStore.error}
        <div class="auth-error">{authStore.error}</div>
      {/if}

      <div class="auth-form">
        <Input
          class="auth-input"
          type="email"
          placeholder="Email"
          bind:value={forgotEmail}
          onkeydown={(e) => e.key === 'Enter' && handleForgotPassword()}
        />
        <Button
          variant="primary"
          class="auth-btn-full"
          disabled={authStore.loading || !forgotEmail}
          onclick={handleForgotPassword}
        >
          {authStore.loading ? 'Loading...' : 'Send Reset Code'}
        </Button>
      </div>

      <p class="auth-footer-text">
        <Button variant="ghost" class="link-btn" onclick={() => switchScreen('sign-in')}>Back to Sign In</Button>
      </p>
    {:else if screen === 'reset-password'}
      <!-- ════════ Reset Password ════════ -->
      <h2 class="auth-title">Enter New Password</h2>
      <p class="auth-subtitle">
        We sent a 6-digit code to {forgotEmail}
      </p>

      {#if validationError}
        <div class="auth-error">{validationError}</div>
      {/if}
      {#if authStore.error}
        <div class="auth-error">{authStore.error}</div>
      {/if}

      <div class="auth-form">
        <input
          class="auth-input code-input"
          type="text"
          placeholder="000000"
          maxlength={6}
          bind:value={code}
        />
        <Input
          class="auth-input"
          type="password"
          placeholder="New password (min 8 characters)"
          bind:value={newPassword}
          onkeydown={(e) => e.key === 'Enter' && handleResetPassword()}
        />
        <Button
          variant="primary"
          class="auth-btn-full"
          disabled={authStore.loading || code.length !== 6 || !newPassword}
          onclick={handleResetPassword}
        >
          {authStore.loading ? 'Loading...' : 'Reset Password'}
        </Button>
      </div>

      <p class="auth-footer-text">
        <Button variant="ghost" class="link-btn" onclick={() => switchScreen('sign-in')}>Back to Sign In</Button>
      </p>
    {:else if screen === 'profile'}
      <!-- ════════ Profile ════════ -->
      <h2 class="auth-title">Account</h2>

      {#if authStore.error}
        <div class="auth-error">{authStore.error}</div>
      {/if}

      <div class="profile-section">
        <span class="profile-email">{authStore.user?.email}</span>

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

        <div class="profile-meta-row">
          <span class="profile-detail">
            Email:
            {#if authStore.user?.emailVerified}
              <span class="badge badge-green">Verified</span>
            {:else}
              <span class="badge badge-yellow">Not verified</span>
              <Button variant="ghost" class="link-btn" onclick={() => { screen = 'verify-email'; authStore.sendVerificationCode(); }}>Verify now</Button>
            {/if}
          </span>
        </div>

        <div class="profile-meta-row">
          <span class="profile-detail">
            Plan: <span class="badge">{authStore.user?.plan || 'Free'}</span>
          </span>
        </div>

        <div class="profile-meta-row">
          <span class="profile-detail">
            Sync: <span class="sync-status" class:sync-error={authStore.syncStatus === 'error'}>
              {authStore.syncStatus}
            </span>
          </span>
        </div>
      </div>

      <div class="profile-actions">
        <Button variant="danger" onclick={handleSignOut}>Sign Out</Button>
      </div>
    {/if}
</Modal>

<style>
  :global(.auth-card) {
    width: 380px;
    max-height: 90vh;
    overflow-y: auto;
    background: var(--weplex-surface);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-xl);
    box-shadow: var(--weplex-shadow-overlay);
    padding: 28px 24px;
  }

  .auth-title {
    font-size: var(--weplex-text-lg);
    font-weight: 600;
    margin-bottom: 6px;
  }

  .auth-subtitle {
    font-size: var(--weplex-text-sm);
    color: var(--weplex-text-muted);
    margin-bottom: 16px;
    line-height: 1.4;
  }

  .auth-form {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-top: 16px;
  }

  :global(.auth-input) {
    width: 100%;
    box-sizing: border-box;
    background: var(--weplex-bg);
  }

  .auth-input.code-input {
    text-align: center;
    font-size: 20px;
    letter-spacing: 0.4em;
    font-family: var(--weplex-font-mono);
    padding: 10px;
  }

  :global(.auth-btn-full) {
    width: 100%;
  }

  :global(.link-btn) {
    background: none;
    border: none;
    color: var(--weplex-accent);
    font-size: var(--weplex-text-sm);
    cursor: pointer;
    padding: 0;
    text-decoration: none;
  }

  :global(.link-btn:hover) {
    text-decoration: underline;
  }

  :global(.link-btn:disabled) {
    color: var(--weplex-text-muted);
    cursor: not-allowed;
    text-decoration: none;
  }

  :global(.link-btn.forgot) {
    display: block;
    margin-top: 10px;
    text-align: right;
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
  }

  :global(.link-btn.forgot:hover) {
    color: var(--weplex-accent);
  }

  .oauth-divider {
    display: flex;
    align-items: center;
    gap: 12px;
    margin: 18px 0;
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

  :global(.btn-oauth) {
    flex: 1;
  }

  .auth-footer-text {
    margin-top: 18px;
    text-align: center;
    font-size: var(--weplex-text-sm);
    color: var(--weplex-text-muted);
  }

  .auth-error {
    font-size: var(--weplex-text-sm);
    color: var(--weplex-error);
    padding: 8px;
    border: 1px solid var(--weplex-error);
    border-radius: var(--weplex-radius-md);
    margin-bottom: 8px;
    background: rgba(239, 68, 68, 0.06);
  }

  .auth-success {
    font-size: var(--weplex-text-sm);
    color: var(--weplex-active);
    padding: 8px;
    border: 1px solid var(--weplex-active);
    border-radius: var(--weplex-radius-md);
    margin-bottom: 8px;
    background: rgba(16, 185, 129, 0.06);
  }

  .verify-actions {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-top: 14px;
  }

  /* ═══ Profile screen ═══ */
  .profile-section {
    margin-top: 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
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
    font-size: var(--weplex-text-sm);
    font-weight: 500;
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

  .profile-meta-row {
    display: flex;
    align-items: center;
  }

  .profile-detail {
    font-size: var(--weplex-text-sm);
    color: var(--weplex-text-secondary);
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

  :global(.profile-detail .link-btn) {
    font-size: var(--weplex-text-xs);
    margin-left: 8px;
    text-decoration: underline;
  }

  :global(.profile-detail .link-btn:hover) {
    opacity: 0.8;
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

  .profile-actions {
    margin-top: 20px;
  }
</style>

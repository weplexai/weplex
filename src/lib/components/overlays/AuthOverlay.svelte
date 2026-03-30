<script lang="ts">
  import { authStore } from '../../stores/authStore.svelte';
  import { uiStore } from '../../stores/uiStore';

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

  // ── Keyboard ─────────────────────────────────────────────────────────────

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') uiStore.closeOverlay();
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

<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions a11y_no_noninteractive_element_interactions -->
<div
  class="overlay-backdrop"
  role="presentation"
  onclick={() => uiStore.closeOverlay()}
  onkeydown={handleKeydown}
>
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions a11y_no_noninteractive_element_interactions a11y_interactive_supports_focus -->
  <div
    class="auth-card"
    role="dialog"
    tabindex="-1"
    aria-label="Authentication"
    onclick={(e) => e.stopPropagation()}
  >
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
        <input
          class="auth-input"
          type="email"
          placeholder="Email"
          bind:value={email}
          onkeydown={(e) => e.key === 'Enter' && handleSignIn()}
        />
        <input
          class="auth-input"
          type="password"
          placeholder="Password"
          bind:value={password}
          onkeydown={(e) => e.key === 'Enter' && handleSignIn()}
        />
        <button
          class="btn-primary"
          disabled={authStore.loading || !email || !password}
          onclick={handleSignIn}
        >
          {authStore.loading ? 'Loading...' : 'Sign In'}
        </button>
      </div>

      <button class="link-btn forgot" onclick={() => switchScreen('forgot-password')}>
        Forgot password?
      </button>

      <div class="oauth-divider">
        <span class="oauth-divider-text">or</span>
      </div>

      <div class="oauth-buttons">
        <button
          class="btn-oauth"
          disabled={authStore.loading}
          onclick={() => handleOAuth('github')}
        >
          GitHub
        </button>
        <button
          class="btn-oauth"
          disabled={authStore.loading}
          onclick={() => handleOAuth('google')}
        >
          Google
        </button>
      </div>

      <p class="auth-footer-text">
        Don't have an account?
        <button class="link-btn" onclick={() => switchScreen('register')}>Register</button>
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
        <input
          class="auth-input"
          type="text"
          placeholder="Display name (optional)"
          bind:value={displayName}
        />
        <input
          class="auth-input"
          type="email"
          placeholder="Email"
          bind:value={email}
          onkeydown={(e) => e.key === 'Enter' && handleRegister()}
        />
        <input
          class="auth-input"
          type="password"
          placeholder="Password (min 8 characters)"
          bind:value={password}
          onkeydown={(e) => e.key === 'Enter' && handleRegister()}
        />
        <button
          class="btn-primary"
          disabled={authStore.loading || !email || !password}
          onclick={handleRegister}
        >
          {authStore.loading ? 'Loading...' : 'Create Account'}
        </button>
      </div>

      <p class="auth-footer-text">
        Already have an account?
        <button class="link-btn" onclick={() => switchScreen('sign-in')}>Sign In</button>
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
        <button
          class="btn-primary"
          disabled={authStore.loading || code.length !== 6}
          onclick={handleVerifyEmail}
        >
          {authStore.loading ? 'Loading...' : 'Verify'}
        </button>
      </div>

      <div class="verify-actions">
        <button
          class="link-btn"
          disabled={resendCooldown > 0 || authStore.loading}
          onclick={handleResendCode}
        >
          {resendCooldown > 0 ? `Resend in ${resendCooldown}s` : "Didn't receive? Resend"}
        </button>
        <button class="link-btn" onclick={() => uiStore.closeOverlay()}> Skip for now </button>
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
        <input
          class="auth-input"
          type="email"
          placeholder="Email"
          bind:value={forgotEmail}
          onkeydown={(e) => e.key === 'Enter' && handleForgotPassword()}
        />
        <button
          class="btn-primary"
          disabled={authStore.loading || !forgotEmail}
          onclick={handleForgotPassword}
        >
          {authStore.loading ? 'Loading...' : 'Send Reset Code'}
        </button>
      </div>

      <p class="auth-footer-text">
        <button class="link-btn" onclick={() => switchScreen('sign-in')}>Back to Sign In</button>
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
        <input
          class="auth-input"
          type="password"
          placeholder="New password (min 8 characters)"
          bind:value={newPassword}
          onkeydown={(e) => e.key === 'Enter' && handleResetPassword()}
        />
        <button
          class="btn-primary"
          disabled={authStore.loading || code.length !== 6 || !newPassword}
          onclick={handleResetPassword}
        >
          {authStore.loading ? 'Loading...' : 'Reset Password'}
        </button>
      </div>

      <p class="auth-footer-text">
        <button class="link-btn" onclick={() => switchScreen('sign-in')}>Back to Sign In</button>
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
              <input
                class="auth-input"
                type="text"
                bind:value={editDisplayName}
                placeholder="Display name"
                onkeydown={(e) => e.key === 'Enter' && saveDisplayName()}
              />
              <button class="btn-sm save" onclick={saveDisplayName}>Save</button>
              <button class="btn-sm" onclick={() => (editingName = false)}>Cancel</button>
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
              <button class="link-btn" onclick={() => { screen = 'verify-email'; authStore.sendVerificationCode(); }}>Verify now</button>
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
        <button class="btn-signout" onclick={handleSignOut}>Sign Out</button>
      </div>
    {/if}
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

  .auth-card {
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

  .auth-input {
    width: 100%;
    box-sizing: border-box;
    padding: 8px 10px;
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-md);
    background: var(--weplex-bg);
    color: var(--weplex-text);
    font-size: var(--weplex-text-sm);
    outline: none;
  }

  .auth-input:focus {
    border-color: var(--weplex-accent);
  }

  .auth-input.code-input {
    text-align: center;
    font-size: 20px;
    letter-spacing: 0.4em;
    font-family: var(--weplex-font-mono);
    padding: 10px;
  }

  .btn-primary {
    width: 100%;
    padding: 8px 16px;
    border: none;
    border-radius: var(--weplex-radius-md);
    background: var(--weplex-accent);
    color: white;
    font-size: var(--weplex-text-sm);
    font-weight: 500;
    cursor: pointer;
    transition: opacity var(--weplex-duration-fast) var(--weplex-easing);
  }

  .btn-primary:hover {
    opacity: 0.9;
  }

  .btn-primary:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .link-btn {
    background: none;
    border: none;
    color: var(--weplex-accent);
    font-size: var(--weplex-text-sm);
    cursor: pointer;
    padding: 0;
    text-decoration: none;
  }

  .link-btn:hover {
    text-decoration: underline;
  }

  .link-btn:disabled {
    color: var(--weplex-text-muted);
    cursor: not-allowed;
    text-decoration: none;
  }

  .link-btn.forgot {
    display: block;
    margin-top: 10px;
    text-align: right;
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
  }

  .link-btn.forgot:hover {
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

  .btn-oauth {
    flex: 1;
    padding: 7px 12px;
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-md);
    background: transparent;
    color: var(--weplex-text-secondary);
    font-size: var(--weplex-text-sm);
    cursor: pointer;
    text-align: center;
    transition: all var(--weplex-duration-fast) var(--weplex-easing);
  }

  .btn-oauth:hover {
    background: var(--weplex-surface-hover);
  }

  .btn-oauth:disabled {
    opacity: 0.5;
    cursor: not-allowed;
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

  .inline-edit .auth-input {
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

  .link-btn {
    background: none;
    border: none;
    color: var(--weplex-accent);
    font-size: var(--weplex-text-xs);
    cursor: pointer;
    padding: 0;
    margin-left: 8px;
    text-decoration: underline;
  }

  .link-btn:hover {
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

  .profile-actions {
    margin-top: 20px;
  }

  .btn-signout {
    padding: 6px 14px;
    border: 1px solid var(--weplex-error);
    border-radius: var(--weplex-radius-sm);
    background: transparent;
    color: var(--weplex-error);
    font-size: var(--weplex-text-sm);
    cursor: pointer;
    transition: background var(--weplex-duration-fast) var(--weplex-easing);
  }

  .btn-signout:hover {
    background: rgba(239, 68, 68, 0.1);
  }
</style>

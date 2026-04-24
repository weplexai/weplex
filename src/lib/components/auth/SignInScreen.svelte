<script lang="ts">
  import { authStore } from '../../stores/authStore.svelte';
  import { uiStore } from '../../stores/uiStore';
  import { Button, Input } from '../ui';

  interface Props {
    switchScreen: (screen: string) => void;
  }

  let { switchScreen }: Props = $props();

  const EMAIL_RE = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

  let email = $state('');
  let password = $state('');
  let successMessage = $state<string | null>(null);
  let validationError = $state<string | null>(null);

  function clearMessages() {
    validationError = null;
    successMessage = null;
    authStore.clearError();
  }

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

  async function handleOAuth(provider: 'github' | 'google') {
    clearMessages();
    try {
      await authStore.oauthLogin(provider);
      uiStore.closeOverlay();
    } catch {
      // Error set in authStore
    }
  }

  // Allow parent to set success message (e.g. after password reset)
  export function setSuccessMessage(msg: string) {
    successMessage = msg;
  }
</script>

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

<style>
  .auth-title {
    font-size: var(--weplex-text-lg);
    font-weight: 600;
    margin-bottom: 6px;
  }

  .auth-form {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-top: 16px;
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
</style>

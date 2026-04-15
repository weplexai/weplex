<script lang="ts">
  import { authStore } from '../../stores/authStore.svelte';
  import { Button, Input } from '../ui';

  interface Props {
    switchScreen: (screen: string) => void;
    onResetEmailSet: (email: string) => void;
  }

  let { switchScreen, onResetEmailSet }: Props = $props();

  const EMAIL_RE = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

  let forgotEmail = $state('');
  let validationError = $state<string | null>(null);

  function clearMessages() {
    validationError = null;
    authStore.clearError();
  }

  async function handleForgotPassword() {
    clearMessages();
    if (!EMAIL_RE.test(forgotEmail)) {
      validationError = 'Please enter a valid email address';
      return;
    }
    try {
      await authStore.forgotPassword(forgotEmail);
      onResetEmailSet(forgotEmail);
      switchScreen('reset-password');
    } catch {
      // Error set in authStore
    }
  }
</script>

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
  <Button variant="ghost" class="link-btn" onclick={() => switchScreen(authStore.isAuthenticated ? 'profile' : 'sign-in')}>
    {authStore.isAuthenticated ? 'Back to Account' : 'Back to Sign In'}
  </Button>
</p>

<style>
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
</style>

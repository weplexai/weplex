<script lang="ts">
  import { authStore } from '../../stores/authStore.svelte';
  import { Button, Input } from '../ui';

  interface Props {
    switchScreen: (screen: string) => void;
    resetEmail: string;
  }

  let { switchScreen, resetEmail }: Props = $props();

  let code = $state('');
  let newPassword = $state('');
  let validationError = $state<string | null>(null);

  function clearMessages() {
    validationError = null;
    authStore.clearError();
  }

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
      await authStore.resetPassword(resetEmail, code, newPassword);
      code = '';
      newPassword = '';
      switchScreen('sign-in');
    } catch {
      // Error set in authStore
    }
  }
</script>

<h2 class="auth-title">Enter New Password</h2>
<p class="auth-subtitle">
  We sent a 6-digit code to {resetEmail}
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

  .auth-input.code-input {
    text-align: center;
    font-size: 20px;
    letter-spacing: 0.4em;
    font-family: var(--weplex-font-mono);
    padding: 10px;
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

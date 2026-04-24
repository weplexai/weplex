<script lang="ts">
  import { onDestroy } from 'svelte';
  import { authStore } from '../../stores/authStore.svelte';
  import { uiStore } from '../../stores/uiStore';
  import { Button } from '../ui';

  interface Props {
    switchScreen: (screen: string) => void;
  }

  let { switchScreen }: Props = $props();

  let code = $state('');
  let successMessage = $state<string | null>(null);
  let validationError = $state<string | null>(null);

  // Resend cooldown
  let resendCooldown = $state(0);
  let resendTimer: ReturnType<typeof setInterval> | null = null;

  function clearMessages() {
    validationError = null;
    successMessage = null;
    authStore.clearError();
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

  onDestroy(() => {
    if (resendTimer) clearInterval(resendTimer);
  });
</script>

<h2 class="auth-title">Verify Your Email</h2>
<p class="auth-subtitle">
  We sent a 6-digit code to {authStore.user?.email || ''}
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

  .verify-actions {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-top: 14px;
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

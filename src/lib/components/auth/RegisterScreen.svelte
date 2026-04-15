<script lang="ts">
  import { authStore } from '../../stores/authStore.svelte';
  import { Button, Input } from '../ui';

  interface Props {
    switchScreen: (screen: string) => void;
  }

  let { switchScreen }: Props = $props();

  const EMAIL_RE = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

  let email = $state('');
  let password = $state('');
  let displayName = $state('');
  let validationError = $state<string | null>(null);

  function clearMessages() {
    validationError = null;
    authStore.clearError();
  }

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
</script>

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

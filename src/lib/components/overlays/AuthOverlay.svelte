<script lang="ts">
  import { authStore } from '../../stores/authStore.svelte';
  import SignInScreen from '../auth/SignInScreen.svelte';
  import RegisterScreen from '../auth/RegisterScreen.svelte';
  import VerifyEmailScreen from '../auth/VerifyEmailScreen.svelte';
  import ForgotPasswordScreen from '../auth/ForgotPasswordScreen.svelte';
  import ResetPasswordScreen from '../auth/ResetPasswordScreen.svelte';
  import ProfileScreen from '../auth/ProfileScreen.svelte';

  type AuthScreen =
    | 'sign-in'
    | 'register'
    | 'verify-email'
    | 'forgot-password'
    | 'reset-password'
    | 'profile';

  // Determine initial screen based on auth state
  let screen = $state<AuthScreen>(authStore.isAuthenticated ? 'profile' : 'sign-in');

  // Shared state for forgot-password -> reset-password flow
  let resetEmail = $state('');

  function switchScreen(next: string) {
    authStore.clearError();
    screen = next as AuthScreen;
  }

  function handleResetEmailSet(email: string) {
    resetEmail = email;
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

<div class="auth-inner" class:auth-centered={screen !== 'profile'}>
  <div class="auth-card">
    {#if screen === 'sign-in'}
      <SignInScreen {switchScreen} />
    {:else if screen === 'register'}
      <RegisterScreen {switchScreen} />
    {:else if screen === 'verify-email'}
      <VerifyEmailScreen {switchScreen} />
    {:else if screen === 'forgot-password'}
      <ForgotPasswordScreen {switchScreen} onResetEmailSet={handleResetEmailSet} />
    {:else if screen === 'reset-password'}
      <ResetPasswordScreen {switchScreen} {resetEmail} />
    {:else if screen === 'profile'}
      <ProfileScreen {switchScreen} />
    {/if}
  </div>
</div>

<style>
  .auth-inner {
    width: 100%;
    height: 100%;
    background: var(--weplex-bg);
    overflow-y: auto;
    display: flex;
  }

  .auth-card {
    width: 100%;
    max-width: 700px;
    padding: 32px 40px;
  }

  .auth-inner.auth-centered {
    align-items: center;
    justify-content: center;
  }

  .auth-inner.auth-centered > .auth-card {
    max-width: 380px;
    width: 380px;
  }

  /* ── Shared styles used by child screen components via :global ── */

  :global(.auth-input) {
    width: 100%;
    box-sizing: border-box;
    background: var(--weplex-bg);
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
    text-decoration: none;
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

  :global(.btn-oauth) {
    flex: 1;
  }
</style>

<script lang="ts">
  import { ShieldAlert, ShieldCheck, ShieldX } from 'lucide-svelte';
  import type { GuardVerdict, ResourceVerdict } from '../../types/guard';

  interface Props {
    verdict: GuardVerdict;
    findings?: ResourceVerdict | null;
    size?: 'sm' | 'md';
    onclick?: () => void;
  }

  let { verdict, findings = null, size = 'sm', onclick }: Props = $props();

  // Hide the green icon on small badges to keep the happy-path Hub clean.
  let showGreen = $derived(verdict !== 'green' || size === 'md');

  let iconSize = $derived(size === 'sm' ? 13 : 16);

  let activeCount = $derived.by(() => {
    if (!findings) return 0;
    const overridden = new Set(findings.overriddenFindings);
    return findings.findings.filter((f) => !overridden.has(f.ruleId)).length;
  });

  let tooltip = $derived.by(() => {
    if (verdict === 'yellow') {
      const n = activeCount || 1;
      return n === 1
        ? '1 warning — click to review'
        : `${n} warnings — click to review`;
    }
    if (verdict === 'red') {
      const n = activeCount || 1;
      return n === 1 ? 'Blocked: 1 issue' : `Blocked: ${n} issues`;
    }
    return '';
  });

  let interactive = $derived(typeof onclick === 'function');
</script>

{#if showGreen}
  {#if interactive}
    <button
      type="button"
      class="guard-badge guard-{verdict} guard-{size}"
      title={tooltip}
      aria-label={tooltip || 'Resource passes guard checks'}
      onclick={onclick}
    >
      {#if verdict === 'green'}
        <ShieldCheck size={iconSize} />
      {:else if verdict === 'yellow'}
        <ShieldAlert size={iconSize} />
      {:else}
        <ShieldX size={iconSize} />
      {/if}
    </button>
  {:else}
    <span
      class="guard-badge guard-{verdict} guard-{size}"
      title={tooltip}
      aria-label={tooltip || 'Resource passes guard checks'}
    >
      {#if verdict === 'green'}
        <ShieldCheck size={iconSize} />
      {:else if verdict === 'yellow'}
        <ShieldAlert size={iconSize} />
      {:else}
        <ShieldX size={iconSize} />
      {/if}
    </span>
  {/if}
{/if}

<style>
  .guard-badge {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: var(--weplex-radius-sm);
    border: none;
    background: transparent;
    cursor: default;
    flex-shrink: 0;
    line-height: 0;
  }

  button.guard-badge {
    cursor: pointer;
    padding: 2px;
    transition: background var(--weplex-duration-fast);
  }

  button.guard-badge:hover {
    background: var(--weplex-surface-hover);
  }

  .guard-sm {
    width: 18px;
    height: 18px;
  }

  .guard-md {
    width: 22px;
    height: 22px;
  }

  .guard-green {
    color: var(--weplex-active, #10b981);
  }

  .guard-yellow {
    color: var(--weplex-warning, #f59e0b);
  }

  .guard-red {
    color: var(--weplex-error, #ef4444);
  }
</style>

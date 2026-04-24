<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    variant?: 'default' | 'success' | 'warning' | 'error' | 'info' | 'accent';
    size?: 'sm' | 'md';
    class?: string;
    children: Snippet;
  }

  let {
    variant = 'default',
    size = 'sm',
    class: className = '',
    children,
  }: Props = $props();
</script>

<span class="badge badge-{variant} badge-{size} {className}">
  {@render children()}
</span>

<style>
  .badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-family: var(--weplex-font-ui);
    font-weight: 500;
    border-radius: var(--weplex-radius-full);
    white-space: nowrap;
  }

  /* Sizes */
  .badge-sm {
    padding: 1px 8px;
    font-size: var(--weplex-text-xs);
  }

  .badge-md {
    padding: 2px 10px;
    font-size: var(--weplex-text-sm);
  }

  /* Variants */
  .badge-default {
    background: var(--weplex-surface-hover);
    color: var(--weplex-text-secondary);
  }

  .badge-success {
    background: color-mix(in srgb, var(--weplex-active) 15%, transparent);
    color: var(--weplex-active);
  }

  .badge-warning {
    background: color-mix(in srgb, var(--weplex-warning) 15%, transparent);
    color: var(--weplex-warning);
  }

  .badge-error {
    background: color-mix(in srgb, var(--weplex-error) 15%, transparent);
    color: var(--weplex-error);
  }

  .badge-info {
    background: color-mix(in srgb, var(--weplex-info) 15%, transparent);
    color: var(--weplex-info);
  }

  .badge-accent {
    background: color-mix(in srgb, var(--weplex-accent) 15%, transparent);
    color: var(--weplex-accent);
  }
</style>

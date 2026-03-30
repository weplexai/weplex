<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    variant?: 'primary' | 'secondary' | 'danger' | 'ghost';
    size?: 'sm' | 'md';
    icon?: boolean;
    disabled?: boolean;
    type?: 'button' | 'submit';
    title?: string;
    class?: string;
    onclick?: (e: MouseEvent) => void;
    children: Snippet;
  }

  let {
    variant = 'secondary',
    size = 'md',
    icon = false,
    disabled = false,
    type = 'button',
    title,
    class: className = '',
    onclick,
    children,
  }: Props = $props();
</script>

<button
  class="btn btn-{variant} btn-{size} {className}"
  class:btn-icon={icon}
  {disabled}
  {type}
  {title}
  {onclick}
>
  {@render children()}
</button>

<style>
  .btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    border: none;
    font-family: var(--weplex-font-ui);
    font-weight: 500;
    cursor: pointer;
    transition:
      opacity 0.15s,
      background 0.15s,
      border-color 0.15s,
      color 0.15s;
    white-space: nowrap;
    flex-shrink: 0;
  }

  .btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  /* Sizes */
  .btn-sm {
    padding: 4px 10px;
    font-size: var(--weplex-text-xs);
    border-radius: var(--weplex-radius-sm);
  }

  .btn-md {
    padding: 6px 14px;
    font-size: var(--weplex-text-sm);
    border-radius: var(--weplex-radius-md);
  }

  /* Icon-only */
  .btn-icon.btn-sm {
    width: 26px;
    height: 26px;
    padding: 0;
  }

  .btn-icon.btn-md {
    width: 32px;
    height: 32px;
    padding: 0;
  }

  /* Primary */
  .btn-primary {
    background: var(--weplex-accent);
    color: white;
  }

  .btn-primary:hover:not(:disabled) {
    opacity: 0.85;
  }

  /* Secondary */
  .btn-secondary {
    background: transparent;
    border: 1px solid var(--weplex-border);
    color: var(--weplex-text-muted);
  }

  .btn-secondary:hover:not(:disabled) {
    border-color: var(--weplex-text-muted);
    color: var(--weplex-text);
  }

  /* Danger */
  .btn-danger {
    background: transparent;
    border: 1px solid color-mix(in srgb, var(--weplex-error) 30%, transparent);
    color: var(--weplex-error);
  }

  .btn-danger:hover:not(:disabled) {
    background: color-mix(in srgb, var(--weplex-error) 10%, transparent);
    border-color: var(--weplex-error);
  }

  /* Ghost */
  .btn-ghost {
    background: transparent;
    color: var(--weplex-text-muted);
  }

  .btn-ghost:hover:not(:disabled) {
    color: var(--weplex-text);
    background: var(--weplex-surface-hover);
  }
</style>

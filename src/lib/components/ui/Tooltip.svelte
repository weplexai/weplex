<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    text: string;
    position?: 'top' | 'bottom' | 'left' | 'right';
    class?: string;
    children: Snippet;
  }

  let {
    text,
    position = 'top',
    class: className = '',
    children,
  }: Props = $props();

  let visible = $state(false);
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="tooltip-wrapper {className}"
  onmouseenter={() => (visible = true)}
  onmouseleave={() => (visible = false)}
>
  {@render children()}
  {#if visible && text}
    <div class="tooltip tooltip-{position}" role="tooltip">
      {text}
    </div>
  {/if}
</div>

<style>
  .tooltip-wrapper {
    position: relative;
    display: inline-flex;
  }

  .tooltip {
    position: absolute;
    padding: 4px 8px;
    background: var(--weplex-surface-hover);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-sm);
    color: var(--weplex-text);
    font-family: var(--weplex-font-ui);
    font-size: var(--weplex-text-xs);
    white-space: nowrap;
    pointer-events: none;
    z-index: 200;
    box-shadow: var(--weplex-shadow-md);
  }

  .tooltip-top {
    bottom: calc(100% + 6px);
    left: 50%;
    transform: translateX(-50%);
  }

  .tooltip-bottom {
    top: calc(100% + 6px);
    left: 50%;
    transform: translateX(-50%);
  }

  .tooltip-left {
    right: calc(100% + 6px);
    top: 50%;
    transform: translateY(-50%);
  }

  .tooltip-right {
    left: calc(100% + 6px);
    top: 50%;
    transform: translateY(-50%);
  }
</style>

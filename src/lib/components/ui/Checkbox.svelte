<script lang="ts">
  import type { Snippet } from 'svelte';
  import { Check } from 'lucide-svelte';

  interface Props {
    checked?: boolean;
    disabled?: boolean;
    onchange?: (checked: boolean) => void;
    class?: string;
    children?: Snippet;
  }

  let {
    checked = $bindable(false),
    disabled = false,
    onchange,
    class: className = '',
    children,
  }: Props = $props();

  function handleChange(e: Event) {
    const target = e.target as HTMLInputElement;
    checked = target.checked;
    onchange?.(checked);
  }
</script>

<label class="checkbox {className}" class:checkbox-disabled={disabled}>
  <input
    type="checkbox"
    class="checkbox-input"
    bind:checked
    {disabled}
    onchange={handleChange}
  />
  <span class="checkbox-box" class:checkbox-checked={checked}>
    {#if checked}
      <Check size={12} />
    {/if}
  </span>
  {#if children}
    <span class="checkbox-label">
      {@render children()}
    </span>
  {/if}
</label>

<style>
  .checkbox {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    cursor: pointer;
    font-family: var(--weplex-font-ui);
    font-size: var(--weplex-text-sm);
    color: var(--weplex-text-secondary);
  }

  .checkbox-disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .checkbox-input {
    position: absolute;
    opacity: 0;
    width: 0;
    height: 0;
    pointer-events: none;
  }

  .checkbox-box {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-sm);
    background: var(--weplex-surface);
    color: white;
    transition:
      background 0.15s,
      border-color 0.15s;
    flex-shrink: 0;
  }

  .checkbox-checked {
    background: var(--weplex-accent);
    border-color: var(--weplex-accent);
  }

  .checkbox:hover:not(.checkbox-disabled) .checkbox-box:not(.checkbox-checked) {
    border-color: var(--weplex-border-active);
  }

  .checkbox-label {
    user-select: none;
  }
</style>

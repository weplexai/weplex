<script lang="ts">
  import { ChevronDown } from 'lucide-svelte';

  interface Option {
    value: string | null;
    label: string;
  }

  interface Props {
    options: Option[];
    value?: string | null;
    onchange?: (value: string) => void;
    id?: string;
    class?: string;
  }

  let { options, value = '', onchange, id, class: className = '' }: Props = $props();

  let open = $state(false);
  let btnEl: HTMLButtonElement | undefined = $state();
  let listEl: HTMLUListElement | undefined = $state();

  let selectedLabel = $derived(
    options.find((o) => o.value === value)?.label || ''
  );

  function toggle() {
    open = !open;
  }

  function select(opt: Option) {
    open = false;
    onchange?.(String(opt.value ?? ''));
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      open = false;
      btnEl?.focus();
    }
  }

  function handleClickOutside(e: MouseEvent) {
    if (
      open &&
      btnEl &&
      listEl &&
      !btnEl.contains(e.target as Node) &&
      !listEl.contains(e.target as Node)
    ) {
      open = false;
    }
  }
</script>

<svelte:window onclick={handleClickOutside} onkeydown={handleKeydown} />

<div class="weplex-select {className}" class:open>
  <button
    type="button"
    class="weplex-select-trigger"
    {id}
    onclick={toggle}
    bind:this={btnEl}
    aria-haspopup="listbox"
    aria-expanded={open}
  >
    <span class="weplex-select-value">{selectedLabel}</span>
    <ChevronDown size={12} class="weplex-select-chevron" />
  </button>

  {#if open}
    <ul class="weplex-select-dropdown" role="listbox" bind:this={listEl}>
      {#each options as opt (String(opt.value))}
        <li role="option" aria-selected={opt.value === value}>
          <button
            type="button"
            class="weplex-select-option"
            class:active={opt.value === value}
            onclick={() => select(opt)}
          >
            {opt.label}
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .weplex-select {
    position: relative;
    display: inline-flex;
  }

  .weplex-select-trigger {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 8px;
    min-width: 80px;
    height: 28px;
    background: var(--weplex-surface);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-md);
    color: var(--weplex-text);
    font-family: var(--weplex-font-ui);
    font-size: var(--weplex-text-sm);
    cursor: pointer;
    transition:
      border-color 0.15s,
      background 0.15s;
    white-space: nowrap;
  }

  .weplex-select-trigger:hover {
    border-color: var(--weplex-border-active);
    background: var(--weplex-surface-hover);
  }

  .open .weplex-select-trigger {
    border-color: var(--weplex-accent);
  }

  .weplex-select-value {
    flex: 1;
    text-align: left;
  }

  .weplex-select-trigger :global(.weplex-select-chevron) {
    opacity: 0.5;
    flex-shrink: 0;
    transition: transform 0.15s;
  }

  .open .weplex-select-trigger :global(.weplex-select-chevron) {
    transform: rotate(180deg);
  }

  .weplex-select-dropdown {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    right: 0;
    min-width: 100%;
    max-height: 200px;
    overflow-y: auto;
    margin: 0;
    padding: 4px;
    list-style: none;
    background: var(--weplex-surface);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-md);
    box-shadow: var(--weplex-shadow-md);
    z-index: 100;
  }

  .weplex-select-dropdown li {
    margin: 0;
    padding: 0;
  }

  .weplex-select-option {
    display: block;
    width: 100%;
    padding: 5px 8px;
    background: none;
    border: none;
    border-radius: var(--weplex-radius-sm);
    color: var(--weplex-text-secondary);
    font-family: var(--weplex-font-ui);
    font-size: var(--weplex-text-sm);
    text-align: left;
    cursor: pointer;
    white-space: nowrap;
    transition:
      background 0.1s,
      color 0.1s;
  }

  .weplex-select-option:hover {
    background: var(--weplex-surface-hover);
    color: var(--weplex-text);
  }

  .weplex-select-option.active {
    color: var(--weplex-accent);
  }
</style>

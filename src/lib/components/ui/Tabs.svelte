<script lang="ts">
  interface Tab {
    id: string;
    label: string;
  }

  interface Props {
    tabs: Tab[];
    active: string;
    onchange: (id: string) => void;
    orientation?: 'horizontal' | 'vertical';
    class?: string;
  }

  let {
    tabs,
    active,
    onchange,
    orientation = 'horizontal',
    class: className = '',
  }: Props = $props();
</script>

<div
  class="tabs tabs-{orientation} {className}"
  role="tablist"
  aria-orientation={orientation}
>
  {#each tabs as tab (tab.id)}
    <button
      class="tab"
      class:active={active === tab.id}
      role="tab"
      aria-selected={active === tab.id}
      data-tab={tab.id}
      onclick={() => onchange(tab.id)}
    >
      {tab.label}
    </button>
  {/each}
</div>

<style>
  .tabs {
    display: flex;
    gap: 2px;
  }

  .tabs-vertical {
    flex-direction: column;
  }

  .tabs-horizontal {
    flex-direction: row;
    border-bottom: 1px solid var(--weplex-border);
    padding-bottom: 1px;
  }

  .tab {
    padding: 7px 8px;
    border: none;
    border-radius: var(--weplex-radius-md);
    background: transparent;
    color: var(--weplex-text-secondary);
    font-family: var(--weplex-font-ui);
    font-size: var(--weplex-text-sm);
    cursor: pointer;
    transition:
      background 0.1s,
      color 0.1s;
    white-space: nowrap;
  }

  /* Vertical */
  .tabs-vertical .tab {
    display: block;
    width: 100%;
    text-align: left;
  }

  /* Horizontal */
  .tabs-horizontal .tab {
    border-radius: var(--weplex-radius-sm) var(--weplex-radius-sm) 0 0;
  }

  .tab:hover {
    background: var(--weplex-surface);
  }

  .tab.active {
    background: var(--weplex-surface-hover);
    color: var(--weplex-text);
  }
</style>

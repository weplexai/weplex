<script lang="ts">
  interface Props {
    checked?: boolean;
    disabled?: boolean;
    onchange?: (checked: boolean) => void;
    class?: string;
  }

  let {
    checked = $bindable(false),
    disabled = false,
    onchange,
    class: className = '',
  }: Props = $props();

  function handleClick() {
    if (disabled) return;
    checked = !checked;
    onchange?.(checked);
  }
</script>

<button
  class="toggle {className}"
  class:toggle-on={checked}
  class:toggle-disabled={disabled}
  role="switch"
  aria-checked={checked}
  {disabled}
  onclick={handleClick}
  type="button"
>
  <span class="toggle-thumb"></span>
</button>

<style>
  .toggle {
    position: relative;
    width: 36px;
    height: 20px;
    border: none;
    border-radius: var(--weplex-radius-full);
    background: var(--weplex-border);
    cursor: pointer;
    padding: 2px;
    transition: background 0.2s;
    flex-shrink: 0;
  }

  .toggle-on {
    background: var(--weplex-accent);
  }

  .toggle-disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .toggle-thumb {
    display: block;
    width: 16px;
    height: 16px;
    border-radius: var(--weplex-radius-full);
    background: white;
    transition: transform 0.2s;
  }

  .toggle-on .toggle-thumb {
    transform: translateX(16px);
  }
</style>

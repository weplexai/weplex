<script lang="ts">
  interface Props {
    value?: string;
    placeholder?: string;
    type?: 'text' | 'email' | 'password' | 'url' | 'number';
    size?: 'sm' | 'md';
    mono?: boolean;
    disabled?: boolean;
    id?: string;
    class?: string;
    oninput?: (e: Event) => void;
    onchange?: (e: Event) => void;
    onkeydown?: (e: KeyboardEvent) => void;
  }

  let {
    value = $bindable(''),
    placeholder = '',
    type = 'text',
    size = 'md',
    mono = false,
    disabled = false,
    id,
    class: className = '',
    oninput,
    onchange,
    onkeydown,
  }: Props = $props();
</script>

<input
  class="input input-{size} {className}"
  class:input-mono={mono}
  {type}
  {placeholder}
  {disabled}
  {id}
  bind:value
  {oninput}
  {onchange}
  {onkeydown}
/>

<style>
  .input {
    width: 100%;
    background: var(--weplex-surface);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-md);
    color: var(--weplex-text);
    font-family: var(--weplex-font-ui);
    outline: none;
    transition:
      border-color 0.15s,
      background 0.15s;
  }

  .input:focus {
    border-color: var(--weplex-accent);
  }

  .input:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .input::placeholder {
    color: var(--weplex-text-muted);
  }

  /* Sizes */
  .input-sm {
    padding: 4px 8px;
    font-size: var(--weplex-text-xs);
    height: 28px;
  }

  .input-md {
    padding: 6px 10px;
    font-size: var(--weplex-text-sm);
    height: 34px;
  }

  /* Monospace */
  .input-mono {
    font-family: var(--weplex-font-mono);
  }
</style>

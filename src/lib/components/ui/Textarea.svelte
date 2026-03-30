<script lang="ts">
  interface Props {
    value?: string;
    placeholder?: string;
    rows?: number;
    mono?: boolean;
    resize?: 'none' | 'vertical';
    disabled?: boolean;
    id?: string;
    class?: string;
    oninput?: (e: Event) => void;
    onchange?: (e: Event) => void;
  }

  let {
    value = $bindable(''),
    placeholder = '',
    rows = 3,
    mono = false,
    resize = 'vertical',
    disabled = false,
    id,
    class: className = '',
    oninput,
    onchange,
  }: Props = $props();
</script>

<textarea
  class="textarea {className}"
  class:textarea-mono={mono}
  style:resize
  {rows}
  {placeholder}
  {disabled}
  {id}
  bind:value
  {oninput}
  {onchange}
></textarea>

<style>
  .textarea {
    width: 100%;
    background: var(--weplex-surface);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-md);
    color: var(--weplex-text);
    font-family: var(--weplex-font-ui);
    font-size: var(--weplex-text-sm);
    padding: 8px 10px;
    outline: none;
    transition:
      border-color 0.15s,
      background 0.15s;
  }

  .textarea:focus {
    border-color: var(--weplex-accent);
  }

  .textarea:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .textarea::placeholder {
    color: var(--weplex-text-muted);
  }

  .textarea-mono {
    font-family: var(--weplex-font-mono);
  }
</style>

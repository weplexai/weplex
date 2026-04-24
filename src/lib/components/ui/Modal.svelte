<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    onclose: () => void;
    position?: 'center' | 'top';
    label?: string;
    class?: string;
    children: Snippet;
  }

  let {
    onclose,
    position = 'center',
    label = 'Dialog',
    class: className = '',
    children,
  }: Props = $props();

  function handleBackdropClick(e: MouseEvent) {
    if (e.target === e.currentTarget) {
      onclose();
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.stopPropagation();
      onclose();
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions a11y_no_noninteractive_element_interactions -->
<div
  class="modal-backdrop"
  class:modal-top={position === 'top'}
  class:modal-center={position === 'center'}
  role="presentation"
  onclick={handleBackdropClick}
  onkeydown={handleKeydown}
>
  <!-- svelte-ignore a11y_interactive_supports_focus -->
  <div
    class="modal-content {className}"
    role="dialog"
    tabindex="-1"
    aria-label={label}
    onclick={(e) => e.stopPropagation()}
  >
    {@render children()}
  </div>
</div>

<style>
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.4);
    display: flex;
    justify-content: center;
    z-index: 100;
  }

  .modal-top {
    padding-top: 15vh;
    align-items: flex-start;
  }

  .modal-center {
    align-items: center;
  }

  .modal-content {
    max-height: 85vh;
    overflow-y: auto;
  }
</style>

<script lang="ts">
  import type { SplitDirection } from '../../types';

  let {
    direction,
    onResize,
    onResizeEnd,
  }: {
    direction: SplitDirection;
    onResize: (delta: number) => void;
    onResizeEnd?: () => void;
  } = $props();

  let dragging = $state(false);

  function onPointerDown(e: PointerEvent) {
    e.preventDefault();
    dragging = true;
    const target = e.currentTarget as HTMLElement;
    target.setPointerCapture(e.pointerId);
  }

  function onPointerMove(e: PointerEvent) {
    if (!dragging) return;
    const delta = direction === 'horizontal' ? e.movementX : e.movementY;
    onResize(delta);
  }

  function onPointerUp(e: PointerEvent) {
    if (!dragging) return;
    dragging = false;
    const target = e.currentTarget as HTMLElement;
    target.releasePointerCapture(e.pointerId);
    onResizeEnd?.();
  }
</script>

<div
  class="split-divider"
  class:horizontal={direction === 'horizontal'}
  class:vertical={direction === 'vertical'}
  class:dragging
  role="separator"
  tabindex="-1"
  onpointerdown={onPointerDown}
  onpointermove={onPointerMove}
  onpointerup={onPointerUp}
  onpointercancel={onPointerUp}
>
  <div class="split-divider-line"></div>
</div>

<style>
  .split-divider {
    position: relative;
    flex-shrink: 0;
    z-index: 5;
    display: flex;
    align-items: center;
    justify-content: center;
    user-select: none;
    touch-action: none;
  }

  .split-divider.horizontal {
    width: 6px;
    cursor: col-resize;
  }

  .split-divider.vertical {
    height: 6px;
    cursor: row-resize;
  }

  .split-divider-line {
    background: var(--weplex-border);
    border-radius: 1px;
    transition:
      background 0.15s ease,
      width 0.15s ease,
      height 0.15s ease;
  }

  .horizontal .split-divider-line {
    width: 1px;
    height: 100%;
  }

  .vertical .split-divider-line {
    height: 1px;
    width: 100%;
  }

  .split-divider:hover .split-divider-line,
  .split-divider.dragging .split-divider-line {
    background: var(--weplex-accent);
  }

  .horizontal:hover .split-divider-line,
  .horizontal.dragging .split-divider-line {
    width: 2px;
  }

  .vertical:hover .split-divider-line,
  .vertical.dragging .split-divider-line {
    height: 2px;
  }
</style>

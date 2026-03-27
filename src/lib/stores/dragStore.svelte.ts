let dragType = $state<'session' | 'folder' | null>(null);
let dragId = $state<number | string | null>(null);
let dragEl = $state<HTMLElement | null>(null);
let startX = $state(0);
let startY = $state(0);
let currentX = $state(0);
let currentY = $state(0);
let isDragging = $state(false);

// Drop target info (set by Sidebar hit-testing or terminal drop zones)
export type DropTargetType =
  | 'before-session'
  | 'after-session'
  | 'folder'
  | 'pinned-zone'
  | 'unpinned-zone'
  | 'space-group'
  | 'split-left'
  | 'split-right'
  | 'split-top'
  | 'split-bottom'
  | 'split-center';

let dropTarget = $state<{
  type: DropTargetType;
  id?: number | string; // session, folder, space, or pane id
} | null>(null);

// Minimum pixels to move before drag starts
const DRAG_THRESHOLD = 5;

export const dragStore = {
  get dragType() {
    return dragType;
  },
  get dragId() {
    return dragId;
  },
  get isDragging() {
    return isDragging;
  },
  get currentX() {
    return currentX;
  },
  get currentY() {
    return currentY;
  },
  get dropTarget() {
    return dropTarget;
  },

  startPotentialDrag(
    type: 'session' | 'folder',
    id: number | string,
    x: number,
    y: number,
    el: HTMLElement,
  ) {
    dragType = type;
    dragId = id;
    startX = x;
    startY = y;
    currentX = x;
    currentY = y;
    dragEl = el;
    isDragging = false;
    dropTarget = null;
  },

  updatePosition(x: number, y: number) {
    currentX = x;
    currentY = y;
    if (!isDragging) {
      const dist = Math.sqrt((x - startX) ** 2 + (y - startY) ** 2);
      if (dist > DRAG_THRESHOLD) {
        isDragging = true;
        if (dragEl) dragEl.style.opacity = '0.4';
      }
    }
  },

  setDropTarget(target: typeof dropTarget) {
    dropTarget = target;
  },

  endDrag() {
    if (dragEl) dragEl.style.opacity = '';
    dragType = null;
    dragId = null;
    dragEl = null;
    isDragging = false;
    dropTarget = null;
  },
};

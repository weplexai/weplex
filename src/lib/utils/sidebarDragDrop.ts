import type { DropTargetType } from '../stores/dragStore';
import { dragStore } from '../stores/dragStore';
import { sessionStore } from '../stores/sessionStore';
import { spaceStore } from '../stores/spaceStore';
import { folderStore } from '../stores/folderStore';
import { splitStore } from '../stores/splitStore';
import { findNode } from './splitTree';
import type { Folder } from '../types';

/** Extract numeric session ID from a DOM element's data-session-id attribute. */
function getSessionIdFromEl(el: HTMLElement): number | null {
  const raw = el.dataset.sessionId;
  if (!raw) return null;
  const id = Number(raw);
  return isNaN(id) ? null : id;
}

function findSession(id: number) {
  return sessionStore.sessions.find((s) => s.id === id);
}

function getSiblings(session: { pinned: boolean; folderId?: string; spaceId: string }) {
  if (session.pinned && session.folderId) {
    return sessionStore.getByFolder(session.folderId);
  }
  if (session.pinned) {
    return sessionStore.getPinnedStandalone(session.spaceId);
  }
  return sessionStore.getUnpinned(session.spaceId);
}

/** Hit-test pointer position against sidebar elements to find drop target. */
export function findDropTarget(
  x: number,
  y: number,
  scrollEl: HTMLElement,
  spaceId: string,
): typeof dragStore.dropTarget {
  const draggedId = dragStore.dragId;
  const dragType = dragStore.dragType;

  if (dragType !== 'session') return null;

  // Check folder headers first
  const folderEls = scrollEl.querySelectorAll<HTMLElement>('[data-folder-id]');
  for (const folderEl of folderEls) {
    const header = folderEl.querySelector('.folder-header');
    if (!header) continue;
    const rect = header.getBoundingClientRect();
    if (y >= rect.top && y <= rect.bottom && x >= rect.left && x <= rect.right) {
      const folderId = folderEl.dataset.folderId!;
      return { type: 'folder', id: folderId };
    }
  }

  // Check session items
  const sessionEls = scrollEl.querySelectorAll<HTMLElement>('[data-session-id]');
  for (const el of sessionEls) {
    const rect = el.getBoundingClientRect();
    if (y >= rect.top && y <= rect.bottom && x >= rect.left && x <= rect.right) {
      const sessionId = getSessionIdFromEl(el);
      if (sessionId === null || sessionId === draggedId) continue;

      const midY = rect.top + rect.height / 2;
      return y < midY
        ? { type: 'before-session', id: sessionId }
        : { type: 'after-session', id: sessionId };
    }
  }

  // Check if pointer is in pinned or unpinned zone
  const pinnedZone = scrollEl.querySelector<HTMLElement>('.pinned-zone');
  const unpinnedZone = scrollEl.querySelector<HTMLElement>('.unpinned-zone');

  if (pinnedZone) {
    const rect = pinnedZone.getBoundingClientRect();
    if (y >= rect.top && y <= rect.bottom && x >= rect.left && x <= rect.right) {
      return { type: 'pinned-zone' };
    }
  }

  if (unpinnedZone) {
    const rect = unpinnedZone.getBoundingClientRect();
    if (y >= rect.top && y <= rect.bottom && x >= rect.left && x <= rect.right) {
      return { type: 'unpinned-zone' };
    }
  }

  // Hyperspace "By Space" fallback
  if (spaceStore.isHyperspace) {
    const spaceGroupEls = scrollEl.querySelectorAll<HTMLElement>(
      '.hyperspace-group[data-space-id]',
    );
    for (const groupEl of spaceGroupEls) {
      const rect = groupEl.getBoundingClientRect();
      if (y >= rect.top && y <= rect.bottom && x >= rect.left && x <= rect.right) {
        const targetSpaceId = groupEl.dataset.spaceId!;
        const draggedSession = sessionStore.sessions.find((s) => s.id === draggedId);
        if (draggedSession && draggedSession.spaceId !== targetSpaceId) {
          return { type: 'space-group', id: targetSpaceId };
        }
        return null;
      }
    }
  }

  // Check terminal panes (for drag-to-split) — disabled in Hyperspace
  if (spaceStore.isHyperspace) return null;
  const paneEls = document.querySelectorAll<HTMLElement>('[data-pane-id]');
  for (const paneEl of paneEls) {
    const rect = paneEl.getBoundingClientRect();
    if (y >= rect.top && y <= rect.bottom && x >= rect.left && x <= rect.right) {
      const paneId = paneEl.dataset.paneId!;
      const layout = splitStore.getLayout(spaceId);
      if (layout) {
        const leaf = findNode(layout, paneId);
        if (leaf && leaf.type === 'leaf' && leaf.sessionId === draggedId) return null;
      }
      const dx = (x - rect.left) / rect.width;
      const dy = (y - rect.top) / rect.height;
      const distLeft = dx;
      const distRight = 1 - dx;
      const distTop = dy;
      const distBottom = 1 - dy;
      const minDist = Math.min(distLeft, distRight, distTop, distBottom);
      const EDGE_THRESHOLD = 0.25;

      let zone: DropTargetType;
      if (minDist > EDGE_THRESHOLD) {
        zone = 'split-center';
      } else if (minDist === distLeft) {
        zone = 'split-left';
      } else if (minDist === distRight) {
        zone = 'split-right';
      } else if (minDist === distTop) {
        zone = 'split-top';
      } else {
        zone = 'split-bottom';
      }
      return { type: zone, id: paneId };
    }
  }

  return null;
}

/** Execute the drop action based on current drag state. */
export function executeDrop(spaceId: string, folders: Folder[]) {
  const target = dragStore.dropTarget;
  if (!target || dragStore.dragType !== 'session') return;

  const draggedId = dragStore.dragId as number;

  switch (target.type) {
    case 'folder': {
      const folderId = target.id as string;
      sessionStore.reorder(draggedId, null, { pinned: true, folderId });
      const folder = folders.find((f) => f.id === folderId);
      if (folder?.collapsed) folderStore.toggle(folderId);
      break;
    }
    case 'before-session': {
      const beforeId = target.id as number;
      const beforeSession = findSession(beforeId);
      if (beforeSession) {
        const draggedSession = findSession(draggedId);
        if (draggedSession && draggedSession.spaceId !== beforeSession.spaceId) {
          sessionStore.moveToSpace(draggedId, beforeSession.spaceId);
        } else {
          sessionStore.reorder(draggedId, beforeId, {
            pinned: beforeSession.pinned,
            folderId: beforeSession.folderId,
          });
        }
      }
      break;
    }
    case 'after-session': {
      const afterId = target.id as number;
      const afterSession = findSession(afterId);
      if (afterSession) {
        const draggedSession = findSession(draggedId);
        if (draggedSession && draggedSession.spaceId !== afterSession.spaceId) {
          sessionStore.moveToSpace(draggedId, afterSession.spaceId);
        } else {
          const siblings = getSiblings(afterSession);
          const idx = siblings.findIndex((s) => s.id === afterId);
          const nextId = idx < siblings.length - 1 ? siblings[idx + 1].id : null;
          sessionStore.reorder(draggedId, nextId, {
            pinned: afterSession.pinned,
            folderId: afterSession.folderId,
          });
        }
      }
      break;
    }
    case 'pinned-zone':
      sessionStore.reorder(draggedId, null, { pinned: true, folderId: undefined });
      break;
    case 'unpinned-zone':
      sessionStore.reorder(draggedId, null, { pinned: false, folderId: undefined });
      break;
    case 'space-group': {
      const targetSpaceId = target.id as string;
      sessionStore.moveToSpace(draggedId, targetSpaceId);
      break;
    }
    case 'split-left': {
      const paneId = target.id as string;
      splitStore.splitWithExistingSession(spaceId, paneId, 'horizontal', draggedId, 'before');
      break;
    }
    case 'split-right': {
      const paneId = target.id as string;
      splitStore.splitWithExistingSession(spaceId, paneId, 'horizontal', draggedId, 'after');
      break;
    }
    case 'split-top': {
      const paneId = target.id as string;
      splitStore.splitWithExistingSession(spaceId, paneId, 'vertical', draggedId, 'before');
      break;
    }
    case 'split-bottom': {
      const paneId = target.id as string;
      splitStore.splitWithExistingSession(spaceId, paneId, 'vertical', draggedId, 'after');
      break;
    }
    case 'split-center': {
      const paneId = target.id as string;
      splitStore.replaceSessionInPane(spaceId, paneId, draggedId);
      break;
    }
  }
}

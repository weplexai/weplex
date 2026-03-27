import type { SplitLeaf, SplitBranch, SplitNode, SplitDirection } from '../types';

// Unique pane ID generator — uses random UUIDs to avoid collisions after reload
export function nextPaneId(): string {
  return crypto.randomUUID().slice(0, 8);
}

// Create a leaf node for a session
export function createLeaf(sessionId: number): SplitLeaf {
  return { type: 'leaf', id: nextPaneId(), sessionId };
}

// Find a node by its id (depth-first)
export function findNode(root: SplitNode, nodeId: string): SplitNode | null {
  if (root.id === nodeId) return root;
  if (root.type === 'branch') {
    return findNode(root.children[0], nodeId) || findNode(root.children[1], nodeId);
  }
  return null;
}

// Find the parent branch of a node
export function findParent(root: SplitNode, nodeId: string): SplitBranch | null {
  if (root.type !== 'branch') return null;
  for (const child of root.children) {
    if (child.id === nodeId) return root;
    const found = findParent(child, nodeId);
    if (found) return found;
  }
  return null;
}

// Split a leaf into a branch with two children.
// The original leaf keeps its session, a new leaf gets newSessionId.
// Returns a new tree (immutable).
export function splitPane(
  root: SplitNode,
  paneId: string,
  direction: SplitDirection,
  newSessionId: number,
  position: 'before' | 'after' = 'after',
): { tree: SplitNode; newLeafId: string } {
  const newLeaf = createLeaf(newSessionId);

  function replace(node: SplitNode): SplitNode {
    if (node.id === paneId && node.type === 'leaf') {
      const children: [SplitNode, SplitNode] =
        position === 'before' ? [newLeaf, { ...node }] : [{ ...node }, newLeaf];
      const branch: SplitBranch = {
        type: 'branch',
        id: nextPaneId(),
        direction,
        ratio: 0.5,
        children,
      };
      return branch;
    }
    if (node.type === 'branch') {
      return {
        ...node,
        children: [replace(node.children[0]), replace(node.children[1])] as [SplitNode, SplitNode],
      };
    }
    return node;
  }

  return { tree: replace(root), newLeafId: newLeaf.id };
}

// Close a pane — remove it and promote its sibling.
// Returns null if root itself is the target (last pane).
export function closePane(root: SplitNode, paneId: string): SplitNode | null {
  if (root.type === 'leaf' && root.id === paneId) {
    return null; // removing the only pane
  }

  function replace(node: SplitNode): SplitNode | null {
    if (node.type !== 'branch') return node;

    // Check if one of the direct children is the target
    for (let i = 0; i < 2; i++) {
      if (node.children[i].id === paneId) {
        // Promote the sibling
        return node.children[1 - i];
      }
    }

    // Recurse into children
    const newChildren = node.children.map((c) => replace(c)) as [
      SplitNode | null,
      SplitNode | null,
    ];
    if (newChildren[0] === null || newChildren[1] === null) {
      // Should not happen at this level — null means root removal
      return newChildren[0] || newChildren[1];
    }
    return { ...node, children: [newChildren[0], newChildren[1]] };
  }

  return replace(root);
}

// Clamp ratio to safe bounds
const MIN_RATIO = 0.15;
const MAX_RATIO = 0.85;

function clampRatio(ratio: number): number {
  return Math.max(MIN_RATIO, Math.min(MAX_RATIO, ratio));
}

// Update ratio on a specific branch node (with clamping)
export function updateRatio(root: SplitNode, branchId: string, ratio: number): SplitNode {
  const clamped = clampRatio(ratio);
  if (root.type === 'leaf') return root;
  if (root.id === branchId && root.type === 'branch') {
    return { ...root, ratio: clamped };
  }
  return {
    ...root,
    children: [
      updateRatio(root.children[0], branchId, clamped),
      updateRatio(root.children[1], branchId, clamped),
    ] as [SplitNode, SplitNode],
  };
}

// Replace sessionId in a specific leaf (immutable)
export function replaceSessionInLeaf(
  root: SplitNode,
  paneId: string,
  newSessionId: number,
): SplitNode {
  if (root.type === 'leaf') {
    return root.id === paneId ? { ...root, sessionId: newSessionId } : root;
  }
  return {
    ...root,
    children: [
      replaceSessionInLeaf(root.children[0], paneId, newSessionId),
      replaceSessionInLeaf(root.children[1], paneId, newSessionId),
    ] as [SplitNode, SplitNode],
  };
}

// Get all leaf pane IDs in depth-first order (for focus navigation)
export function getAllLeafIds(root: SplitNode): string[] {
  if (root.type === 'leaf') return [root.id];
  return [...getAllLeafIds(root.children[0]), ...getAllLeafIds(root.children[1])];
}

// Get all session IDs present in the tree
export function getAllSessionIds(root: SplitNode): number[] {
  if (root.type === 'leaf') return [root.sessionId];
  return [...getAllSessionIds(root.children[0]), ...getAllSessionIds(root.children[1])];
}

// Find the leaf containing a given sessionId
export function findLeafBySessionId(root: SplitNode, sessionId: number): SplitLeaf | null {
  if (root.type === 'leaf') {
    return root.sessionId === sessionId ? root : null;
  }
  return (
    findLeafBySessionId(root.children[0], sessionId) ||
    findLeafBySessionId(root.children[1], sessionId)
  );
}

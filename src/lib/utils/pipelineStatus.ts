// Shared utilities for collaborative pipeline stage status display

import type { CollaborativeStageStatus } from '../types';

/** Unicode icon for a collaborative stage status. */
export function stageStatusIcon(status: CollaborativeStageStatus): string {
  switch (status) {
    case 'completed': return '\u2713';
    case 'running': return '\u25CF';
    case 'waiting': return '\u23F3';
    case 'failed': return '\u2717';
    case 'skipped': return '\u2013';
    default: return '\u25CB';
  }
}

/** CSS class name for a collaborative stage status. */
export function stageStatusClass(status: CollaborativeStageStatus): string {
  return status;
}

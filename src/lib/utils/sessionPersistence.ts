import type { Session } from '../types';
import { durableSave, durableRemove } from './durablePersist';

const STORAGE_KEY = 'weplex_sessions';
const ACTIVE_KEY = 'weplex_active_session';

/** Load persisted sessions from localStorage. Resets status to 'new' (PTY gone after restart). */
export function loadSessions(): {
  sessions: Session[];
  nextId: number;
  activeId: number | null;
} {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return { sessions: [], nextId: 1, activeId: null };
    const saved: Session[] = JSON.parse(raw);
    const maxId = saved.reduce((max, s) => Math.max(max, s.id), 0);
    const activeRaw = localStorage.getItem(ACTIVE_KEY);
    const activeId = activeRaw
      ? Number(activeRaw)
      : saved.length > 0
        ? saved[saved.length - 1].id
        : null;
    // Reset status — PTY connections are gone after restart
    // Save previous status so resume logic can skip finished sessions
    // Backfill order for sessions created before order field existed
    for (const s of saved) {
      s.previousStatus = s.status;
      s.status = 'new';
      if (s.order === undefined) s.order = s.createdAt;
    }
    return { sessions: saved, nextId: maxId + 1, activeId };
  } catch {
    return { sessions: [], nextId: 1, activeId: null };
  }
}

/** Persist sessions and active session ID to localStorage. */
export function persistSessions(
  sessions: Session[],
  activeSessionId: number | null,
): void {
  try {
    durableSave(STORAGE_KEY, JSON.stringify(sessions));
    if (activeSessionId !== null) {
      durableSave(ACTIVE_KEY, String(activeSessionId));
    } else {
      durableRemove(ACTIVE_KEY);
    }
  } catch {}
}

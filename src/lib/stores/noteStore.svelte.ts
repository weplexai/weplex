import type { Note } from '../types';
import { durableSave } from '../utils/durablePersist';

const STORAGE_KEY = 'weplex_notes';

function loadNotes(): Note[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    return JSON.parse(raw);
  } catch {
    return [];
  }
}

let notes = $state<Note[]>(loadNotes());

function persist() {
  try {
    durableSave(STORAGE_KEY, JSON.stringify(notes));
  } catch {}
}

export const noteStore = {
  get notes() {
    return notes;
  },

  getByKey(key: string): Note | undefined {
    return notes.find((n) => n.key === key);
  },

  upsert(key: string, keyType: 'cwd' | 'ssh', content: string): void {
    const idx = notes.findIndex((n) => n.key === key);
    const now = Date.now();
    if (idx !== -1) {
      notes[idx] = { ...notes[idx], content, updatedAt: now };
    } else if (content.trim()) {
      notes.push({
        id: crypto.randomUUID(),
        content,
        key,
        keyType,
        createdAt: now,
        updatedAt: now,
      });
    }
    persist();
  },

  delete(key: string): void {
    const idx = notes.findIndex((n) => n.key === key);
    if (idx !== -1) {
      notes.splice(idx, 1);
      persist();
    }
  },
};

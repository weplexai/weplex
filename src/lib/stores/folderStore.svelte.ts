import type { Folder } from '../types';
import { durableSave } from '../utils/durablePersist';

const STORAGE_KEY = 'weplex_folders';

function loadFolders(): Folder[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    return JSON.parse(raw);
  } catch {
    return [];
  }
}

let folders = $state<Folder[]>(loadFolders());

function persist() {
  try {
    durableSave(STORAGE_KEY, JSON.stringify(folders));
  } catch {}
}

export const folderStore = {
  get folders() {
    return folders;
  },

  getBySpace(spaceId: string): Folder[] {
    return folders.filter((f) => f.spaceId === spaceId).sort((a, b) => a.order - b.order);
  },

  create(name: string, spaceId: string): Folder {
    const id = `folder-${Date.now()}`;
    const folder: Folder = {
      id,
      name,
      spaceId,
      order: folders.filter((f) => f.spaceId === spaceId).length,
      collapsed: false,
    };
    folders.push(folder);
    persist();
    return folder;
  },

  rename(id: string, name: string) {
    const idx = folders.findIndex((f) => f.id === id);
    if (idx !== -1) {
      folders[idx] = { ...folders[idx], name };
      persist();
    }
  },

  toggle(id: string) {
    const idx = folders.findIndex((f) => f.id === id);
    if (idx !== -1) {
      folders[idx] = { ...folders[idx], collapsed: !folders[idx].collapsed };
      persist();
    }
  },

  remove(id: string) {
    folders = folders.filter((f) => f.id !== id);
    persist();
  },
};

import { check, type Update, type DownloadEvent } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';

export interface UpdateState {
  available: boolean;
  version: string;
  downloading: boolean;
  progress: number;
}

const state = $state<UpdateState>({
  available: false,
  version: '',
  downloading: false,
  progress: 0,
});

export const updateState = state;

let pendingUpdate: Update | null = null;

export async function checkForUpdates(): Promise<void> {
  try {
    const update = await check();
    if (update) {
      state.available = true;
      state.version = update.version;
      pendingUpdate = update;
    }
  } catch (e) {
    console.warn('[Weplex] Update check failed:', e);
  }
}

export async function installUpdate(): Promise<void> {
  if (!pendingUpdate) return;

  state.downloading = true;
  state.progress = 0;

  try {
    let downloaded = 0;
    let contentLength = 0;

    await pendingUpdate.downloadAndInstall((event: DownloadEvent) => {
      if (event.event === 'Started') {
        contentLength = event.data.contentLength ?? 0;
      } else if (event.event === 'Progress') {
        downloaded += event.data.chunkLength;
        state.progress = contentLength > 0 ? Math.round((downloaded / contentLength) * 100) : 0;
      } else if (event.event === 'Finished') {
        state.progress = 100;
      }
    });

    await relaunch();
  } catch (e) {
    console.error('[Weplex] Update install failed:', e);
    state.downloading = false;
  }
}

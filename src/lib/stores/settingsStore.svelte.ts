import type { AppSettings } from '../types';

const DEFAULTS: AppSettings = {
  defaultShell: '',
  defaultDirectory: '~',
  fontFamily: "'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace",
  fontSize: 13,
  theme: 'dark',
  sidebarDefault: 'expanded',
  idleTimeout: 300000,
  persistSessions: true,
  chatSoundEnabled: true,
  chatNotificationsEnabled: true,
};

let settings = $state<AppSettings>({ ...DEFAULTS });

export const settingsStore = {
  get settings() {
    return settings;
  },

  update(patch: Partial<AppSettings>) {
    settings = { ...settings, ...patch };
    if (patch.theme) {
      document.documentElement.setAttribute('data-theme', patch.theme);
    }
  },

  reset() {
    settings = { ...DEFAULTS };
    document.documentElement.removeAttribute('data-theme');
  },
};

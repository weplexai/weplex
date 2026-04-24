import type { OverlayType, HubSection } from '../types';

const SIDEBAR_WIDTH_KEY = 'weplex_sidebar_width';
const SIDEBAR_HIDDEN_KEY = 'weplex_sidebar_hidden';
const DETAIL_WIDTH_KEY = 'weplex_detail_width';

const MIN_WIDTH = 180;
const MAX_WIDTH = 450;
const DEFAULT_WIDTH = 240;

const MIN_DETAIL_WIDTH = 220;
const MAX_DETAIL_WIDTH = 560;
const DEFAULT_DETAIL_WIDTH = 280;

function loadSidebarWidth(): number {
  try {
    const raw = localStorage.getItem(SIDEBAR_WIDTH_KEY);
    if (!raw) return DEFAULT_WIDTH;
    const w = Number(raw);
    return w >= MIN_WIDTH && w <= MAX_WIDTH ? w : DEFAULT_WIDTH;
  } catch {
    return DEFAULT_WIDTH;
  }
}

function loadDetailWidth(): number {
  try {
    const raw = localStorage.getItem(DETAIL_WIDTH_KEY);
    if (!raw) return DEFAULT_DETAIL_WIDTH;
    const w = Number(raw);
    return w >= MIN_DETAIL_WIDTH && w <= MAX_DETAIL_WIDTH ? w : DEFAULT_DETAIL_WIDTH;
  } catch {
    return DEFAULT_DETAIL_WIDTH;
  }
}

function loadSidebarHidden(): boolean {
  try {
    return localStorage.getItem(SIDEBAR_HIDDEN_KEY) === 'true';
  } catch {
    return false;
  }
}

let detailPanelOpen = $state(false);
let detailWidthVal = $state(loadDetailWidth());
let spaceChatOpen = $state(false);
let activeOverlay = $state<OverlayType>('none');
let sidebarWidthVal = $state(loadSidebarWidth());
let sidebarHidden = $state(loadSidebarHidden());
let hubMode = $state(false);
let hubExiting = $state(false);
let hubSection = $state<HubSection>('resources');
let hubExitAt = $state(0);

export const uiStore = {
  get detailPanelOpen() {
    return detailPanelOpen;
  },
  get activeOverlay() {
    return activeOverlay;
  },
  get sidebarHidden() {
    return sidebarHidden;
  },

  get sidebarWidth(): number {
    if (sidebarHidden) return 0;
    return sidebarWidthVal;
  },

  get sidebarWidthRaw(): number {
    return sidebarWidthVal;
  },

  MIN_WIDTH,
  MAX_WIDTH,

  get sidebarVisible() {
    return !sidebarHidden;
  },

  toggleSidebar() {
    sidebarHidden = !sidebarHidden;
    try {
      localStorage.setItem(SIDEBAR_HIDDEN_KEY, String(sidebarHidden));
    } catch {}
  },

  showSidebar() {
    sidebarHidden = false;
    try {
      localStorage.setItem(SIDEBAR_HIDDEN_KEY, 'false');
    } catch {}
  },

  hideSidebar() {
    sidebarHidden = true;
    try {
      localStorage.setItem(SIDEBAR_HIDDEN_KEY, 'true');
    } catch {}
  },

  setSidebarWidth(w: number) {
    // Clamp and also enforce max 50% of window
    const maxHalf = Math.floor(window.innerWidth / 2);
    const clamped = Math.min(Math.max(w, MIN_WIDTH), MAX_WIDTH, maxHalf);
    sidebarWidthVal = clamped;
    try {
      localStorage.setItem(SIDEBAR_WIDTH_KEY, String(clamped));
    } catch {}
  },

  get spaceChatOpen() {
    return spaceChatOpen;
  },

  toggleSpaceChat() {
    spaceChatOpen = !spaceChatOpen;
  },

  openSpaceChat() {
    spaceChatOpen = true;
  },

  closeSpaceChat() {
    spaceChatOpen = false;
  },

  toggleDetailPanel() {
    detailPanelOpen = !detailPanelOpen;
  },

  get detailPanelWidth(): number {
    return detailWidthVal;
  },

  MIN_DETAIL_WIDTH,
  MAX_DETAIL_WIDTH,

  setDetailPanelWidth(w: number) {
    const maxHalf = Math.floor(window.innerWidth / 2);
    const clamped = Math.min(Math.max(w, MIN_DETAIL_WIDTH), MAX_DETAIL_WIDTH, maxHalf);
    detailWidthVal = clamped;
    try {
      localStorage.setItem(DETAIL_WIDTH_KEY, String(clamped));
    } catch {}
  },

  openOverlay(type: OverlayType) {
    activeOverlay = type;
  },

  closeOverlay() {
    activeOverlay = 'none';
  },

  toggleOverlay(type: OverlayType) {
    activeOverlay = activeOverlay === type ? 'none' : type;
  },

  openAuth() {
    activeOverlay = 'auth';
  },

  // Hub mode
  get hubMode() {
    return hubMode;
  },
  get hubSection(): HubSection {
    return hubSection;
  },

  enterHubMode(section?: HubSection) {
    if (section) hubSection = section;
    hubMode = true;
    activeOverlay = 'none';
  },

  get hubExitAt() {
    return hubExitAt;
  },
  get hubExiting() {
    return hubExiting;
  },

  exitHubMode() {
    hubExiting = true;
    hubExitAt = Date.now();
    // Fade out hub (200ms), then brief overlap where both are hidden (50ms)
    setTimeout(() => {
      hubMode = false;
      hubExiting = false;
    }, 250);
  },

  toggleHubMode() {
    if (hubMode) {
      hubMode = false;
    } else {
      hubMode = true;
      activeOverlay = 'none';
    }
  },

  setHubSection(section: HubSection) {
    hubSection = section;
  },
};

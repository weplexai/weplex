export type SidebarState = 'expanded' | 'collapsed' | 'overlay';
export type OverlayType =
  | 'none'
  | 'command-palette'
  | 'quick-switcher'
  | 'new-session'
  | 'settings'
  | 'space-modal'
  | 'agents'
  | 'auth'
  | 'marketplace'
  | 'uikit';
export type HubSection = 'resources' | 'commands' | 'marketplace' | 'spaces' | 'settings' | 'account';
export type SplitDirection = 'horizontal' | 'vertical';

export interface AppSettings {
  defaultShell: string;
  defaultDirectory: string;
  fontFamily: string;
  fontSize: number;
  theme: 'dark' | 'light';
  sidebarDefault: SidebarState;
  idleTimeout: number;
  persistSessions: boolean;
  chatSoundEnabled: boolean;
  chatNotificationsEnabled: boolean;
  agentshieldDeepScan: boolean;
}

export interface SplitLeaf {
  type: 'leaf';
  id: string;
  sessionId: number;
}

export interface SplitBranch {
  type: 'branch';
  id: string;
  direction: SplitDirection;
  ratio: number;
  children: [SplitNode, SplitNode];
}

export type SplitNode = SplitLeaf | SplitBranch;

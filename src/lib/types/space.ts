export type SpaceType = 'personal' | 'team';

export interface Space {
  id: string;
  name: string;
  color: string;
  order: number;
  profileId?: string;
  bgColor?: string;
  grain?: number;
  bgMode?: 'dark' | 'light';
  directory?: string;
  type: SpaceType;
  shared: boolean;
  teamId?: string;
  serverId?: string;
  createdBy?: string;
  defaultWorkflowId?: string;
}

export interface Profile {
  id: string;
  name: string;
  isDefault: boolean;
  configDir: string | null;
  envVars: Record<string, string>;
  linkedAccount?: {
    email?: string;
    plan?: string;
  };
}

export interface DiscoveredProfile {
  path: string;
  name: string;
  source: 'filesystem' | 'shell_config';
}

export interface Folder {
  id: string;
  name: string;
  spaceId: string;
  order: number;
  collapsed: boolean;
}

export const SPACE_COLORS = [
  '#8B5CF6', '#3B82F6', '#10B981', '#F59E0B', '#EF4444',
  '#EC4899', '#06B6D4', '#F97316', '#84CC16', '#A855F7',
];

export const SPACE_BG_COLORS = [
  '#7C3AED', '#2563EB', '#0D9488', '#16A34A', '#D97706',
  '#DC2626', '#DB2777', '#9333EA', '#0891B2', '#EA580C',
  '#65A30D', '#4F46E5',
  '#A78BFA', '#60A5FA', '#5EEAD4', '#86EFAC', '#FCD34D',
  '#FCA5A5', '#F9A8D4', '#C4B5FD', '#67E8F9', '#FDBA74',
  '#BEF264', '#A5B4FC',
];

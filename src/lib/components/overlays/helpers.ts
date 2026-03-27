import { tildeHome } from '../../utils/path';
import type { PipelineStage } from './types';

export function modelShort(m: string | null | undefined): string {
  if (!m) return '\u2014';
  if (m.includes('opus')) return 'opus';
  if (m.includes('sonnet')) return 'sonnet';
  if (m.includes('haiku')) return 'haiku';
  return m || '\u2014';
}

export function modelClass(m: string | null | undefined): string {
  if (!m) return '';
  if (m.includes('opus')) return 'opus';
  if (m.includes('sonnet')) return 'sonnet';
  if (m.includes('haiku')) return 'haiku';
  return '';
}

export function initial(name: string | null | undefined): string {
  if (!name) return '?';
  return name.charAt(0).toUpperCase();
}

export function shortenPath(p: string | null | undefined): string {
  if (!p) return '';
  return tildeHome(p);
}

export function getMissingAgents(stages: PipelineStage[], agentNameSet: Set<string>): string[] {
  const required = new Set(
    stages.flatMap((s) =>
      s.parallel
        ? (s.parallel.map((ps) => ps.agent).filter(Boolean) as string[])
        : s.agent
          ? [s.agent]
          : [],
    ),
  );
  return [...required].filter((a) => !agentNameSet.has(a));
}

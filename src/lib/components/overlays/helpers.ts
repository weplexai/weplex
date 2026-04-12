import { tildeHome } from '../../utils/path';

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


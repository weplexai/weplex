import type { Session } from '../types';
import { spaceStore } from '../stores/spaceStore';
import { profileStore } from '../stores/profileStore.svelte';

/**
 * Resolve the value to pass as `WEPLEX_PROFILE_ID` (PTY env) and as the
 * `profile_id` parameter to Tauri commands like `get_session_summary`.
 *
 * Single source of truth for the rule "what does this session encrypt under":
 *   session.profileId  ─→  space.profileId  ─→  'default'
 *
 * Resolves to `profile.configDir` when available (absolute path), falling
 * back to `'default'` for the system profile or any profile without a
 * configDir override. The encryption key in macOS Keychain is keyed off
 * this exact string, so any drift between writer (TerminalView) and reader
 * (TimelineTab / SessionHoverPreview / SpaceChat) silently flips notes into
 * the 🔒 unreadable state.
 */
export function resolveProfileEnvId(session: Pick<Session, 'profileId' | 'spaceId'>): string {
  const space = spaceStore.spaces.find((s) => s.id === session.spaceId);
  const profileId = session.profileId ?? space?.profileId ?? 'default';
  const profile = profileStore.getById(profileId) || profileStore.defaultProfile;
  return profile?.configDir || 'default';
}

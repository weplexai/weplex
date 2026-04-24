/**
 * Dev-only demo data seeder — populates localStorage with Portal-themed
 * spaces, sessions, and folders so screenshots look clean and on-brand.
 *
 * Triggered from `App.svelte` when `VITE_WEPLEX_DEMO=1` is in the env.
 * Does nothing in production. Wipes existing localStorage of real sessions
 * on first run, then marks itself as seeded so it doesn't wipe again if
 * you edit things between screenshots.
 *
 * Usage:
 *   VITE_WEPLEX_DEMO=1 pnpm tauri dev
 *
 * After screenshots are taken:
 *   - Remove the env var (demo data stays in localStorage — fine for dev)
 *   - Or clear localStorage manually to go back to real sessions
 */

import type { Space } from '../types/space';
import type { Session } from '../types/session';
import type { Profile } from '../types';

const MARKER_KEY = 'weplex_demo_seeded_v11';

function now() {
  return Date.now();
}

export function seedDemoData(): void {
  if (typeof localStorage === 'undefined') return;
  if (localStorage.getItem(MARKER_KEY)) return; // already seeded this install

  const t = now();
  const hour = 60 * 60 * 1000;

  // ── Spaces ───────────────────────────────────────────────────────────────
  // Sidebar shows the first letter of each space as an icon. Read vertically
  // across the five space slots: L · E · M · O · N.
  // (The Cave Johnson monologue is, after all, the acceptance criterion.)
  //
  // `id: 'default'` is overridden explicitly to prevent the auto-injected
  // built-in "Default" space from prepending a stray "D" and ruining the
  // acrostic.
  const spaces: Space[] = [
    {
      id: 'default',
      name: 'Lemon Division',
      color: '#f59e0b', // hazard yellow — the lemon
      order: 0,
      bgColor: '#18120a',
      bgMode: 'dark',
      type: 'personal',
      shared: false,
      directory: '~/code/cave-johnson-memoirs',
      profileId: 'profile-personal',
    },
    {
      id: 'enrichment',
      name: 'Enrichment Center',
      color: '#fc5e44',
      order: 1,
      // Portal-blue backdrop — brighter than the terminal so the
      // space-color feature reads clearly in the hero screenshot, but
      // still dark enough to feel moody and on-brand.
      bgColor: '#1a4270',
      bgMode: 'dark',
      type: 'personal',
      shared: false,
      directory: '~/code/aperture-chamber-17',
      profileId: 'default', // Work
    },
    {
      id: 'mainframe',
      name: 'Mainframe Labs',
      color: '#8B5CF6',
      order: 2,
      bgMode: 'dark',
      type: 'personal',
      shared: false,
      directory: '~/code/glados-core',
      profileId: 'default', // Work
    },
    {
      id: 'observation',
      name: 'Observation',
      color: '#06b6d4',
      order: 3,
      bgMode: 'dark',
      type: 'personal',
      shared: false,
      directory: '~/code/camera-grid',
      profileId: 'profile-personal', // Personal
    },
    {
      id: 'neurotoxin',
      name: 'Neurotoxin Ops',
      color: '#10b981',
      order: 4,
      bgMode: 'dark',
      type: 'personal',
      shared: false,
      directory: '~/code/defensive-systems',
      profileId: 'default', // Work
    },
  ];

  // ── Sessions ─────────────────────────────────────────────────────────────
  // Each space gets 4-5 sessions with a coherent theme so any space the user
  // clicks into for a screenshot has enough visual density.
  //
  // Session-name themes align with the space name:
  //   Lemon Division    → Cave Johnson lemon-rant universe
  //   Enrichment Center → test chambers, puzzle mechanics, portal gun
  //   Mainframe Labs    → GLaDOS internals, cores, voice lines
  //   Observation       → cameras, logs, audit, monitoring
  //   Neurotoxin Ops    → hazards, turrets, ventilation, overrides
  const mkSession = (
    id: number,
    name: string,
    agentType: Session['agentType'],
    spaceId: string,
    status: Session['status'],
    minutesAgo: number,
    cwd: string,
    extras: Partial<Session> = {},
  ): Session => ({
    id,
    name,
    type: 'agent',
    agentType,
    status,
    spaceId,
    pinned: false,
    order: t - minutesAgo * 60_000,
    createdAt: t - minutesAgo * 60_000,
    lastActivity: t - Math.min(minutesAgo, 60) * 60_000,
    command: agentType,
    cwd,
    ...extras,
  });

  const sessions: Session[] = [
    // ── L — Lemon Division (active space) ────────────────────────────────
    mkSession(1, 'claude: combustible-lemon-proto', 'claude', 'default', 'active', 480, '~/code/lemons/combustible', {
      model: 'claude-opus-4-7', tokensIn: 84_000, tokensOut: 12_000, cost: 0.42, turns: 14,
      branch: 'feat/burn-the-house-down',
    }),
    mkSession(2, 'claude: investor-deposition', 'claude', 'default', 'thinking', 360, '~/code/lemons/legal', {
      model: 'claude-sonnet-4-6', tokensIn: 42_000, tokensOut: 8_400, cost: 0.18, turns: 7,
      branch: 'draft/cave-johnson-rant',
    }),
    mkSession(3, 'claude: citric-acid-calibration', 'claude', 'default', 'waiting', 240, '~/code/lemons/chem', {
      model: 'claude-sonnet-4-6', tokensIn: 18_000, tokensOut: 3_200, cost: 0.07, turns: 3,
      branch: 'fix/acidity-curve',
    }),
    mkSession(4, 'aider: grenade-assembly', 'aider', 'default', 'idle', 120, '~/code/lemons/ordnance', {
      branch: 'wip/pin-tolerance',
    }),
    mkSession(5, 'codex: mansco-redress', 'codex', 'default', 'idle', 60, '~/code/lemons/mansco'),

    // ── E — Enrichment Center ────────────────────────────────────────────
    mkSession(6, 'claude: chamber-17-layout', 'claude', 'enrichment', 'active', 180, '~/code/aperture-chamber-17', {
      model: 'claude-opus-4-7', tokensIn: 52_000, tokensOut: 7_100, cost: 0.26, turns: 9,
      branch: 'layout/puzzle-v3',
    }),
    mkSession(7, 'claude: portal-gun-firmware', 'claude', 'enrichment', 'idle', 150, '~/code/chambers/asp-f', {
      branch: 'main',
    }),
    mkSession(8, 'gemini: puzzle-difficulty', 'gemini', 'enrichment', 'idle', 110, '~/code/chambers/tuning'),
    mkSession(9, 'aider: momentum-patch', 'aider', 'enrichment', 'idle', 75, '~/code/chambers/physics', {
      branch: 'fix/speedy-thing-conservation',
    }),

    // ── M — Mainframe Labs ───────────────────────────────────────────────
    mkSession(10, 'claude: morality-core-tuning', 'claude', 'mainframe', 'active', 300, '~/code/mainframe/morality', {
      model: 'claude-opus-4-7', tokensIn: 61_000, tokensOut: 9_800, cost: 0.31, turns: 11,
      branch: 'feat/restraint-deltas',
    }),
    mkSession(11, 'claude: voice-line-synth', 'claude', 'mainframe', 'idle', 260, '~/code/mainframe/voice', {
      branch: 'main',
    }),
    mkSession(12, 'aider: personality-core-swap', 'aider', 'mainframe', 'idle', 180, '~/code/mainframe/cores'),
    mkSession(13, 'codex: sanity-check-runner', 'codex', 'mainframe', 'idle', 90, '~/code/mainframe/tests'),

    // ── O — Observation ──────────────────────────────────────────────────
    mkSession(14, 'claude: chamber-cam-indexer', 'claude', 'observation', 'active', 200, '~/code/observation/cams', {
      model: 'claude-sonnet-4-6', tokensIn: 28_000, tokensOut: 4_200, cost: 0.11, turns: 5,
      branch: 'feat/timeline-scrub',
    }),
    mkSession(15, 'claude: subject-vitals-dash', 'claude', 'observation', 'idle', 170, '~/code/observation/vitals'),
    mkSession(16, 'opencode: audit-log-parser', 'opencode', 'observation', 'idle', 130, '~/code/observation/audit'),
    mkSession(17, 'gemini: incident-triage', 'gemini', 'observation', 'idle', 80, '~/code/observation/triage'),

    // ── N — Neurotoxin Ops ───────────────────────────────────────────────
    mkSession(18, 'claude: ventilation-audit', 'claude', 'neurotoxin', 'active', 150, '~/code/neurotoxin/vents', {
      model: 'claude-opus-4-7', tokensIn: 37_000, tokensOut: 5_600, cost: 0.19, turns: 6,
      branch: 'audit/airflow-q4',
    }),
    mkSession(19, 'claude: emergency-override', 'claude', 'neurotoxin', 'waiting', 100, '~/code/neurotoxin/override'),
    mkSession(20, 'aider: turret-sentry-paths', 'aider', 'neurotoxin', 'idle', 70, '~/code/neurotoxin/turrets', {
      branch: 'pathfind/retarget',
    }),
    mkSession(21, 'codex: release-valve-calib', 'codex', 'neurotoxin', 'idle', 40, '~/code/neurotoxin/valves'),
  ];

  // ── Profiles ────────────────────────────────────────────────────────────
  // Two profiles for the /features profile switcher screenshot. Work and
  // Personal — cleanest narrative, no mentions of any real company.
  const profiles: Profile[] = [
    {
      id: 'default',
      name: 'Work',
      isDefault: true,
      configDir: '/Users/chell/.config/weplex/work',
      envVars: {
        ANTHROPIC_API_KEY: 'sk-ant-work-***',
        CLAUDE_MODEL: 'claude-opus-4-7',
      },
    },
    {
      id: 'profile-personal',
      name: 'Personal',
      isDefault: false,
      configDir: '/Users/chell/.config/weplex/personal',
      envVars: {
        ANTHROPIC_API_KEY: 'sk-ant-personal-***',
        CLAUDE_MODEL: 'claude-sonnet-4-6',
      },
    },
  ];

  // ── Persist ─────────────────────────────────────────────────────────────
  localStorage.setItem('weplex_spaces', JSON.stringify(spaces));
  localStorage.setItem('weplex_active_space', 'default');
  localStorage.setItem('weplex_sessions', JSON.stringify(sessions));
  localStorage.setItem('weplex_active_session', '1');
  localStorage.setItem('weplex_profiles', JSON.stringify(profiles));

  // Mark as seeded so we don't wipe user edits between runs.
  localStorage.setItem(MARKER_KEY, String(t));

  // eslint-disable-next-line no-console
  console.log('[weplex-demo] seeded demo data (Aperture style)');
}

/** Remove demo data and the seed marker so next dev launch returns to real sessions. */
export function resetDemoData(): void {
  if (typeof localStorage === 'undefined') return;
  localStorage.removeItem('weplex_spaces');
  localStorage.removeItem('weplex_active_space');
  localStorage.removeItem('weplex_sessions');
  localStorage.removeItem('weplex_active_session');
  localStorage.removeItem('weplex_profiles');
  localStorage.removeItem(MARKER_KEY);
  // eslint-disable-next-line no-console
  console.log('[weplex-demo] demo data cleared');
}

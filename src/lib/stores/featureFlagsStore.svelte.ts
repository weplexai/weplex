// Reactive feature flag store backed by PostHog.
//
// Values start at `false` for alpha (hide everything new by default), then
// PostHog's onFeatureFlags callback flips them on for users that match
// rollout targeting. Consumers read via `$derived` or direct access.

import { getFlag, readyFlags, type FeatureFlag } from '../services/analytics';

interface FeatureFlagsState {
  marketplace: boolean;
  commands: boolean;
  resources: boolean;
  loaded: boolean;
}

// Private $state — mutated by the bootstrap routine below
let state = $state<FeatureFlagsState>({
  marketplace: false,
  commands: false,
  resources: false,
  loaded: false,
});

export const featureFlags = {
  get marketplace() { return state.marketplace; },
  get commands() { return state.commands; },
  get resources() { return state.resources; },
  get loaded() { return state.loaded; },

  /** Refresh values from PostHog. Call once at startup after init. */
  async bootstrap(): Promise<void> {
    await readyFlags();
    state = {
      marketplace: getFlag('feature_marketplace', false),
      commands: getFlag('feature_commands', false),
      resources: getFlag('feature_resources', false),
      loaded: true,
    };
  },

  /** Manually override a flag (for dev / debug). */
  set(flag: keyof Omit<FeatureFlagsState, 'loaded'>, value: boolean): void {
    state = { ...state, [flag]: value };
  },
};

export type { FeatureFlag };

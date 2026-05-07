<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { Modal, Button, Select } from '../ui';
  import { ExternalLink, AlertTriangle, ShieldAlert, ShieldX } from 'lucide-svelte';
  import { profileStore } from '../../stores/profileStore.svelte';
  import { lockfileStore } from '../../stores/lockfileStore.svelte';
  import { guardStore } from '../../stores/guardStore.svelte';
  import { settingsStore } from '../../stores/settingsStore.svelte';
  import { schedule as scheduleCompile } from '../../utils/compileScheduler';
  import { federationService } from '../../services/federationService';
  import ResourceGuardBadge from './ResourceGuardBadge.svelte';
  import type {
    FederatedPackDetailDto,
    FederatedResourceDto,
  } from '../../types/federation';
  import type { MutationReport, ResourceKind } from '../../types/lockfile';

  interface Props {
    packId: string;
    /** Closes the modal — caller may also refresh its own state. */
    onclose: () => void;
  }

  let { packId, onclose }: Props = $props();

  // ─── Detail load state ──────────────────────────────────────────────

  let pack = $state<FederatedPackDetailDto | null>(null);
  let loading = $state(true);
  let loadError = $state<string | null>(null);

  // ─── Install state ──────────────────────────────────────────────────
  // Keyed by `FederatedResourceDto.path` (unique per pack). Values:
  //   `pending`  → fetching/installing
  //   `installed`→ landed in lockfile
  //   `error: <msg>` → failure for this resource only
  let installState = $state<Record<string, string>>({});
  let installing = $state(false);
  let installSummary = $state<{ ok: number; fail: number } | null>(null);

  // ─── Confirmation gate for `red` packs ──────────────────────────────

  let confirmRedNeeded = $state(false);
  let redConfirmed = $state(false);

  // ─── Profile picker ─────────────────────────────────────────────────

  let installableProfiles = $derived(
    profileStore.profiles.filter((p) => !!p.configDir),
  );
  let selectedConfigDir = $state<string>('');

  // ─── Per-resource preview expansion ─────────────────────────────────

  let expandedPreviews = $state<Record<string, boolean>>({});

  function togglePreview(path: string) {
    expandedPreviews = {
      ...expandedPreviews,
      [path]: !expandedPreviews[path],
    };
  }

  // ─── Lifecycle ──────────────────────────────────────────────────────

  onMount(async () => {
    // Pre-pick a default install target so the user doesn't have to.
    const def = profileStore.defaultProfile;
    selectedConfigDir =
      (def?.configDir ?? installableProfiles[0]?.configDir) || '';

    try {
      const detail = await federationService.getPack(packId);
      if (!detail) {
        loadError = 'Marketplace is offline. Try again later.';
      } else {
        pack = detail;
        confirmRedNeeded = detail.score.overall === 'red';
      }
    } catch (e) {
      loadError = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  });

  // ─── Helpers ────────────────────────────────────────────────────────

  /**
   * Federation kinds map 1:1 onto the lockfile's ResourceKind today.
   * Keep the mapping explicit so a future divergence (e.g. a new
   * federation kind that the lockfile doesn't yet support) becomes a
   * compile-time warning instead of a silent string cast.
   */
  function toLockfileKind(k: FederatedResourceDto['kind']): ResourceKind {
    return k;
  }

  /**
   * Compute the SHA-256 of a UTF-8 string using the Web Crypto API.
   * The federation service ships sha256 alongside every resource —
   * we recompute it locally and refuse to install on mismatch so a
   * compromised mirror cannot inject altered bodies.
   */
  async function sha256Utf8(input: string): Promise<string> {
    const enc = new TextEncoder().encode(input);
    const buf = await crypto.subtle.digest('SHA-256', enc);
    return Array.from(new Uint8Array(buf))
      .map((b) => b.toString(16).padStart(2, '0'))
      .join('');
  }

  /**
   * Fetch a raw text resource and bound the bytes received. We trust
   * neither the registry nor the host repo: a hostile server could
   * stream gigabytes; a runaway proxy could 500. The same 1 MiB cap
   * the Rust side enforces on disk applies here.
   */
  async function fetchRawText(url: string): Promise<string> {
    const res = await fetch(url, { signal: AbortSignal.timeout(30_000) });
    if (!res.ok) throw new Error(`fetch ${url} → HTTP ${res.status}`);
    const body = await res.text();
    if (body.length > 1024 * 1024) {
      throw new Error('resource exceeds 1 MiB cap');
    }
    return body;
  }

  // ─── Install flow ───────────────────────────────────────────────────

  async function installOne(
    resource: FederatedResourceDto,
    targetConfigDir: string,
    packIdValue: string,
    packCommitSha: string,
  ): Promise<void> {
    installState = { ...installState, [resource.path]: 'pending' };
    try {
      const content = await fetchRawText(resource.rawUrl);
      const actualSha = await sha256Utf8(content);
      if (actualSha !== resource.sha256) {
        throw new Error(
          `sha256 mismatch (expected ${resource.sha256.slice(0, 8)}…, got ${actualSha.slice(0, 8)}…)`,
        );
      }
      await invoke<MutationReport>('install_marketplace_package', {
        targetConfigDir,
        name: resource.name,
        content,
        sidecar: null,
        kind: toLockfileKind(resource.kind),
        pack: packIdValue,
        packCommitSha,
      });
      installState = { ...installState, [resource.path]: 'installed' };
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      installState = { ...installState, [resource.path]: `error: ${msg}` };
    }
  }

  async function installAll() {
    if (!pack || installing) return;
    if (!selectedConfigDir) {
      loadError = 'Choose a profile to install into.';
      return;
    }
    if (confirmRedNeeded && !redConfirmed) {
      // The button is disabled in that state; this is just defence.
      return;
    }
    installing = true;
    installSummary = null;
    // Reset state so a re-install attempt starts clean.
    installState = {};

    // Sequential install — each invoke takes a flock on the lockfile,
    // so parallelising would just contend. Order is also useful for
    // user feedback (the rows light up top-to-bottom).
    for (const r of pack.resources) {
      await installOne(r, selectedConfigDir, pack.id, pack.commitSha);
    }

    // Refresh dependent stores once at the end. Best-effort: any
    // failure here doesn't affect what's already on disk + in the
    // lockfile, but the Hub views won't reflect the new state until
    // the next refresh.
    try {
      await lockfileStore.refresh(selectedConfigDir);
      await guardStore.refresh(
        selectedConfigDir,
        settingsStore.settings.agentshieldDeepScan,
      );
      await scheduleCompile(selectedConfigDir, {
        deepScan: settingsStore.settings.agentshieldDeepScan,
      });
    } catch (e) {
      // Stores carry their own error state; nothing to surface here.
      console.warn('[federation] post-install refresh:', e);
    }

    const ok = Object.values(installState).filter((s) => s === 'installed').length;
    const fail = Object.values(installState).filter((s) => s.startsWith('error:')).length;
    installSummary = { ok, fail };
    installing = false;
  }

  // ─── Per-row helpers for the template ───────────────────────────────

  function findingTone(
    severity: 'info' | 'warn' | 'block',
  ): 'info' | 'warn' | 'block' {
    return severity;
  }

  function statusFor(path: string): 'idle' | 'pending' | 'installed' | 'error' {
    const s = installState[path];
    if (!s) return 'idle';
    if (s === 'pending') return 'pending';
    if (s === 'installed') return 'installed';
    return 'error';
  }

  function errorMessage(path: string): string | null {
    const s = installState[path];
    if (!s || !s.startsWith('error:')) return null;
    return s.slice('error: '.length);
  }
</script>

<Modal {onclose} label="Federated pack details" class="fed-detail-modal">
  <div class="fed-detail">
    {#if loading}
      <div class="fed-loading">Loading pack details…</div>
    {:else if loadError}
      <div class="fed-error" role="alert">
        <AlertTriangle size={14} />
        {loadError}
      </div>
      <div class="fed-actions">
        <Button variant="secondary" onclick={onclose}>Close</Button>
      </div>
    {:else if pack}
      <header class="fed-detail-header">
        <div class="fed-detail-title">
          <h2>{pack.name}</h2>
          <ResourceGuardBadge verdict={pack.score.overall} size="md" />
        </div>
        <p class="fed-detail-desc">{pack.description}</p>
        <div class="fed-detail-meta">
          <a
            class="fed-repo-link"
            href={pack.repoUrl}
            target="_blank"
            rel="noopener noreferrer"
          >
            <ExternalLink size={11} />
            <span>{pack.id}</span>
          </a>
          <span class="meta-pill">★ {pack.stars}</span>
          <span class="meta-pill">{pack.resourceCount} resources</span>
          <span class="meta-pill" title={pack.commitSha}>
            {pack.defaultBranch}@{pack.commitSha.slice(0, 7)}
          </span>
          <span class="meta-pill" title={pack.lastIndexedAt}>
            indexed {new Date(pack.lastIndexedAt).toLocaleDateString()}
          </span>
        </div>
      </header>

      <!-- Profile picker -->
      <div class="fed-profile-row">
        <label for="fed-profile-select">Install to:</label>
        {#if installableProfiles.length === 0}
          <span class="fed-warning">No profiles with config directory configured.</span>
        {:else}
          <Select
            value={selectedConfigDir}
            onchange={(v) => (selectedConfigDir = v)}
            options={installableProfiles.map((p) => ({
              value: p.configDir!,
              label: p.name,
            }))}
          />
        {/if}
      </div>

      <!-- Red-pack confirmation gate -->
      {#if confirmRedNeeded}
        <div class="fed-red-confirm" role="alert">
          <ShieldX size={14} />
          <div class="fed-red-text">
            <strong>This pack scored RED in AgentShield.</strong>
            <span>
              Review the findings on each resource before installing.
              You should only proceed if you understand the risk.
            </span>
            <label class="fed-red-check">
              <input type="checkbox" bind:checked={redConfirmed} />
              I understand and want to install anyway.
            </label>
          </div>
        </div>
      {/if}

      <!-- Resource list -->
      <ul class="fed-resource-list">
        {#each pack.resources as r (r.path)}
          {@const status = statusFor(r.path)}
          {@const err = errorMessage(r.path)}
          {@const isOpen = expandedPreviews[r.path] === true}
          <li class="fed-resource" data-status={status}>
            <div class="fed-resource-head">
              <div class="fed-resource-titleline">
                <span class="fed-kind-chip kind-{r.kind}">{r.kind}</span>
                <span class="fed-resource-name">{r.name}</span>
                <ResourceGuardBadge verdict={r.agentshield.score} size="sm" />
              </div>
              <div class="fed-resource-meta">
                <span title="size in bytes">{r.size} B</span>
                <span class="fed-sha" title={r.sha256}>{r.sha256.slice(0, 8)}</span>
              </div>
            </div>
            <div class="fed-resource-path">{r.path}</div>

            <button
              type="button"
              class="fed-preview-toggle"
              onclick={() => togglePreview(r.path)}
            >
              {isOpen ? 'Hide preview' : 'Show preview'}
            </button>
            {#if isOpen}
              <pre class="fed-preview-body">{r.preview}</pre>
            {/if}

            {#if r.agentshield.findings.length > 0}
              <ul class="fed-findings">
                {#each r.agentshield.findings as f (f.fingerprint)}
                  <li class="fed-finding tone-{findingTone(f.severity)}">
                    {#if f.severity === 'block'}
                      <ShieldX size={11} />
                    {:else}
                      <ShieldAlert size={11} />
                    {/if}
                    <div class="fed-finding-text">
                      <strong>{f.message}</strong>
                      <span class="fed-finding-rule">{f.ruleId}</span>
                      {#if f.location}
                        <span class="fed-finding-loc">@ {f.location}</span>
                      {/if}
                    </div>
                  </li>
                {/each}
              </ul>
            {/if}

            <div class="fed-resource-status" data-status={status}>
              {#if status === 'pending'}
                <span class="fed-status-pending">Installing…</span>
              {:else if status === 'installed'}
                <span class="fed-status-ok">Installed ✓</span>
              {:else if status === 'error' && err}
                <span class="fed-status-err">{err}</span>
              {/if}
            </div>
          </li>
        {/each}
      </ul>

      <!-- Footer actions -->
      <div class="fed-actions">
        {#if installSummary}
          <span class="fed-summary">
            {installSummary.ok}/{pack.resources.length} installed
            {#if installSummary.fail > 0}
              · {installSummary.fail} failed
            {/if}
          </span>
        {/if}
        <Button variant="secondary" onclick={onclose} disabled={installing}>
          {installing ? 'Installing…' : 'Close'}
        </Button>
        <Button
          variant="primary"
          onclick={installAll}
          disabled={installing
            || !selectedConfigDir
            || (confirmRedNeeded && !redConfirmed)
            || installableProfiles.length === 0}
        >
          {installing ? 'Installing…' : 'Install all'}
        </Button>
      </div>
    {/if}
  </div>
</Modal>

<style>
  :global(.fed-detail-modal) {
    width: 720px;
    max-width: 92vw;
    max-height: 85vh;
    overflow-y: auto;
  }

  .fed-detail {
    padding: 18px 22px 16px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .fed-loading,
  .fed-error {
    padding: 24px;
    text-align: center;
    color: var(--weplex-text-muted);
    font-size: var(--weplex-text-sm);
  }
  .fed-error {
    color: var(--weplex-error);
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
  }

  .fed-detail-header {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding-bottom: 10px;
    border-bottom: 1px solid var(--weplex-border);
  }
  .fed-detail-title {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  .fed-detail-title h2 {
    margin: 0;
    font-size: var(--weplex-text-md);
    font-weight: 600;
  }
  .fed-detail-desc {
    margin: 0;
    font-size: var(--weplex-text-sm);
    color: var(--weplex-text-secondary);
    line-height: 1.4;
  }
  .fed-detail-meta {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
    font-size: 11px;
    color: var(--weplex-text-muted);
  }
  .fed-repo-link {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    color: var(--weplex-text-muted);
    text-decoration: none;
  }
  .fed-repo-link:hover {
    color: var(--weplex-accent);
  }
  .meta-pill {
    padding: 1px 6px;
    background: var(--weplex-surface-hover);
    border-radius: 3px;
  }

  .fed-profile-row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
  }
  .fed-profile-row label {
    font-weight: 600;
  }
  .fed-warning {
    color: var(--weplex-warning, #f59e0b);
  }

  .fed-red-confirm {
    display: flex;
    gap: 10px;
    padding: 10px 12px;
    background: color-mix(in srgb, var(--weplex-error) 8%, transparent);
    border: 1px solid color-mix(in srgb, var(--weplex-error) 35%, transparent);
    border-radius: var(--weplex-radius-sm);
    color: var(--weplex-error);
    align-items: flex-start;
  }
  .fed-red-text {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text);
  }
  .fed-red-text strong {
    color: var(--weplex-error);
  }
  .fed-red-check {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 4px;
    cursor: pointer;
  }

  .fed-resource-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .fed-resource {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 10px 12px;
    background: var(--weplex-surface);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-sm);
  }
  .fed-resource[data-status='installed'] {
    border-color: color-mix(in srgb, var(--weplex-active) 40%, transparent);
  }
  .fed-resource[data-status='error'] {
    border-color: color-mix(in srgb, var(--weplex-error) 40%, transparent);
  }

  .fed-resource-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }
  .fed-resource-titleline {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }
  .fed-resource-name {
    font-size: var(--weplex-text-sm);
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .fed-resource-meta {
    display: flex;
    gap: 8px;
    font-size: 11px;
    color: var(--weplex-text-muted);
    font-family: var(--weplex-font-mono);
    flex-shrink: 0;
  }
  .fed-sha {
    font-family: var(--weplex-font-mono);
  }

  .fed-resource-path {
    font-size: 11px;
    color: var(--weplex-text-muted);
    font-family: var(--weplex-font-mono);
  }

  .fed-kind-chip {
    font-size: 9px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    padding: 1px 5px;
    border-radius: 3px;
    color: var(--weplex-text-muted);
    background: var(--weplex-surface-hover);
  }
  .fed-kind-chip.kind-agent {
    color: var(--weplex-accent);
    background: color-mix(in srgb, var(--weplex-accent) 12%, transparent);
  }
  .fed-kind-chip.kind-rule {
    color: var(--weplex-warning, #f59e0b);
    background: color-mix(in srgb, var(--weplex-warning, #f59e0b) 12%, transparent);
  }
  .fed-kind-chip.kind-skill {
    color: var(--weplex-active);
    background: color-mix(in srgb, var(--weplex-active) 12%, transparent);
  }

  .fed-preview-toggle {
    align-self: flex-start;
    padding: 0;
    background: none;
    border: none;
    color: var(--weplex-text-muted);
    font-size: 11px;
    cursor: pointer;
  }
  .fed-preview-toggle:hover {
    color: var(--weplex-accent);
  }
  .fed-preview-body {
    margin: 4px 0 0;
    padding: 8px 10px;
    background: var(--weplex-bg);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-sm);
    font-family: var(--weplex-font-mono);
    font-size: 11px;
    color: var(--weplex-text-secondary);
    white-space: pre-wrap;
    max-height: 220px;
    overflow-y: auto;
  }

  .fed-findings {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .fed-finding {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    padding: 6px 8px;
    border-radius: var(--weplex-radius-sm);
    font-size: 11px;
    border: 1px solid var(--weplex-border);
  }
  .fed-finding.tone-warn {
    color: var(--weplex-warning, #f59e0b);
    background: color-mix(in srgb, var(--weplex-warning, #f59e0b) 8%, transparent);
    border-color: color-mix(in srgb, var(--weplex-warning, #f59e0b) 25%, transparent);
  }
  .fed-finding.tone-block {
    color: var(--weplex-error);
    background: color-mix(in srgb, var(--weplex-error) 10%, transparent);
    border-color: color-mix(in srgb, var(--weplex-error) 35%, transparent);
  }
  .fed-finding.tone-info {
    color: var(--weplex-text-muted);
    background: var(--weplex-surface);
  }
  .fed-finding-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    color: var(--weplex-text);
  }
  .fed-finding-rule {
    font-family: var(--weplex-font-mono);
    color: var(--weplex-text-muted);
  }
  .fed-finding-loc {
    color: var(--weplex-text-muted);
  }

  .fed-resource-status {
    font-size: 11px;
  }
  .fed-status-pending {
    color: var(--weplex-text-muted);
  }
  .fed-status-ok {
    color: var(--weplex-active);
  }
  .fed-status-err {
    color: var(--weplex-error);
  }

  .fed-actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 6px;
    padding-top: 10px;
    border-top: 1px solid var(--weplex-border);
  }
  .fed-summary {
    margin-right: auto;
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
  }
</style>

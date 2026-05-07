<script lang="ts">
  import { Star, ExternalLink } from 'lucide-svelte';
  import ResourceGuardBadge from './ResourceGuardBadge.svelte';
  import type { FederatedPackSummaryDto } from '../../types/federation';

  interface Props {
    pack: FederatedPackSummaryDto;
    /** Open the detail modal. */
    onview: (packId: string) => void;
  }

  let { pack, onview }: Props = $props();

  /**
   * "2 days ago" / "Mar 12" — keep it terse so the meta row stays
   * single-line on narrow viewports. The exact ISO is on the title
   * attribute for users who hover.
   */
  function formatRelative(iso: string): string {
    const then = new Date(iso).getTime();
    if (!Number.isFinite(then)) return iso;
    const diffMs = Date.now() - then;
    const day = 86_400_000;
    if (diffMs < day) return 'today';
    if (diffMs < 2 * day) return 'yesterday';
    if (diffMs < 30 * day) return `${Math.floor(diffMs / day)}d ago`;
    return new Date(iso).toLocaleDateString(undefined, {
      month: 'short',
      day: 'numeric',
    });
  }

  function formatStars(n: number): string {
    if (n >= 1000) return `${(n / 1000).toFixed(1)}K`;
    return String(n);
  }

  function openRepo(e: MouseEvent) {
    // Plain anchor — Tauri's webview opens external URLs via the OS shell
    // as long as `target="_blank"` and the link isn't intercepted.
    e.stopPropagation();
  }

  function onCardClick() {
    onview(pack.id);
  }

  function onCardKey(e: KeyboardEvent) {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      onview(pack.id);
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  class="fed-card"
  role="button"
  tabindex="0"
  onclick={onCardClick}
  onkeydown={onCardKey}
>
  <div class="fed-card-header">
    <h3 class="fed-card-name" title={pack.id}>{pack.name}</h3>
    <ResourceGuardBadge verdict={pack.score.overall} size="md" />
  </div>

  <p class="fed-card-desc">{pack.description}</p>

  <div class="fed-card-kinds">
    {#each pack.resourceKinds as kind (kind)}
      <span class="fed-kind-chip kind-{kind}">{kind}</span>
    {/each}
  </div>

  <div class="fed-card-meta">
    <span class="meta-stars" title={`${pack.stars} stars`}>
      <Star size={11} />
      {formatStars(pack.stars)}
    </span>
    <span class="meta-resources">{pack.resourceCount} resources</span>
    <span class="meta-indexed" title={pack.lastIndexedAt}>
      indexed {formatRelative(pack.lastIndexedAt)}
    </span>
  </div>

  <div class="fed-card-footer">
    <a
      class="fed-repo-link"
      href={pack.repoUrl}
      target="_blank"
      rel="noopener noreferrer"
      onclick={openRepo}
    >
      <ExternalLink size={11} />
      <span>{pack.id}</span>
    </a>
    <button
      type="button"
      class="fed-view-btn"
      onclick={(e) => {
        e.stopPropagation();
        onview(pack.id);
      }}
    >
      View details
    </button>
  </div>
</div>

<style>
  .fed-card {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    background: var(--weplex-surface);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-md);
    cursor: pointer;
    transition: border-color var(--weplex-duration-fast);
  }
  .fed-card:hover,
  .fed-card:focus-visible {
    border-color: var(--weplex-accent);
    outline: none;
  }

  .fed-card-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .fed-card-name {
    font-size: var(--weplex-text-sm);
    font-weight: 600;
    margin: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .fed-card-desc {
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
    margin: 0;
    line-height: 1.4;
    /* Two-line clamp keeps every card the same height. */
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .fed-card-kinds {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
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
  .fed-kind-chip.kind-command {
    color: var(--weplex-text);
    background: var(--weplex-surface-hover);
  }

  .fed-card-meta {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    font-size: 11px;
    color: var(--weplex-text-muted);
  }
  .meta-stars {
    display: inline-flex;
    align-items: center;
    gap: 3px;
  }

  .fed-card-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    margin-top: auto;
  }
  .fed-repo-link {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-size: 11px;
    color: var(--weplex-text-muted);
    text-decoration: none;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .fed-repo-link:hover {
    color: var(--weplex-accent);
  }

  .fed-view-btn {
    padding: 4px 10px;
    font-size: var(--weplex-text-xs);
    font-weight: 500;
    background: var(--weplex-surface-hover);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-sm);
    color: var(--weplex-text);
    cursor: pointer;
    flex-shrink: 0;
  }
  .fed-view-btn:hover {
    border-color: var(--weplex-accent);
    color: var(--weplex-accent);
  }
</style>

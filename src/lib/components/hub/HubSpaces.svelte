<script lang="ts">
  import { spaceStore } from '../../stores/spaceStore';
  import { profileStore } from '../../stores/profileStore';
  import { uiStore } from '../../stores/uiStore';
  import { Plus, Trash2, FolderOpen, User } from 'lucide-svelte';
  import { SPACE_COLORS } from '../../types';
  import type { Space } from '../../types';

  function createSpace() {
    const idx = spaceStore.spaces.length;
    const color = SPACE_COLORS[idx % SPACE_COLORS.length];
    spaceStore.create(`Space ${idx + 1}`, color);
  }

  function editSpace(id: string) {
    spaceStore.editingSpaceId = id;
    uiStore.openOverlay('space-modal');
  }
</script>

<div class="hub-spaces">
  <div class="spaces-header">
    <h2 class="spaces-title">Spaces</h2>
  </div>

  <div class="spaces-grid">
    {#each spaceStore.spaces as space (space.id)}
      {@const profile = space.profileId ? profileStore.getById(space.profileId) : null}

      <!-- svelte-ignore a11y_no_static_element_interactions a11y_click_events_have_key_events -->
      <div
        class="space-card"
        style="--card-color: {space.color}"
        onclick={() => editSpace(space.id)}
      >
        <div class="card-bar"></div>
        <span class="card-watermark">{space.name[0].toUpperCase()}</span>

        <div class="card-body">
          <div class="card-top">
            <span class="card-name">{space.name}</span>
            {#if space.id === 'default'}
              <span class="card-badge">default</span>
            {/if}
            {#if space.type === 'team'}
              <span class="card-badge team">team</span>
            {/if}
            {#if space.shared && space.type !== 'team'}
              <span class="card-badge shared">shared</span>
            {/if}
          </div>

          <div class="card-details">
            <div class="card-detail">
              <FolderOpen size={11} />
              {#if space.directory}
                <span class="mono">{space.directory}</span>
              {:else}
                <span class="muted">No directory</span>
              {/if}
            </div>
            <div class="card-detail">
              <User size={11} />
              <span>{profile ? profile.name : 'Default'}</span>
            </div>
          </div>
        </div>

        {#if space.id !== 'default'}
          <button
            class="card-delete"
            title="Delete space"
            onclick={(e) => { e.stopPropagation(); spaceStore.remove(space.id); }}
          >
            <Trash2 size={13} />
          </button>
        {/if}
      </div>
    {/each}

    <button class="space-card add-card" onclick={createSpace}>
      <Plus size={28} strokeWidth={1.2} />
      <span>New Space</span>
    </button>
  </div>
</div>

<style>
  .hub-spaces {
    width: 100%;
    height: 100%;
    padding: 32px 40px;
    overflow-y: auto;
    background: var(--weplex-bg);
  }

  .spaces-header {
    margin-bottom: 24px;
  }

  .spaces-title {
    font-size: var(--weplex-text-lg);
    font-weight: 600;
    margin: 0;
  }

  .spaces-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
    gap: 16px;
  }

  .space-card {
    position: relative;
    display: flex;
    flex-direction: column;
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-lg);
    background: var(--weplex-surface);
    overflow: hidden;
    cursor: pointer;
    transition: all 0.2s;
    text-align: left;
    padding: 0;
  }

  .space-card:hover {
    border-color: color-mix(in srgb, var(--card-color, var(--weplex-text-muted)) 50%, transparent);
    box-shadow: 0 4px 20px rgba(0, 0, 0, 0.25);
    transform: translateY(-2px);
  }

  .card-watermark {
    position: absolute;
    bottom: -8px;
    right: 12px;
    font-size: 96px;
    font-weight: 800;
    line-height: 1;
    color: var(--card-color, var(--weplex-text-muted));
    opacity: 0.06;
    pointer-events: none;
    user-select: none;
    transition: opacity 0.2s;
  }

  .space-card:hover .card-watermark {
    opacity: 0.1;
  }

  .card-bar {
    height: 4px;
    background: var(--card-color, var(--weplex-text-muted));
  }

  .card-body {
    padding: 18px 20px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    flex: 1;
    min-height: 130px;
    background: linear-gradient(
      135deg,
      color-mix(in srgb, var(--card-color, transparent) 6%, transparent) 0%,
      transparent 60%
    );
  }

  .card-top {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .card-name {
    font-size: var(--weplex-text-md);
    font-weight: 600;
    color: var(--weplex-text);
  }

  .card-badge {
    font-size: 10px;
    padding: 1px 6px;
    border-radius: var(--weplex-radius-full);
    background: rgba(255, 255, 255, 0.06);
    color: var(--weplex-text-muted);
    display: flex;
    align-items: center;
    gap: 3px;
  }

  .card-badge.team {
    background: rgba(59, 130, 246, 0.12);
    color: #60a5fa;
  }

  .card-badge.shared {
    background: rgba(16, 185, 129, 0.12);
    color: #34d399;
  }

  .card-details {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .card-detail {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-secondary);
  }

  .card-detail .muted {
    color: var(--weplex-text-muted);
    font-style: italic;
  }

  .card-detail .mono {
    font-family: var(--weplex-font-mono);
    opacity: 0.8;
  }

  .card-delete {
    position: absolute;
    top: 12px;
    right: 10px;
    width: 24px;
    height: 24px;
    display: flex;
    align-items: center;
    justify-content: center;
    border: none;
    border-radius: var(--weplex-radius-sm);
    background: var(--weplex-surface);
    color: var(--weplex-text-muted);
    cursor: pointer;
    opacity: 0;
    transition: all 0.15s;
  }

  .space-card:hover .card-delete {
    opacity: 1;
  }

  .card-delete:hover {
    background: rgba(239, 68, 68, 0.1);
    color: var(--weplex-error);
  }

  /* Add card */
  .add-card {
    border-style: dashed;
    border-color: var(--weplex-border);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    min-height: 158px;
    color: var(--weplex-text-muted);
    font-size: var(--weplex-text-sm);
    background: transparent;
  }

  .add-card:hover {
    border-color: var(--weplex-accent);
    color: var(--weplex-accent);
    transform: none;
    box-shadow: none;
  }
</style>

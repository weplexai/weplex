<script lang="ts">
  import { teamStore } from '../../stores/teamStore.svelte';
  import { authStore } from '../../stores/authStore.svelte';
  import {
    Copy,
    RefreshCw,
    UserMinus,
    LogOut,
    Users,
    Plus,
    Shield,
    Trash2,
    Check,
    Crown,
  } from 'lucide-svelte';
  import type { TeamMember, TeamInfo } from '../../types';

  // Create team form
  let createName = $state('');
  let createLoading = $state(false);
  let createError = $state<string | null>(null);
  let showCreateForm = $state(false);

  // Join team form
  let joinCode = $state('');
  let joinLoading = $state(false);
  let joinError = $state<string | null>(null);
  let showJoinForm = $state(false);

  // General
  let copyFeedback = $state<string | null>(null); // teamId that just copied
  let regenLoading = $state<string | null>(null); // teamId being regenerated
  let removingMemberId = $state<string | null>(null);
  let leaveLoading = $state<string | null>(null); // teamId being left
  let deleteLoading = $state<string | null>(null); // teamId being deleted

  // Expanded team detail
  let expandedTeamId = $state<string | null>(null);

  let teams = $derived(teamStore.teams);
  let activeTeamId = $derived(teamStore.activeTeamId);
  let currentUserId = $derived(authStore.user?.id ?? null);
  let hasTeams = $derived(teams.length > 0);

  // Determine if current user is owner of a specific team
  function isOwner(team: TeamInfo): boolean {
    return team.ownerId === currentUserId;
  }

  // Find current user's membership in a team
  function currentMember(team: TeamInfo): TeamMember | undefined {
    return team.members.find((m) => m.userId === currentUserId);
  }

  function toggleExpand(teamId: string) {
    expandedTeamId = expandedTeamId === teamId ? null : teamId;
  }

  async function handleCreate() {
    if (!createName.trim()) return;
    createLoading = true;
    createError = null;
    try {
      await teamStore.createTeam(createName.trim());
      createName = '';
      showCreateForm = false;
    } catch (e: unknown) {
      createError = e instanceof Error ? e.message : String(e);
    } finally {
      createLoading = false;
    }
  }

  async function handleJoin() {
    if (!joinCode.trim()) return;
    joinLoading = true;
    joinError = null;
    try {
      await teamStore.joinTeam(joinCode.trim());
      joinCode = '';
      showJoinForm = false;
    } catch (e: unknown) {
      joinError = e instanceof Error ? e.message : String(e);
    } finally {
      joinLoading = false;
    }
  }

  async function copyInviteCode(team: TeamInfo) {
    try {
      await navigator.clipboard.writeText(team.inviteCode);
      copyFeedback = team.id;
      setTimeout(() => (copyFeedback = null), 1500);
    } catch {}
  }

  async function handleRegenCode(teamId: string) {
    regenLoading = teamId;
    try {
      await teamStore.regenerateCode(teamId);
    } catch (e: unknown) {
      console.error('[Weplex] Regenerate invite code failed:', e);
    } finally {
      regenLoading = null;
    }
  }

  async function handleRemoveMember(teamId: string, memberId: string) {
    removingMemberId = memberId;
    try {
      await teamStore.removeMember(teamId, memberId);
    } catch (e: unknown) {
      console.error('[Weplex] Remove member failed:', e);
    } finally {
      removingMemberId = null;
    }
  }

  async function handleLeave(teamId: string) {
    leaveLoading = teamId;
    try {
      await teamStore.leaveTeam(teamId);
      if (expandedTeamId === teamId) expandedTeamId = null;
    } catch (e: unknown) {
      console.error('[Weplex] Leave team failed:', e);
    } finally {
      leaveLoading = null;
    }
  }

  async function handleDelete(teamId: string) {
    deleteLoading = teamId;
    try {
      await teamStore.deleteTeam(teamId);
      if (expandedTeamId === teamId) expandedTeamId = null;
    } catch (e: unknown) {
      console.error('[Weplex] Delete team failed:', e);
    } finally {
      deleteLoading = null;
    }
  }

  function handleSetActive(teamId: string) {
    teamStore.setActiveTeam(teamId);
  }

  function memberInitial(member: TeamMember): string {
    const name = member.displayName || member.email;
    return name[0]?.toUpperCase() || '?';
  }

  function memberLabel(member: TeamMember): string {
    return member.displayName || member.email;
  }

  function formatExpiresAt(isoDate: string): string {
    const now = Date.now();
    const expires = new Date(isoDate).getTime();
    const diffMs = expires - now;

    if (diffMs <= 0) return 'Expired';

    const minutes = Math.floor(diffMs / 60_000);
    const hours = Math.floor(minutes / 60);
    const days = Math.floor(hours / 24);

    if (days > 0) return `Expires in ${days}d ${hours % 24}h`;
    if (hours > 0) return `Expires in ${hours}h ${minutes % 60}m`;
    return `Expires in ${minutes}m`;
  }

  function handleCreateKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') handleCreate();
  }

  function handleJoinKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') handleJoin();
  }
</script>

{#if !authStore.isAuthenticated}
  <h3 class="section-title">Teams</h3>
  <p class="section-desc">Sign in to create or join teams for collaboration.</p>

{:else}
  <h3 class="section-title">Teams</h3>

  <!-- Action buttons — always visible -->
  <div class="team-actions">
    <button
      class="btn-sm"
      class:save={showCreateForm}
      onclick={() => { showCreateForm = !showCreateForm; showJoinForm = false; }}
    >
      <Plus size={12} /> Create
    </button>
    <button
      class="btn-sm"
      class:save={showJoinForm}
      onclick={() => { showJoinForm = !showJoinForm; showCreateForm = false; }}
    >
      <Users size={12} /> Join
    </button>
  </div>

  <!-- Create form (toggled) -->
  {#if showCreateForm}
    <div class="team-form-group">
      <div class="team-form-row">
        <input
          class="setting-input"
          type="text"
          placeholder="Team name"
          bind:value={createName}
          onkeydown={handleCreateKeydown}
        />
        <button
          class="btn-sm save"
          onclick={handleCreate}
          disabled={createLoading || !createName.trim()}
        >
          {#if createLoading}Creating...{:else}Create{/if}
        </button>
      </div>
      {#if createError}
        <span class="form-error">{createError}</span>
      {/if}
    </div>
  {/if}

  <!-- Join form (toggled) -->
  {#if showJoinForm}
    <div class="team-form-group">
      <div class="team-form-row">
        <input
          class="setting-input mono"
          type="text"
          placeholder="Invite code"
          bind:value={joinCode}
          onkeydown={handleJoinKeydown}
        />
        <button
          class="btn-sm save"
          onclick={handleJoin}
          disabled={joinLoading || !joinCode.trim()}
        >
          {#if joinLoading}Joining...{:else}Join{/if}
        </button>
      </div>
      {#if joinError}
        <span class="form-error">{joinError}</span>
      {/if}
    </div>
  {/if}

  {#if !hasTeams && !showCreateForm && !showJoinForm}
    <p class="section-desc">Create a team or join an existing one to run collaboration.</p>
  {/if}

  <!-- Team list -->
  {#if hasTeams}
    <div class="team-list">
      {#each teams as team (team.id)}
        <div class="team-item" class:expanded={expandedTeamId === team.id}>
          <!-- Team header row -->
          <button class="team-header" onclick={() => toggleExpand(team.id)}>
            <span class="team-expand-icon">{expandedTeamId === team.id ? '▾' : '▸'}</span>
            <span class="team-name">{team.name}</span>
            <span class="team-member-count">{team.members.length}</span>
            {#if team.id === activeTeamId}
              <span class="active-dot" title="Active team"></span>
            {/if}
          </button>

          <!-- Expanded team detail -->
          {#if expandedTeamId === team.id}
            <div class="team-detail">
              <!-- Invite code -->
              <div class="invite-section">
                <span class="form-label">Invite Code</span>
                <div class="invite-row">
                  <code class="invite-code">{team.inviteCode}</code>
                  <button class="btn-icon" title="Copy" onclick={() => copyInviteCode(team)}>
                    <Copy size={13} />
                    {#if copyFeedback === team.id}<span class="copy-ok">Copied</span>{/if}
                  </button>
                  {#if isOwner(team)}
                    <button
                      class="btn-icon"
                      title="Regenerate code"
                      onclick={() => handleRegenCode(team.id)}
                      disabled={regenLoading === team.id}
                    >
                      <RefreshCw size={13} class={regenLoading === team.id ? 'spin' : ''} />
                    </button>
                  {/if}
                </div>
                {#if team.inviteCodeExpiresAt}
                  <span class="expires-text">{formatExpiresAt(team.inviteCodeExpiresAt)}</span>
                {/if}
              </div>

              <!-- Members list -->
              <div class="members-section">
                <span class="form-label">Members</span>
                {#each team.members as member (member.id)}
                  <div class="member-row">
                    <span class="member-avatar">{memberInitial(member)}</span>
                    <div class="member-info">
                      <span class="member-name">
                        {memberLabel(member)}
                        {#if member.userId === currentUserId}<span class="you-tag">you</span>{/if}
                      </span>
                      {#if member.displayName}
                        <span class="member-email">{member.email}</span>
                      {/if}
                    </div>
                    <span class="role-badge" class:owner={member.role === 'owner'}>
                      {#if member.role === 'owner'}<Crown size={10} />{/if}
                      {member.role}
                    </span>
                    {#if isOwner(team) && member.userId !== currentUserId}
                      <button
                        class="btn-icon danger"
                        title="Remove member"
                        onclick={() => handleRemoveMember(team.id, member.userId)}
                        disabled={removingMemberId === member.id}
                      >
                        <UserMinus size={13} />
                      </button>
                    {/if}
                  </div>
                {/each}
              </div>

              <!-- Team actions -->
              <div class="team-detail-actions">
                {#if team.id !== activeTeamId}
                  <button class="btn-sm" onclick={() => handleSetActive(team.id)}>
                    <Check size={12} /> Set Active
                  </button>
                {/if}
                <button
                  class="btn-sm delete"
                  onclick={() => handleLeave(team.id)}
                  disabled={leaveLoading === team.id}
                >
                  <LogOut size={12} />
                  {#if leaveLoading === team.id}Leaving...{:else}Leave{/if}
                </button>
                {#if isOwner(team)}
                  <button
                    class="btn-sm delete"
                    onclick={() => handleDelete(team.id)}
                    disabled={deleteLoading === team.id}
                  >
                    <Trash2 size={12} />
                    {#if deleteLoading === team.id}Deleting...{:else}Delete{/if}
                  </button>
                {/if}
              </div>
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
{/if}

<style>
  .section-title {
    font-size: var(--weplex-text-lg);
    font-weight: 600;
    margin-bottom: 16px;
  }

  .section-desc {
    font-size: var(--weplex-text-sm);
    color: var(--weplex-text-muted);
    margin-bottom: 16px;
    line-height: 1.4;
  }

  .team-actions {
    display: flex;
    gap: 6px;
    margin-bottom: 12px;
  }

  .team-form-group {
    margin-bottom: 12px;
  }

  .form-label {
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    display: block;
    margin-bottom: 6px;
  }

  .team-form-row {
    display: flex;
    gap: 6px;
    align-items: center;
  }

  .setting-input {
    padding: 5px 8px;
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-md);
    background: var(--weplex-bg);
    color: var(--weplex-text);
    font-size: var(--weplex-text-sm);
    flex: 1;
    outline: none;
  }

  .setting-input:focus {
    border-color: var(--weplex-accent);
  }

  .setting-input.mono {
    font-family: var(--weplex-font-mono);
    font-size: var(--weplex-text-xs);
  }

  .form-error {
    font-size: var(--weplex-text-xs);
    color: var(--weplex-error);
    margin-top: 4px;
    display: block;
  }

  .btn-sm {
    padding: 4px 10px;
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-sm);
    background: transparent;
    color: var(--weplex-text-secondary);
    font-size: var(--weplex-text-xs);
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    white-space: nowrap;
  }

  .btn-sm:hover {
    background: var(--weplex-surface-hover);
  }

  .btn-sm:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .btn-sm.save {
    background: var(--weplex-accent);
    border-color: var(--weplex-accent);
    color: white;
  }

  .btn-sm.save:hover {
    opacity: 0.9;
  }

  .btn-sm.delete {
    border-color: var(--weplex-error);
    color: var(--weplex-error);
  }

  .btn-sm.delete:hover {
    background: rgba(239, 68, 68, 0.1);
  }

  /* Team list */
  .team-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-bottom: 12px;
  }

  .team-item {
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-md);
    overflow: hidden;
  }

  .team-item.expanded {
    border-color: var(--weplex-accent);
  }

  .team-header {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 8px 10px;
    border: none;
    background: transparent;
    color: var(--weplex-text);
    font-size: var(--weplex-text-sm);
    cursor: pointer;
    text-align: left;
  }

  .team-header:hover {
    background: var(--weplex-surface-hover);
  }

  .team-expand-icon {
    font-size: 10px;
    color: var(--weplex-text-muted);
    width: 12px;
    flex-shrink: 0;
  }

  .team-name {
    flex: 1;
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .team-member-count {
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
    flex-shrink: 0;
  }

  .active-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--weplex-active);
    flex-shrink: 0;
  }

  /* Team detail (expanded) */
  .team-detail {
    padding: 8px 10px 10px;
    border-top: 1px solid var(--weplex-border);
  }

  .team-detail-actions {
    display: flex;
    gap: 6px;
    padding-top: 8px;
    border-top: 1px solid var(--weplex-border);
    flex-wrap: wrap;
  }

  /* Invite section */
  .invite-section {
    margin-bottom: 12px;
  }

  .invite-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .expires-text {
    display: block;
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
    margin-top: 4px;
  }

  .invite-code {
    font-family: var(--weplex-font-mono);
    font-size: 10px;
    padding: 4px 8px;
    background: var(--weplex-bg);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-sm);
    color: var(--weplex-accent);
    user-select: all;
    word-break: break-all;
    max-width: 200px;
    line-height: 1.3;
  }

  .btn-icon {
    width: 26px;
    height: 26px;
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-sm);
    background: transparent;
    color: var(--weplex-text-muted);
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    position: relative;
  }

  .btn-icon:hover {
    background: var(--weplex-surface-hover);
    color: var(--weplex-text);
  }

  .btn-icon:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .btn-icon.danger:hover {
    background: rgba(239, 68, 68, 0.1);
    color: var(--weplex-error);
    border-color: var(--weplex-error);
  }

  .copy-ok {
    position: absolute;
    top: -20px;
    left: 50%;
    transform: translateX(-50%);
    font-size: 10px;
    color: var(--weplex-active);
    white-space: nowrap;
    animation: fadeUp 1.5s ease-out forwards;
  }

  @keyframes fadeUp {
    0% { opacity: 1; transform: translateX(-50%) translateY(0); }
    100% { opacity: 0; transform: translateX(-50%) translateY(-8px); }
  }

  :global(.spin) {
    animation: spin 0.6s linear infinite;
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }

  /* Members */
  .members-section {
    margin-bottom: 8px;
  }

  .member-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 0;
    border-bottom: 1px solid var(--weplex-border);
  }

  .member-row:last-child {
    border-bottom: none;
  }

  .member-avatar {
    width: 26px;
    height: 26px;
    border-radius: var(--weplex-radius-full);
    background: color-mix(in srgb, var(--weplex-accent) 20%, transparent);
    color: var(--weplex-accent);
    font-size: var(--weplex-text-xs);
    font-weight: 600;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .member-info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .member-name {
    font-size: var(--weplex-text-sm);
    color: var(--weplex-text);
    display: flex;
    align-items: center;
    gap: 4px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .member-email {
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
    font-family: var(--weplex-font-mono);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .you-tag {
    font-size: 9px;
    padding: 0 4px;
    border-radius: var(--weplex-radius-full);
    background: color-mix(in srgb, var(--weplex-active) 15%, transparent);
    color: var(--weplex-active);
  }

  .role-badge {
    font-size: var(--weplex-text-xs);
    padding: 1px 6px;
    border-radius: var(--weplex-radius-full);
    background: var(--weplex-surface-hover);
    color: var(--weplex-text-muted);
    display: flex;
    align-items: center;
    gap: 3px;
    flex-shrink: 0;
  }

  .role-badge.owner {
    background: color-mix(in srgb, var(--weplex-accent) 15%, transparent);
    color: var(--weplex-accent);
  }
</style>

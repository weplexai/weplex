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
  } from 'lucide-svelte';
  import type { TeamMember } from '../../types';

  // Create team form
  let createName = $state('');
  let createLoading = $state(false);
  let createError = $state<string | null>(null);

  // Join team form
  let joinCode = $state('');
  let joinLoading = $state(false);
  let joinError = $state<string | null>(null);

  // General
  let copyFeedback = $state(false);
  let regenLoading = $state(false);
  let removingMemberId = $state<string | null>(null);
  let leaveLoading = $state(false);

  let team = $derived(teamStore.team);
  let currentUserId = $derived(authStore.user?.id ?? null);
  let isAdmin = $derived(
    team ? team.ownerId === currentUserId || team.members.some(
      (m) => m.id === currentUserId && m.teamRole === 'admin',
    ) : false,
  );

  async function handleCreate() {
    if (!createName.trim()) return;
    createLoading = true;
    createError = null;
    try {
      await teamStore.createTeam(createName.trim());
      createName = '';
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
    } catch (e: unknown) {
      joinError = e instanceof Error ? e.message : String(e);
    } finally {
      joinLoading = false;
    }
  }

  async function copyInviteCode() {
    if (!team) return;
    try {
      await navigator.clipboard.writeText(team.inviteCode);
      copyFeedback = true;
      setTimeout(() => (copyFeedback = false), 1500);
    } catch {}
  }

  async function handleRegenCode() {
    regenLoading = true;
    try {
      await teamStore.regenerateCode();
    } catch (e: unknown) {
      console.error('Regenerate failed:', e);
    } finally {
      regenLoading = false;
    }
  }

  async function handleRemoveMember(memberId: string) {
    removingMemberId = memberId;
    try {
      await teamStore.removeMember(memberId);
    } catch (e: unknown) {
      console.error('Remove failed:', e);
    } finally {
      removingMemberId = null;
    }
  }

  async function handleLeave() {
    leaveLoading = true;
    try {
      await teamStore.leaveTeam();
    } catch (e: unknown) {
      console.error('Leave failed:', e);
    } finally {
      leaveLoading = false;
    }
  }

  function memberInitial(member: TeamMember): string {
    const name = member.displayName || member.email;
    return name[0]?.toUpperCase() || '?';
  }

  function memberLabel(member: TeamMember): string {
    return member.displayName || member.email;
  }

  function handleCreateKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') handleCreate();
  }

  function handleJoinKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') handleJoin();
  }
</script>

{#if !authStore.isAuthenticated}
  <h3 class="section-title">Team</h3>
  <p class="section-desc">Sign in to create or join a team for collaborative pipelines.</p>

{:else if !team}
  <!-- No team: Create / Join -->
  <h3 class="section-title">Team</h3>
  <p class="section-desc">Create a team or join an existing one to run collaborative pipelines.</p>

  <div class="team-form-group">
    <span class="form-label">Create Team</span>
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
        {#if createLoading}Creating...{:else}<Plus size={12} /> Create{/if}
      </button>
    </div>
    {#if createError}
      <span class="form-error">{createError}</span>
    {/if}
  </div>

  <div class="team-form-group">
    <span class="form-label">Join Team</span>
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

{:else}
  <!-- Has team -->
  <h3 class="section-title">{team.name}</h3>
  <p class="section-desc">{team.members.length} member{team.members.length !== 1 ? 's' : ''}</p>

  <!-- Invite code -->
  <div class="invite-section">
    <span class="form-label">Invite Code</span>
    <div class="invite-row">
      <code class="invite-code">{team.inviteCode}</code>
      <button class="btn-icon" title="Copy" onclick={copyInviteCode}>
        <Copy size={13} />
        {#if copyFeedback}<span class="copy-ok">Copied</span>{/if}
      </button>
      {#if isAdmin}
        <button
          class="btn-icon"
          title="Regenerate code"
          onclick={handleRegenCode}
          disabled={regenLoading}
        >
          <RefreshCw size={13} class={regenLoading ? 'spin' : ''} />
        </button>
      {/if}
    </div>
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
            {#if member.id === currentUserId}<span class="you-tag">you</span>{/if}
          </span>
          {#if member.displayName}
            <span class="member-email">{member.email}</span>
          {/if}
        </div>
        <span class="role-badge" class:admin={member.teamRole === 'admin'}>
          {#if member.teamRole === 'admin'}<Shield size={10} />{/if}
          {member.teamRole}
        </span>
        {#if isAdmin && member.id !== currentUserId}
          <button
            class="btn-icon danger"
            title="Remove member"
            onclick={() => handleRemoveMember(member.id)}
            disabled={removingMemberId === member.id}
          >
            <UserMinus size={13} />
          </button>
        {/if}
      </div>
    {/each}
  </div>

  <!-- Leave team -->
  <div class="leave-section">
    <button class="btn-sm delete" onclick={handleLeave} disabled={leaveLoading}>
      <LogOut size={12} />
      {#if leaveLoading}Leaving...{:else}Leave Team{/if}
    </button>
  </div>
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

  .team-form-group {
    margin-bottom: 16px;
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

  /* Invite section */
  .invite-section {
    margin-bottom: 16px;
  }

  .invite-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .invite-code {
    font-family: var(--weplex-font-mono);
    font-size: var(--weplex-text-sm);
    padding: 4px 8px;
    background: var(--weplex-bg);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-sm);
    color: var(--weplex-accent);
    user-select: all;
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
    margin-bottom: 16px;
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

  .role-badge.admin {
    background: color-mix(in srgb, var(--weplex-accent) 15%, transparent);
    color: var(--weplex-accent);
  }

  .leave-section {
    padding-top: 8px;
    border-top: 1px solid var(--weplex-border);
  }
</style>

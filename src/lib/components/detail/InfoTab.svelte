<script lang="ts">
  import type { Session } from '../../types';
  import {
    formatCost,
    formatTokens,
    formatDuration,
    timeAgo,
    formatAbsoluteTime,
    formatRate,
  } from '../../utils/time';
  import { spaceStore } from '../../stores/spaceStore.svelte';
  import { profileStore } from '../../stores/profileStore.svelte';
  import { folderStore } from '../../stores/folderStore.svelte';
  import { settingsStore } from '../../stores/settingsStore.svelte';
  import { noteStore } from '../../stores/noteStore.svelte';
  import { shortPath } from '../../utils/path';
  import { hookStore } from '../../stores/hookStore.svelte';

  let { session }: { session: Session } = $props();

  let space = $derived(
    spaceStore.spaces.find((s) => s.id === session.spaceId),
  );
  let profile = $derived(
    space?.profileId ? profileStore.getById(space.profileId) : profileStore.defaultProfile,
  );
  let folder = $derived(
    session.folderId ? folderStore.folders.find((f) => f.id === session.folderId) : undefined,
  );
  let profileEnvVars = $derived(profile ? Object.entries(profile.envVars || {}) : []);
  let sessionDurationMs = $derived(Date.now() - session.createdAt);

  // Note key: SSH -> "user@host", others -> cwd
  let noteKey = $derived(
    session.type === 'ssh' && session.host
      ? session.sshUser
        ? `${session.sshUser}@${session.host}`
        : session.host
      : session.cwd || '',
  );
  let noteKeyType = $derived<'cwd' | 'ssh'>(session.type === 'ssh' ? 'ssh' : 'cwd');

  let notesValue = $state('');
  $effect(() => {
    notesValue = noteKey ? noteStore.getByKey(noteKey)?.content || '' : '';
  });

  function saveNotes() {
    if (noteKey) noteStore.upsert(noteKey, noteKeyType, notesValue);
  }

  function shortCwd(cwd: string): string {
    return shortPath(cwd);
  }

  // Hook activity for current session
  let hookConflicts = $derived(hookStore.getConflictsForSession(session.id));
  const NOISE_AGENT_TYPES = new Set(['unknown', 'general-purpose', 'Agent']);
  let sessionSubAgents = $derived(
    hookStore.getSubAgents(session.id).filter((s) => !NOISE_AGENT_TYPES.has(s.agentType)),
  );
  let runningSubAgents = $derived(sessionSubAgents.filter((s) => s.status === 'running'));
  let completedSubAgents = $derived(sessionSubAgents.filter((s) => s.status === 'completed'));

  function shellName(command: string | undefined): string {
    if (!command) return settingsStore.settings.defaultShell.split('/').pop() || 'shell';
    return command.split('/').pop()?.split(' ')[0] || command;
  }
</script>

<!-- Space / Profile section -->
{#if space}
  <section class="section">
    <h3 class="section-title">SESSION</h3>
    <div class="field">
      <span class="field-label">Space</span>
      <span class="field-value space-badge" style="--space-color: {space.color}">
        {space.name}
      </span>
    </div>
    {#if folder}
      <div class="field">
        <span class="field-label">Folder</span>
        <span class="field-value">{folder.name}</span>
      </div>
    {/if}
    {#if profile}
      <div class="field">
        <span class="field-label">Profile</span>
        <span class="field-value">{profile.name}</span>
      </div>
    {/if}
    {#if session.type === 'ssh' && session.host}
      <div class="field">
        <span class="field-label">Host</span>
        <span class="field-value mono"
          >{session.sshUser ? `${session.sshUser}@${session.host}` : session.host}</span
        >
      </div>
    {:else if session.type === 'terminal'}
      <div class="field">
        <span class="field-label">Shell</span>
        <span class="field-value mono">{shellName(session.command)}</span>
      </div>
    {:else if session.cwd}
      <div class="field">
        <span class="field-label">Directory</span>
        <span class="field-value mono">{shortCwd(session.cwd)}</span>
      </div>
    {/if}
    <div class="field">
      <span class="field-label">Activity</span>
      <span class="field-value">{timeAgo(session.lastActivity)}</span>
    </div>
    <div class="field">
      <span class="field-label">Started</span>
      <span class="field-value">{formatAbsoluteTime(session.createdAt)}</span>
    </div>
  </section>
{/if}

{#if session.type === 'agent'}
  <!-- Git section -->
  {#if session.branch || session.gitFiles?.length}
    <section class="section">
      <h3 class="section-title">GIT</h3>
      {#if session.branch}
        <div class="field">
          <span class="field-value branch">{session.branch}</span>
        </div>
      {/if}
      {#if session.gitFiles?.length}
        <div class="file-list">
          {#each session.gitFiles as file}
            <div class="file-item">
              <span
                class="file-status"
                class:modified={file.status === 'M'}
                class:added={file.status === 'A'}
                class:deleted={file.status === 'D'}
              >
                {file.status}
              </span>
              <span class="file-path">{file.path}</span>
            </div>
          {/each}
        </div>
      {/if}
    </section>
  {/if}

  <!-- Usage section -->
  {#if session.tokensIn || session.tokensOut}
    <section class="section">
      <h3 class="section-title">USAGE</h3>
      <div class="field">
        <span class="field-label">Input</span>
        <span class="field-value mono">{formatTokens(session.tokensIn || 0)}</span>
      </div>
      <div class="field">
        <span class="field-label">Output</span>
        <span class="field-value mono">{formatTokens(session.tokensOut || 0)}</span>
      </div>
      {#if session.cacheReadTokens}
        <div class="field">
          <span class="field-label">Cache read</span>
          <span class="field-value mono">{formatTokens(session.cacheReadTokens)}</span>
        </div>
      {/if}
      {#if session.cacheWriteTokens}
        <div class="field">
          <span class="field-label">Cache write</span>
          <span class="field-value mono">{formatTokens(session.cacheWriteTokens)}</span>
        </div>
      {/if}
      <div class="field">
        <span class="field-label">Rate</span>
        <span class="field-value mono"
          >{formatRate(session.tokensOut || 0, sessionDurationMs)}</span
        >
      </div>
      {#if session.turns}
        <div class="field">
          <span class="field-label">Turns</span>
          <span class="field-value">{session.turns}</span>
        </div>
      {/if}
      {#if session.authType === 'api-key' && session.cost}
        <div class="field">
          <span class="field-label">Cost</span>
          <span class="field-value">{formatCost(session.cost)}</span>
        </div>
      {/if}
    </section>
  {/if}

  <!-- Agent section -->
  <section class="section">
    <h3 class="section-title">AGENT</h3>
    <div class="field">
      <span class="field-label">Type</span>
      <span class="field-value">{session.agentType || '\u2014'}</span>
    </div>
    {#if session.model}
      <div class="field">
        <span class="field-label">Model</span>
        <span class="field-value accent">{session.model}</span>
      </div>
    {/if}
    <div class="field">
      <span class="field-label">Duration</span>
      <span class="field-value">{formatDuration(Date.now() - session.createdAt)}</span>
    </div>
    {#if session.toolCalls}
      <div class="field">
        <span class="field-label">Tool calls</span>
        <span class="field-value">{session.toolCalls}</span>
      </div>
    {/if}
  </section>

  <!-- Sub-agents spawned by this session -->
  {#if sessionSubAgents.length > 0}
    <section class="section">
      <h3 class="section-title">SUB-AGENTS {#if runningSubAgents.length > 0}<span class="running-badge">{runningSubAgents.length} running</span>{/if}</h3>
      <div class="subagent-list">
        {#each sessionSubAgents as sub}
          <div class="subagent-entry" class:running={sub.status === 'running'}>
            <span class="subagent-dot" class:active={sub.status === 'running'}></span>
            <span class="subagent-type">{sub.agentType}</span>
            {#if sub.status === 'completed' && sub.finishedAt && sub.startedAt}
              <span class="subagent-duration">{Math.round((sub.finishedAt - sub.startedAt) / 1000)}s</span>
            {/if}
          </div>
        {/each}
      </div>
    </section>
  {/if}

  <!-- Conflict warnings -->
  {#if hookConflicts.length > 0}
    <section class="section">
      <h3 class="section-title conflict-title">CONFLICTS</h3>
      {#each hookConflicts as conflict}
        <div class="conflict-entry">
          <span class="conflict-icon">&#9888;</span>
          <span class="conflict-file">{conflict.filePath.split('/').pop()}</span>
          <span class="conflict-info">+{conflict.otherSessions.length} session{conflict.otherSessions.length > 1 ? 's' : ''}</span>
        </div>
      {/each}
    </section>
  {/if}
{:else if session.type === 'ssh'}
  <section class="section">
    <h3 class="section-title">CONNECTION</h3>
    <div class="field">
      <span class="field-label">Port</span>
      <span class="field-value mono">{session.sshPort || 22}</span>
    </div>
    {#if session.sshKeyPath}
      <div class="field">
        <span class="field-label">Key</span>
        <span class="field-value mono">{session.sshKeyPath.split('/').pop()}</span>
      </div>
    {/if}
    <div class="field">
      <span class="field-label">Uptime</span>
      <span class="field-value">{formatDuration(Date.now() - session.createdAt)}</span>
    </div>
  </section>
{:else}
  <!-- Terminal -->
  <section class="section">
    <h3 class="section-title">PROCESS</h3>
    {#if session.command}
      <div class="field">
        <span class="field-label">Command</span>
        <span class="field-value mono">{session.command}</span>
      </div>
    {/if}
    {#if session.pid}
      <div class="field">
        <span class="field-label">PID</span>
        <span class="field-value mono">{session.pid}</span>
      </div>
    {/if}
    {#if session.exitCode !== undefined}
      <div class="field">
        <span class="field-label">Exit code</span>
        <span
          class="field-value mono"
          class:exit-ok={session.exitCode === 0}
          class:exit-err={session.exitCode !== 0}>{session.exitCode}</span
        >
      </div>
    {/if}
    <div class="field">
      <span class="field-label">Uptime</span>
      <span class="field-value">{formatDuration(Date.now() - session.createdAt)}</span>
    </div>
    {#if session.cwd}
      <div class="field">
        <span class="field-label">Directory</span>
        <span class="field-value mono">{shortCwd(session.cwd)}</span>
      </div>
    {/if}
  </section>

  {#if session.branch}
    <section class="section">
      <h3 class="section-title">GIT</h3>
      <div class="field">
        <span class="field-value branch">{session.branch}</span>
      </div>
    </section>
  {/if}

  {#if profileEnvVars.length > 0}
    <section class="section">
      <h3 class="section-title">ENV</h3>
      <div class="env-list">
        {#each profileEnvVars as [key, val]}
          <span class="env-badge"
            ><span class="env-key">{key}</span><span class="env-val">{val}</span></span
          >
        {/each}
      </div>
    </section>
  {/if}
{/if}

<!-- NOTES section (all types) -->
<section class="section">
  <h3 class="section-title">NOTES</h3>
  <textarea
    class="notes-input"
    placeholder="Add notes..."
    aria-label="Session notes"
    bind:value={notesValue}
    onblur={saveNotes}
  ></textarea>
</section>

<style>
  .section {
    margin-bottom: 16px;
  }

  .section-title {
    font-size: 10px;
    font-weight: 600;
    color: var(--weplex-text-muted);
    letter-spacing: 0.06em;
    margin-bottom: 8px;
  }

  .field {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    padding: 3px 0;
    font-size: var(--weplex-text-sm);
  }

  .field-label {
    color: var(--weplex-text-muted);
  }

  .field-value {
    color: var(--weplex-text);
  }

  .field-value.mono {
    font-family: var(--weplex-font-mono);
    font-size: var(--weplex-text-xs);
  }

  .field-value.accent {
    color: var(--weplex-accent);
  }

  .field-value.space-badge {
    color: var(--space-color);
  }

  .field-value.branch {
    color: var(--weplex-active);
    font-family: var(--weplex-font-mono);
    font-size: var(--weplex-text-sm);
  }

  .file-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-top: 4px;
  }

  .file-item {
    display: flex;
    gap: 6px;
    font-size: var(--weplex-text-xs);
    font-family: var(--weplex-font-mono);
  }

  .file-status {
    width: 14px;
    text-align: center;
    font-weight: 600;
    flex-shrink: 0;
  }

  .file-status.modified {
    color: var(--weplex-warning);
  }
  .file-status.added {
    color: var(--weplex-active);
  }
  .file-status.deleted {
    color: var(--weplex-error);
  }

  .file-path {
    color: var(--weplex-text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .exit-ok {
    color: var(--weplex-active);
  }
  .exit-err {
    color: var(--weplex-error);
  }

  .env-list {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-top: 4px;
  }

  .env-badge {
    display: inline-flex;
    align-items: center;
    border-radius: var(--weplex-radius-sm);
    overflow: hidden;
    font-size: var(--weplex-text-xs);
    font-family: var(--weplex-font-mono);
    border: 1px solid var(--weplex-border);
  }

  .env-key {
    padding: 2px 5px;
    background: var(--weplex-surface-hover);
    color: var(--weplex-text-muted);
  }

  .env-val {
    padding: 2px 5px;
    background: transparent;
    color: var(--weplex-text);
  }

  .notes-input {
    width: 100%;
    min-height: 120px;
    padding: 6px 8px;
    background: var(--weplex-bg);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-sm);
    color: var(--weplex-text);
    font-size: var(--weplex-text-xs);
    font-family: var(--weplex-font-mono);
    resize: vertical;
    outline: none;
    box-sizing: border-box;
    line-height: 1.5;
  }

  .notes-input:focus {
    border-color: var(--weplex-accent);
  }

  .notes-input::placeholder {
    color: var(--weplex-text-muted);
  }

  .subagent-list {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .subagent-entry {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: var(--weplex-text-xs);
    padding: 3px 0;
  }

  .subagent-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--weplex-text-muted);
    flex-shrink: 0;
  }

  .subagent-dot.active {
    background: var(--weplex-active);
    animation: dot-pulse 1.4s ease-in-out infinite;
  }

  .subagent-type {
    color: var(--weplex-accent);
    font-weight: 500;
  }

  .subagent-duration {
    color: var(--weplex-text-muted);
    margin-left: auto;
    font-family: var(--weplex-font-mono);
  }

  .running-badge {
    font-size: 9px;
    color: var(--weplex-active);
    font-weight: 400;
    letter-spacing: 0;
    text-transform: none;
    margin-left: 4px;
  }

  .conflict-title {
    color: var(--weplex-warning) !important;
  }

  .conflict-entry {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: var(--weplex-text-xs);
    padding: 2px 0;
  }

  .conflict-icon {
    color: var(--weplex-warning);
    flex-shrink: 0;
  }

  .conflict-file {
    color: var(--weplex-text);
    font-family: var(--weplex-font-mono);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .conflict-info {
    color: var(--weplex-text-muted);
    font-size: 10px;
    flex-shrink: 0;
  }
</style>

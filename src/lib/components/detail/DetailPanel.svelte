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
  import { pipelineRunStore } from '../../stores/pipelineRunStore.svelte';
  import { collabPipelineStore } from '../../stores/collabPipelineStore.svelte';
  import { shortPath } from '../../utils/path';
  import StageOutput from './StageOutput.svelte';
  import CollabRunDetail from './CollabRunDetail.svelte';
  import ActivitySection from './ActivitySection.svelte';

  let { session }: { session: Session | undefined } = $props();
  let showPipelineView = $derived(pipelineRunStore.activeRunId !== null);
  let showCollabView = $derived(collabPipelineStore.activeRunId !== null);

  let space = $derived(
    session ? spaceStore.spaces.find((s) => s.id === session.spaceId) : undefined,
  );
  let profile = $derived(
    space?.profileId ? profileStore.getById(space.profileId) : profileStore.defaultProfile,
  );
  let folder = $derived(
    session?.folderId ? folderStore.folders.find((f) => f.id === session.folderId) : undefined,
  );
  let profileEnvVars = $derived(profile ? Object.entries(profile.envVars || {}) : []);
  let sessionDurationMs = $derived(session ? Date.now() - session.createdAt : 0);

  // Note key: SSH → "user@host", others → cwd
  let noteKey = $derived(
    session?.type === 'ssh' && session.host
      ? session.sshUser
        ? `${session.sshUser}@${session.host}`
        : session.host
      : session?.cwd || '',
  );
  let noteKeyType = $derived<'cwd' | 'ssh'>(session?.type === 'ssh' ? 'ssh' : 'cwd');

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

  function shellName(command: string | undefined): string {
    if (!command) return settingsStore.settings.defaultShell.split('/').pop() || 'shell';
    return command.split('/').pop()?.split(' ')[0] || command;
  }
</script>

<aside class="detail-panel">
  {#if showCollabView}
    <CollabRunDetail />
  {:else if showPipelineView}
    <StageOutput />
  {:else if session}
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

      <!-- Activity notes from agent (polled every 10s) -->
      <ActivitySection sessionId={session.id} />
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
        placeholder="Add notes…"
        aria-label="Session notes"
        bind:value={notesValue}
        onblur={saveNotes}
      ></textarea>
    </section>
  {:else}
    <div class="empty">No session selected</div>
  {/if}
</aside>

<style>
  .detail-panel {
    width: var(--weplex-detail-width);
    min-width: var(--weplex-detail-width);
    margin: 6px 6px 6px 3px;
    border-radius: 10px;
    box-shadow:
      0 0 0 1px rgba(0, 0, 0, 0.4),
      0 2px 8px rgba(0, 0, 0, 0.3);
    background: var(--weplex-bg);
    overflow-y: auto;
    padding: 16px;
  }

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

  .empty {
    padding: 24px;
    text-align: center;
    font-size: var(--weplex-text-sm);
    color: var(--weplex-text-muted);
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
    min-height: 72px;
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
</style>

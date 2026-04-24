<script lang="ts">
  import { onMount } from 'svelte';
  import { commandStore, type Command } from '../../stores/commandStore.svelte';
  import { spaceStore } from '../../stores/spaceStore.svelte';
  import { sessionStore } from '../../stores/sessionStore.svelte';
  import { Button, Modal, Select } from '../ui';
  import { Plus, Save, Trash2, AlertCircle, Zap, FileText } from 'lucide-svelte';

  const ICON_COLOR_OPTIONS = [
    { value: 'accent', label: 'Red (accent)' },
    { value: 'active', label: 'Green (active)' },
    { value: 'warning', label: 'Yellow (warning)' },
    { value: 'error', label: 'Red (error)' },
    { value: 'info', label: 'Blue (info)' },
    { value: 'model-opus', label: 'Purple (opus)' },
    { value: 'model-sonnet', label: 'Blue (sonnet)' },
    { value: 'model-haiku', label: 'Green (haiku)' },
    { value: 'text-muted', label: 'Gray (muted)' },
  ];

  const MODEL_OPTIONS = [
    { value: '', label: 'Default' },
    { value: 'opus', label: 'Opus' },
    { value: 'sonnet', label: 'Sonnet' },
    { value: 'haiku', label: 'Haiku' },
  ];

  const SCOPE_OPTIONS = [
    { value: 'user', label: 'User (global)' },
    { value: 'project', label: 'Project' },
  ];

  let selected = $state<Command | null>(null);
  let editMode = $state<'view' | 'edit' | 'new'>('view');
  let editError = $state<string | null>(null);
  let confirmDelete = $state<Command | null>(null);

  // Editor form
  let formName = $state('');
  let formScope = $state<'user' | 'project'>('user');
  let formDescription = $state('');
  let formArgumentHint = $state('');
  let formAllowedTools = $state('');
  let formModel = $state('');
  let formBody = $state('');
  let formIcon = $state('');
  let formIconColor = $state('accent');
  let formAdapterDefault = $state('');

  // Get active session cwd for project-level commands
  let activeCwd = $derived(sessionStore.activeSession?.cwd);

  onMount(() => {
    commandStore.load(activeCwd);
  });

  function selectCmd(cmd: Command) {
    selected = cmd;
    editMode = 'view';
    editError = null;
  }

  function startNew() {
    selected = null;
    editMode = 'new';
    editError = null;
    formName = '';
    formScope = 'user';
    formDescription = '';
    formArgumentHint = '';
    formAllowedTools = '';
    formModel = '';
    formBody = '';
    formIcon = '';
    formIconColor = 'accent';
    formAdapterDefault = '';
  }

  function startEdit() {
    if (!selected) return;
    editMode = 'edit';
    editError = null;
    formName = selected.name;
    formScope = selected.scope;
    formDescription = selected.description;
    formArgumentHint = selected.argumentHint;
    formAllowedTools = selected.allowedTools.join(', ');
    formModel = selected.model;
    formBody = selected.body;
    formIcon = selected.icon;
    formIconColor = selected.iconColor;
    formAdapterDefault = selected.adapters.default || '';
  }

  async function saveForm() {
    if (!formName.trim()) { editError = 'Name is required'; return; }
    if (!formBody.trim()) { editError = 'Command body is required'; return; }

    const tools = formAllowedTools
      .split(',')
      .map((s) => s.trim())
      .filter(Boolean);

    const err = await commandStore.save(
      formName.trim(),
      formScope,
      activeCwd,
      formDescription.trim(),
      formArgumentHint.trim(),
      tools,
      formModel,
      formBody,
      {
        icon: formIcon.trim() || formName.charAt(0).toUpperCase(),
        iconColor: formIconColor,
        showInPanel: true,
        adapters: formAdapterDefault.trim() ? { default: formAdapterDefault.trim() } : undefined,
      },
    );

    if (err) { editError = err; return; }
    selected = commandStore.getByName(formName.trim()) || null;
    editMode = 'view';
    editError = null;
  }

  async function doDelete() {
    if (!confirmDelete) return;
    const err = await commandStore.remove(confirmDelete.name, confirmDelete.filePath, activeCwd);
    if (err) { editError = err; }
    if (selected?.name === confirmDelete.name) selected = null;
    confirmDelete = null;
  }

  function shortenPath(p: string): string {
    // Handle macOS (/Users/x/) and Linux (/home/x/)
    for (const prefix of ['/Users/', '/home/']) {
      const idx = p.indexOf(prefix);
      if (idx >= 0) {
        const after = p.slice(idx + prefix.length);
        const slashIdx = after.indexOf('/');
        if (slashIdx >= 0) return '~' + after.slice(slashIdx);
      }
    }
    return p;
  }
</script>

<div class="commands-layout">
  <!-- Sidebar -->
  <div class="commands-sidebar">
    <div class="commands-sidebar-header">
      <h3 class="commands-sidebar-title">Commands</h3>
    </div>

    <nav class="commands-nav">
      {#if commandStore.loading}
        <div class="commands-empty">Loading...</div>
      {:else}
        {#if commandStore.userCommands.length > 0}
          <div class="nav-section-label">User</div>
          {#each commandStore.userCommands as cmd (cmd.name)}
            <button
              class="cmd-row"
              class:selected={selected?.name === cmd.name && editMode === 'view'}
              onclick={() => selectCmd(cmd)}
            >
              <span class="row-icon" style="--cmd-color: var(--weplex-{commandStore.safeIconColor(cmd)})">{cmd.icon}</span>
              <span class="row-name">{cmd.name}</span>
            </button>
          {/each}
        {/if}

        {#if commandStore.projectCommands.length > 0}
          <div class="nav-section-label">Project</div>
          {#each commandStore.projectCommands as cmd (cmd.name)}
            <button
              class="cmd-row"
              class:selected={selected?.name === cmd.name && editMode === 'view'}
              onclick={() => selectCmd(cmd)}
            >
              <span class="row-icon" style="--cmd-color: var(--weplex-{commandStore.safeIconColor(cmd)})">{cmd.icon}</span>
              <span class="row-name">{cmd.name}</span>
            </button>
          {/each}
        {/if}

        {#if commandStore.commands.length === 0}
          <div class="commands-empty">No commands found.<br/>Add .md files to ~/.claude/commands/</div>
        {/if}
      {/if}
    </nav>

    <div class="commands-footer">
      <button class="commands-action-btn" onclick={startNew}>
        <Plus size={13} />
        <span>New Command</span>
      </button>
    </div>
  </div>

  <!-- Main content -->
  <div class="commands-main">
    {#if editMode === 'new' || editMode === 'edit'}
      <div class="editor">
        <div class="editor-header">
          <h2>{editMode === 'new' ? 'New Command' : `Edit: ${formName}`}</h2>
          <div class="editor-actions">
            <Button variant="secondary" onclick={() => { editMode = 'view'; editError = null; }}>Cancel</Button>
            <Button variant="primary" onclick={saveForm}><Save size={13} /> Save</Button>
          </div>
        </div>

        {#if editError}
          <div class="editor-error"><AlertCircle size={13} />{editError}</div>
        {/if}

        <div class="editor-form">
          <div class="form-row-pair">
            <div class="form-row">
              <label>Name
                <input type="text" bind:value={formName} placeholder="review" disabled={editMode === 'edit'} />
              </label>
            </div>
            <div class="form-row">
              <label>Scope
                <Select value={formScope} options={SCOPE_OPTIONS} onchange={(v) => { formScope = v as 'user' | 'project'; }} />
              </label>
            </div>
          </div>

          <div class="form-row">
            <label>Description
              <input type="text" bind:value={formDescription} placeholder="What this command does" />
            </label>
          </div>

          <div class="form-row-pair">
            <div class="form-row">
              <label>Argument hint
                <input type="text" bind:value={formArgumentHint} placeholder="[--flag] [$1]" />
              </label>
            </div>
            <div class="form-row">
              <label>Model
                <Select value={formModel} options={MODEL_OPTIONS} onchange={(v) => { formModel = v; }} />
              </label>
            </div>
          </div>

          <div class="form-row">
            <label>Allowed tools <span class="form-hint">(comma-separated)</span>
              <input type="text" bind:value={formAllowedTools} placeholder="Read, Grep, Glob, Bash, Edit, Write, Agent" />
            </label>
          </div>

          <div class="form-row">
            <label>Command body <span class="form-hint">(instructions for Claude)</span>
              <textarea bind:value={formBody} rows={10} placeholder="Review this file and suggest improvements..."></textarea>
            </label>
          </div>

          <div class="form-divider-label">Weplex display</div>

          <div class="form-row-pair">
            <div class="form-row">
              <label>Icon letter
                <input type="text" bind:value={formIcon} placeholder="R" maxlength="2" />
              </label>
            </div>
            <div class="form-row">
              <label>Icon color
                <Select value={formIconColor} options={ICON_COLOR_OPTIONS} onchange={(v) => { formIconColor = v; }} />
              </label>
            </div>
          </div>

          <div class="form-row">
            <label>Non-Claude adapter <span class="form-hint">(text sent to other agents)</span>
              <textarea bind:value={formAdapterDefault} rows={4} placeholder="Fallback text for Codex, Aider, etc."></textarea>
            </label>
          </div>
        </div>
      </div>

    {:else if selected}
      <div class="detail">
        <div class="d-header">
          <span class="d-icon" style="--cmd-color: var(--weplex-{commandStore.safeIconColor(selected)})">{selected.icon}</span>
          <h2 class="d-name">{selected.name}</h2>
          <span class="d-tag scope">{selected.scope}</span>
          {#if selected.model}
            <span class="d-tag model">{selected.model}</span>
          {/if}
        </div>

        {#if selected.description}
          <p class="d-desc">{selected.description}</p>
        {/if}

        {#if selected.argumentHint}
          <div class="d-meta">
            <span class="d-meta-label">Args</span>
            <code class="d-meta-value">{selected.argumentHint}</code>
          </div>
        {/if}

        {#if selected.allowedTools.length > 0}
          <div class="d-tools">
            <span class="d-tools-label">Tools</span>
            {#each selected.allowedTools as tool}
              <span class="tool-chip">{tool}</span>
            {/each}
          </div>
        {/if}

        <div class="d-divider"></div>

        <div class="d-body-section">
          <h4 class="d-body-title">Command body</h4>
          <pre class="d-body">{selected.body}</pre>
        </div>

        {#if Object.keys(selected.adapters).length > 0}
          <div class="d-body-section">
            <h4 class="d-body-title">Non-Claude adapters</h4>
            {#each Object.entries(selected.adapters) as [agent, text]}
              <div class="adapter-block">
                <span class="adapter-label">{agent}</span>
                <pre class="d-body">{text}</pre>
              </div>
            {/each}
          </div>
        {/if}

        <div class="d-footer-actions">
          <Button variant="secondary" onclick={startEdit}>Edit</Button>
          <Button variant="danger" onclick={() => { confirmDelete = selected; }}><Trash2 size={12} /> Delete</Button>
        </div>

        <span class="d-filepath">
          <FileText size={11} />
          {shortenPath(selected.filePath)}
        </span>
      </div>

    {:else}
      <div class="commands-center-msg">
        <Zap size={32} strokeWidth={1.2} />
        <p>Select a command or create a new one</p>
        <p class="commands-center-hint">Commands are stored as .md files in ~/.claude/commands/</p>
      </div>
    {/if}
  </div>
</div>

{#if confirmDelete}
  <Modal onclose={() => (confirmDelete = null)} position="center" label="Confirm deletion" class="confirm-dialog">
    <p class="confirm-text">Delete command <strong>{confirmDelete.name}</strong>?</p>
    <p class="confirm-hint">File {shortenPath(confirmDelete.filePath)} will be removed.</p>
    <div class="confirm-actions">
      <Button variant="secondary" onclick={() => (confirmDelete = null)}>Cancel</Button>
      <Button variant="danger" onclick={doDelete}>Delete</Button>
    </div>
  </Modal>
{/if}

<style>
  .commands-layout { display: flex; width: 100%; height: 100%; background: var(--weplex-bg); overflow: hidden; }

  .commands-sidebar {
    width: 240px; min-width: 240px; display: flex; flex-direction: column;
    border-right: 1px solid var(--weplex-border); background: var(--weplex-sidebar-bg); position: relative;
  }
  .commands-sidebar::before {
    content: ''; position: absolute; inset: 0; pointer-events: none;
    background-image: radial-gradient(circle, rgba(255, 255, 255, 0.06) 0.5px, transparent 0.5px);
    background-size: 12px 12px;
  }
  .commands-sidebar-header { padding: 16px 14px 12px; flex-shrink: 0; }
  .commands-sidebar-title { font-size: var(--weplex-text-md); font-weight: 600; margin: 0; }
  .commands-nav { flex: 1; overflow-y: auto; }
  .commands-empty { padding: 16px; color: var(--weplex-text-muted); font-size: 12px; line-height: 1.5; }

  .nav-section-label {
    font-size: 9px; font-weight: 600; color: var(--weplex-text-muted);
    letter-spacing: 0.06em; text-transform: uppercase; padding: 10px 16px 4px; opacity: 0.6;
  }

  .cmd-row {
    display: flex; align-items: center; gap: 10px; width: 100%;
    padding: 6px 14px 6px 16px; border: none; background: transparent;
    cursor: pointer; text-align: left; transition: background var(--weplex-duration-fast);
    font-family: var(--weplex-font-ui); color: var(--weplex-text);
  }
  .cmd-row:hover { background: var(--weplex-surface-hover); }
  .cmd-row.selected { background: color-mix(in srgb, var(--weplex-accent) 10%, transparent); }

  .row-icon {
    display: flex; align-items: center; justify-content: center;
    width: 22px; height: 22px; border-radius: 5px; flex-shrink: 0;
    font-size: 10px; font-weight: 700; font-family: var(--weplex-font-mono);
    background: color-mix(in srgb, var(--cmd-color) 15%, transparent); color: var(--cmd-color);
  }

  .row-name {
    flex: 1; font-size: 13px; font-weight: 500; font-family: var(--weplex-font-mono);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .cmd-row.selected .row-name { color: var(--weplex-accent); }

  .commands-footer { padding: 8px; border-top: 1px solid var(--weplex-border); flex-shrink: 0; }
  .commands-action-btn {
    display: flex; align-items: center; gap: 6px; width: 100%;
    padding: 7px 10px; border: 1px dashed var(--weplex-border);
    border-radius: var(--weplex-radius-md); background: transparent;
    color: var(--weplex-text-muted); font-size: 12px; cursor: pointer;
    font-family: var(--weplex-font-ui); transition: all var(--weplex-duration-fast);
  }
  .commands-action-btn:hover { border-color: var(--weplex-accent); color: var(--weplex-accent); border-style: solid; }

  .commands-main { flex: 1; display: flex; flex-direction: column; min-width: 0; overflow: hidden; }
  .commands-center-msg {
    flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center;
    gap: 12px; color: var(--weplex-text-muted); font-size: 13px;
  }
  .commands-center-msg p { margin: 0; }
  .commands-center-hint { font-size: 11px; opacity: 0.6; }

  /* Detail */
  .detail { flex: 1; overflow-y: auto; padding: 28px 40px; display: flex; flex-direction: column; }
  .d-header { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
  .d-icon {
    display: flex; align-items: center; justify-content: center;
    width: 32px; height: 32px; border-radius: 8px;
    font-size: 14px; font-weight: 700; font-family: var(--weplex-font-mono);
    background: color-mix(in srgb, var(--cmd-color) 15%, transparent); color: var(--cmd-color); flex-shrink: 0;
  }
  .d-name { font-size: 17px; font-weight: 700; color: var(--weplex-text); font-family: var(--weplex-font-mono); margin: 0; letter-spacing: -0.02em; }
  .d-tag {
    font-size: 10px; font-weight: 500; padding: 2px 8px; border-radius: 4px;
    border: 1px solid var(--weplex-border); color: var(--weplex-text-muted); font-family: var(--weplex-font-mono);
  }
  .d-tag.scope { color: var(--weplex-active); border-color: color-mix(in srgb, var(--weplex-active) 30%, transparent); }
  .d-tag.model { color: var(--weplex-model-opus); border-color: color-mix(in srgb, var(--weplex-model-opus) 30%, transparent); }
  .d-desc { font-size: 13px; color: var(--weplex-text-muted); line-height: 1.5; margin: 12px 0 0; max-width: 600px; }
  .d-meta { display: flex; align-items: center; gap: 8px; margin-top: 8px; }
  .d-meta-label { font-size: 11px; color: var(--weplex-text-muted); }
  .d-meta-value { font-size: 11px; font-family: var(--weplex-font-mono); color: var(--weplex-accent); background: var(--weplex-surface); padding: 2px 6px; border-radius: 4px; }
  .d-tools { display: flex; align-items: center; flex-wrap: wrap; gap: 5px; margin-top: 12px; }
  .d-tools-label { font-size: 11px; color: var(--weplex-text-muted); margin-right: 4px; }
  .tool-chip {
    font-size: 11px; padding: 2px 8px; border-radius: 5px;
    background: var(--weplex-surface); border: 1px solid var(--weplex-border);
    color: var(--weplex-text-secondary); font-family: var(--weplex-font-mono);
  }
  .d-divider { height: 1px; background: var(--weplex-border); margin: 20px 0 16px; }
  .d-body-section { margin-bottom: 16px; }
  .d-body-title { font-size: 12px; font-weight: 600; color: var(--weplex-text-muted); margin: 0 0 8px; text-transform: uppercase; letter-spacing: 0.04em; }
  .d-body {
    font-size: 11.5px; font-family: var(--weplex-font-mono); color: var(--weplex-text-secondary);
    line-height: 1.6; background: var(--weplex-surface); border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-md); padding: 12px 14px; margin: 0; white-space: pre-wrap; word-break: break-word;
  }
  .adapter-block { margin-top: 8px; }
  .adapter-label { font-size: 11px; font-weight: 600; color: var(--weplex-accent); font-family: var(--weplex-font-mono); margin-bottom: 4px; display: block; }
  .d-footer-actions { display: flex; gap: 8px; margin-top: 20px; }
  .d-filepath {
    display: inline-flex; align-items: center; gap: 5px; margin-top: 16px;
    color: var(--weplex-text-muted); font-size: 11px; font-family: var(--weplex-font-mono); opacity: 0.5;
  }

  /* Editor */
  .editor { flex: 1; overflow-y: auto; padding: 24px 32px; display: flex; flex-direction: column; }
  .editor-header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 20px; }
  .editor-header h2 { font-size: 15px; font-weight: 700; color: var(--weplex-text); font-family: var(--weplex-font-mono); margin: 0; }
  .editor-actions { display: flex; gap: 6px; }
  .editor-error {
    display: flex; align-items: center; gap: 6px; margin-bottom: 16px; padding: 8px 12px;
    border-radius: 6px; background: color-mix(in srgb, var(--weplex-error) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--weplex-error) 20%, transparent);
    color: var(--weplex-error); font-size: 12px;
  }
  .editor-form { display: flex; flex-direction: column; gap: 14px; }
  .form-row { display: flex; flex-direction: column; gap: 5px; }
  .form-row label {
    display: flex; flex-direction: column; gap: 5px; font-size: 11px; font-weight: 600;
    color: var(--weplex-text-muted); text-transform: uppercase; letter-spacing: 0.04em;
  }
  .form-hint { font-weight: 400; text-transform: none; letter-spacing: 0; opacity: 0.6; }
  .form-row input[type='text'], .form-row textarea {
    width: 100%; padding: 8px 10px; border: 1px solid var(--weplex-border); border-radius: 6px;
    background: var(--weplex-surface); color: var(--weplex-text); font-size: 13px;
    font-family: var(--weplex-font-mono); outline: none; transition: border-color var(--weplex-duration-fast);
  }
  .form-row input:focus, .form-row textarea:focus { border-color: var(--weplex-accent); }
  .form-row textarea { resize: vertical; line-height: 1.5; }
  .form-row input:disabled { opacity: 0.5; cursor: not-allowed; }
  .form-row-pair { display: flex; gap: 12px; }
  .form-row-pair > .form-row { flex: 1; }
  .form-divider-label {
    font-size: 10px; font-weight: 600; color: var(--weplex-text-muted);
    letter-spacing: 0.06em; text-transform: uppercase; padding-top: 8px;
    border-top: 1px solid var(--weplex-border); margin-top: 4px;
  }

  :global(.confirm-dialog) {
    width: 340px; padding: 20px; background: var(--weplex-surface);
    border: 1px solid var(--weplex-border); border-radius: var(--weplex-radius-xl);
    box-shadow: var(--weplex-shadow-overlay);
  }
  .confirm-text { font-size: 14px; margin: 0 0 6px; }
  .confirm-hint { font-size: 12px; color: var(--weplex-text-muted); margin: 0 0 16px; }
  .confirm-actions { display: flex; gap: 8px; justify-content: flex-end; }
</style>

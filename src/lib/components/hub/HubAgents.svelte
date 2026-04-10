<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { Select, Button, Modal } from '../ui';
  import { uiStore } from '../../stores/uiStore.svelte';
  import { sessionStore } from '../../stores/sessionStore.svelte';
  import { Plus, Save, FileText, AlertCircle } from 'lucide-svelte';
  import { modelShort, modelClass, initial } from '../overlays/helpers';
  import AgentDetail from '../overlays/AgentDetail.svelte';
  import WeplexAgentDetail from '../overlays/WeplexAgentDetail.svelte';
  import type { AgentConfig, SkillInfo } from '../overlays/types';

  // State
  let selectedAgent = $state<AgentConfig | null>(null);
  let selectedWeplexAgent = $state<(typeof weplexAgents)[0] | null>(null);
  let weplexAgentEditor = $state<WeplexAgentDetail>();
  let agents = $state<AgentConfig[]>([]);
  let weplexAgents = $state<
    { name: string; description: string; binary: string; model: string; prompt: string; file_path: string }[]
  >([]);
  let skills = $state<SkillInfo[]>([]);
  let loading = $state(true);

  // Editor
  let editMode = $state<'view' | 'edit-agent' | 'new-agent'>('view');
  let editAgent = $state<AgentConfig | null>(null);
  let editError = $state<string | null>(null);

  let sortedAgents = $derived([...agents].sort((a, b) => a.name.localeCompare(b.name)));

  onMount(() => { loadAll(); });

  async function loadAll() {
    loading = true;
    try {
      const [a, s, da] = await Promise.all([
        invoke<AgentConfig[]>('list_agents'),
        invoke<SkillInfo[]>('list_skills'),
        invoke<typeof weplexAgents>('list_weplex_agents'),
      ]);
      agents = a;
      skills = s;
      weplexAgents = da;
      if (agents.length > 0 && !selectedAgent) selectedAgent = agents[0];
    } catch (e) {
      console.error('Failed to load agents:', e);
    } finally {
      loading = false;
    }
  }

  function selectAgent(agent: AgentConfig) {
    selectedAgent = agent;
    selectedWeplexAgent = null;
    editMode = 'view';
  }

  function selectWeplexAgent(agent: (typeof weplexAgents)[0]) {
    selectedWeplexAgent = agent;
    selectedAgent = null;
    editMode = 'view';
  }

  function startNewWeplexAgent() {
    selectedAgent = null;
    selectedWeplexAgent = null;
    editMode = 'view';
    requestAnimationFrame(() => weplexAgentEditor?.startNew());
  }

  function startNewAgent() {
    editAgent = {
      name: '', description: '', model: 'sonnet',
      tools: ['Read', 'Grep', 'Glob', 'Edit', 'Write', 'Bash'],
      disallowed_tools: [], permission_mode: 'default',
      memory: null, max_turns: null, background: null, isolation: null,
      skills: [], system_prompt: '', file_path: '', source: 'user',
    };
    editMode = 'new-agent';
    editError = null;
  }

  function startEditAgent() {
    if (!selectedAgent) return;
    editAgent = {
      ...selectedAgent,
      tools: [...selectedAgent.tools],
      disallowed_tools: [...selectedAgent.disallowed_tools],
      skills: [...selectedAgent.skills],
    };
    editMode = 'edit-agent';
    editError = null;
  }

  async function saveAgentForm() {
    if (!editAgent) return;
    if (!editAgent.name.trim()) { editError = 'Name is required'; return; }
    if (!editAgent.description.trim()) { editError = 'Description is required'; return; }
    try {
      await invoke('save_agent', { agent: editAgent });
      await loadAll();
      selectedAgent = agents.find((a) => a.name === editAgent!.name) || agents[0] || null;
      editMode = 'view';
      editError = null;
    } catch (e: unknown) {
      editError = e instanceof Error ? e.message : String(e);
    }
  }

  let confirmDelete = $state<{ name: string } | null>(null);

  async function deleteCurrentAgent() {
    if (!selectedAgent) return;
    confirmDelete = { name: selectedAgent.name };
  }

  async function confirmDeleteAction() {
    if (!confirmDelete) return;
    try {
      await invoke('delete_agent', { name: confirmDelete.name });
      selectedAgent = null;
      await loadAll();
      if (agents.length > 0) selectedAgent = agents[0];
    } catch (e) {
      console.error('Delete failed:', e);
    } finally {
      confirmDelete = null;
    }
  }

  const ALL_TOOLS = ['Read', 'Grep', 'Glob', 'Edit', 'Write', 'Bash', 'Agent', 'WebFetch', 'WebSearch', 'NotebookEdit'];
</script>

<div class="agents-layout">
  <!-- Sidebar: agent list -->
  <div class="agents-sidebar">
    <div class="agents-sidebar-header">
      <h3 class="agents-sidebar-title">Agents</h3>
    </div>

    <nav class="agents-nav">
      {#if loading}
        <div class="agents-loading">Loading...</div>
      {:else}
        {#if weplexAgents.length > 0}
          <div class="nav-section-label">Weplex Agents</div>
          {#each weplexAgents as da}
            <button
              class="agent-row"
              class:selected={selectedWeplexAgent?.name === da.name && editMode === 'view'}
              onclick={() => selectWeplexAgent(da)}
            >
              <span class="row-letter weplex">W</span>
              <span class="row-name">{da.name}</span>
              <span class="row-model">{da.binary}</span>
            </button>
          {/each}
        {/if}
        {#if sortedAgents.length > 0}
          <div class="nav-section-label">Claude Agents</div>
        {/if}
        {#each sortedAgents as agent}
          {@const mc = modelClass(agent.model)}
          <button
            class="agent-row"
            class:selected={selectedAgent?.name === agent.name && editMode === 'view'}
            onclick={() => selectAgent(agent)}
          >
            <span class="row-letter {mc}">{initial(agent.name)}</span>
            <span class="row-name">{agent.name}</span>
            <span class="row-model {mc}">{modelShort(agent.model)}</span>
          </button>
        {/each}
        {#if agents.length === 0 && weplexAgents.length === 0}
          <div class="agents-empty">No agents yet. Create one to get started.</div>
        {/if}
      {/if}
    </nav>

    <div class="agents-footer">
      <button class="agents-action-btn" onclick={startNewWeplexAgent}>
        <Plus size={13} />
        <span>New Weplex Agent</span>
      </button>
      <button class="agents-action-btn" onclick={startNewAgent}>
        <Plus size={13} />
        <span>New Claude Agent</span>
      </button>
    </div>
  </div>

  <!-- Main content -->
  <div class="agents-main">
    {#if loading}
      <div class="agents-center-msg">Loading...</div>

    {:else if editMode === 'new-agent' || editMode === 'edit-agent'}
      {#if editAgent}
        <div class="editor">
          <div class="editor-header">
            <h2>{editMode === 'new-agent' ? 'New Agent' : `Edit: ${editAgent.name}`}</h2>
            <div class="editor-actions">
              <Button variant="secondary" onclick={() => { editMode = 'view'; editError = null; }}>Cancel</Button>
              <Button variant="primary" onclick={saveAgentForm}>
                <Save size={13} /> Save
              </Button>
            </div>
          </div>

          {#if editError}
            <div class="editor-error"><AlertCircle size={13} />{editError}</div>
          {/if}

          <div class="editor-form">
            <div class="form-row">
              <label>Name
                <input type="text" bind:value={editAgent.name} placeholder="my-agent" disabled={editMode === 'edit-agent'} />
              </label>
            </div>
            <div class="form-row">
              <label>Description
                <input type="text" bind:value={editAgent.description} placeholder="What this agent does" />
              </label>
            </div>
            <div class="form-row">
              <label>Model
                <Select
                  value={editAgent.model}
                  options={[
                    { value: 'opus', label: 'Opus' },
                    { value: 'sonnet', label: 'Sonnet' },
                    { value: 'haiku', label: 'Haiku' },
                    { value: 'inherit', label: 'Inherit' },
                  ]}
                  onchange={(v) => { editAgent!.model = v; }}
                />
              </label>
            </div>
            <div class="form-row">
              <label>Permission Mode
                <Select
                  value={editAgent.permission_mode}
                  options={[
                    { value: 'default', label: 'Default' },
                    { value: 'plan', label: 'Plan' },
                    { value: 'acceptEdits', label: 'Accept Edits' },
                    { value: 'bypassPermissions', label: 'Bypass' },
                  ]}
                  onchange={(v) => { editAgent!.permission_mode = v; }}
                />
              </label>
            </div>
            <div class="form-row">
              <label>Tools
                <div class="tool-grid">
                  {#each ALL_TOOLS as tool}
                    <label class="tool-check">
                      <input
                        type="checkbox"
                        checked={editAgent.tools.includes(tool)}
                        onchange={(e: Event) => {
                          const target = e.target as HTMLInputElement;
                          if (target.checked) editAgent!.tools = [...editAgent!.tools, tool];
                          else editAgent!.tools = editAgent!.tools.filter((t) => t !== tool);
                        }}
                      />
                      {tool}
                    </label>
                  {/each}
                </div>
              </label>
            </div>
            <div class="form-row">
              <label>Memory
                <Select
                  value={editAgent.memory ?? ''}
                  options={[
                    { value: '', label: 'None' },
                    { value: 'user', label: 'User' },
                    { value: 'project', label: 'Project' },
                  ]}
                  onchange={(v) => { editAgent!.memory = v || null; }}
                />
              </label>
            </div>
            <div class="form-row">
              <label>System Prompt
                <textarea bind:value={editAgent.system_prompt} rows={12} placeholder="Instructions for this agent..."></textarea>
              </label>
            </div>
          </div>
        </div>
      {/if}

    {:else if selectedWeplexAgent || (!selectedAgent && !selectedWeplexAgent)}
      <WeplexAgentDetail
        agent={selectedWeplexAgent}
        bind:this={weplexAgentEditor}
        onSaved={async () => {
          await loadAll();
          if (selectedWeplexAgent)
            selectedWeplexAgent = weplexAgents.find((a) => a.name === selectedWeplexAgent!.name) || weplexAgents[0] || null;
        }}
        onDeleted={async () => {
          selectedWeplexAgent = null;
          await loadAll();
        }}
      />
    {:else if selectedAgent}
      <AgentDetail agent={selectedAgent} onEdit={startEditAgent} onDelete={deleteCurrentAgent} />
    {/if}
  </div>
</div>

{#if confirmDelete}
  <Modal onclose={() => (confirmDelete = null)} position="center" label="Confirm deletion" class="confirm-dialog">
    <p class="confirm-text">Delete agent <strong>{confirmDelete.name}</strong>?</p>
    <p class="confirm-hint">This cannot be undone.</p>
    <div class="confirm-actions">
      <Button variant="secondary" onclick={() => (confirmDelete = null)}>Cancel</Button>
      <Button variant="danger" onclick={confirmDeleteAction}>Delete</Button>
    </div>
  </Modal>
{/if}

<style>
  .agents-layout {
    display: flex;
    width: 100%;
    height: 100%;
    background: var(--weplex-bg);
    overflow: hidden;
  }

  /* Sidebar */
  .agents-sidebar {
    width: 240px;
    min-width: 240px;
    display: flex;
    flex-direction: column;
    border-right: 1px solid var(--weplex-border);
    background: var(--weplex-sidebar-bg);
    position: relative;
  }

  .agents-sidebar::before {
    content: '';
    position: absolute;
    inset: 0;
    background-image: radial-gradient(circle, rgba(255, 255, 255, 0.06) 0.5px, transparent 0.5px);
    background-size: 12px 12px;
    pointer-events: none;
  }

  .agents-sidebar-header {
    padding: 16px 14px 12px;
    flex-shrink: 0;
  }

  .agents-sidebar-title {
    font-size: var(--weplex-text-md);
    font-weight: 600;
    margin: 0;
  }

  .agents-nav {
    flex: 1;
    overflow-y: auto;
    padding: 0;
  }

  .agents-loading, .agents-empty {
    padding: 16px;
    color: var(--weplex-text-muted);
    font-size: 12px;
    line-height: 1.5;
  }

  .nav-section-label {
    font-size: 9px;
    font-weight: 600;
    color: var(--weplex-text-muted);
    letter-spacing: 0.06em;
    text-transform: uppercase;
    padding: 10px 16px 4px;
    opacity: 0.6;
  }

  .agent-row {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 6px 14px 6px 16px;
    border: none;
    background: transparent;
    cursor: pointer;
    text-align: left;
    transition: background var(--weplex-duration-fast);
  }
  .agent-row:hover { background: var(--weplex-surface-hover); }
  .agent-row.selected { background: color-mix(in srgb, var(--weplex-accent) 10%, transparent); }

  .row-letter {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    border-radius: 6px;
    flex-shrink: 0;
    font-size: 11px;
    font-weight: 700;
    font-family: var(--weplex-font-mono);
    background: var(--weplex-surface-hover);
    color: var(--weplex-text-muted);
  }
  .row-letter.opus { background: color-mix(in srgb, var(--weplex-model-opus) 15%, transparent); color: var(--weplex-model-opus); }
  .row-letter.sonnet { background: color-mix(in srgb, var(--weplex-model-sonnet) 15%, transparent); color: var(--weplex-model-sonnet); }
  .row-letter.haiku { background: color-mix(in srgb, var(--weplex-model-haiku) 15%, transparent); color: var(--weplex-model-haiku); }
  .row-letter.weplex { background: color-mix(in srgb, var(--weplex-active) 12%, transparent); color: var(--weplex-active); }

  .row-name {
    flex: 1;
    font-size: 13px;
    font-weight: 500;
    color: var(--weplex-text);
    font-family: var(--weplex-font-mono);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .agent-row.selected .row-name { color: var(--weplex-accent); }

  .row-model {
    font-size: 10px;
    font-weight: 600;
    font-family: var(--weplex-font-mono);
    opacity: 0.45;
    flex-shrink: 0;
    transition: opacity var(--weplex-duration-fast);
  }
  .agent-row:hover .row-model, .agent-row.selected .row-model { opacity: 1; }
  .row-model.opus { color: var(--weplex-model-opus); }
  .row-model.sonnet { color: var(--weplex-model-sonnet); }
  .row-model.haiku { color: var(--weplex-model-haiku); }

  .agents-footer {
    padding: 8px;
    border-top: 1px solid var(--weplex-border);
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .agents-action-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 7px 10px;
    border: 1px dashed var(--weplex-border);
    border-radius: var(--weplex-radius-md);
    background: transparent;
    color: var(--weplex-text-muted);
    font-size: 12px;
    cursor: pointer;
    transition: all var(--weplex-duration-fast);
  }
  .agents-action-btn:hover {
    border-color: var(--weplex-accent);
    color: var(--weplex-accent);
    border-style: solid;
  }

  /* Main */
  .agents-main {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    overflow: hidden;
  }

  .agents-center-msg {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--weplex-text-muted);
    font-size: 13px;
  }

  /* Editor */
  .editor {
    flex: 1;
    overflow-y: auto;
    padding: 24px 32px;
    display: flex;
    flex-direction: column;
  }
  .editor-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 20px;
  }
  .editor-header h2 {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 15px;
    font-weight: 700;
    color: var(--weplex-text);
    font-family: var(--weplex-font-mono);
    margin: 0;
  }
  .editor-actions { display: flex; gap: 6px; }
  .editor-error {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 16px;
    padding: 8px 12px;
    border-radius: 6px;
    background: color-mix(in srgb, var(--weplex-error) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--weplex-error) 20%, transparent);
    color: var(--weplex-error);
    font-size: 12px;
  }
  .editor-form { display: flex; flex-direction: column; gap: 14px; }
  .form-row { display: flex; flex-direction: column; gap: 5px; }
  .form-row label {
    display: flex;
    flex-direction: column;
    gap: 5px;
    font-size: 11px;
    font-weight: 600;
    color: var(--weplex-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .form-row input[type='text'], .form-row textarea {
    width: 100%;
    padding: 8px 10px;
    border: 1px solid var(--weplex-border);
    border-radius: 6px;
    background: var(--weplex-surface);
    color: var(--weplex-text);
    font-size: 13px;
    font-family: var(--weplex-font-mono);
    outline: none;
    transition: border-color var(--weplex-duration-fast);
  }
  .form-row input:focus, .form-row textarea:focus { border-color: var(--weplex-accent); }
  .form-row textarea { resize: vertical; line-height: 1.5; }
  .form-row input:disabled { opacity: 0.5; cursor: not-allowed; }

  .tool-grid { display: flex; flex-wrap: wrap; gap: 4px 12px; }
  .tool-check {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 12px;
    color: var(--weplex-text-secondary);
    font-family: var(--weplex-font-mono);
    cursor: pointer;
  }
  .tool-check input { cursor: pointer; accent-color: var(--weplex-accent); }

  /* Delete dialog */
  :global(.confirm-dialog) {
    width: 340px;
    padding: 20px;
    background: var(--weplex-surface);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-xl);
    box-shadow: var(--weplex-shadow-overlay);
  }
  .confirm-text { font-size: 14px; margin: 0 0 6px; }
  .confirm-hint { font-size: 12px; color: var(--weplex-text-muted); margin: 0 0 16px; }
  .confirm-actions { display: flex; gap: 8px; justify-content: flex-end; }
</style>

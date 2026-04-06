<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { Select, Button, Modal } from '../ui';
  import { uiStore } from '../../stores/uiStore.svelte';
  import { sessionStore } from '../../stores/sessionStore.svelte';
  import { pipelineRunStore } from '../../stores/pipelineRunStore.svelte';
  import { pipelineInjectStore } from '../../stores/pipelineInjectStore.svelte';
  import { teamStore } from '../../stores/teamStore.svelte';
  import { collabPipelineStore } from '../../stores/collabPipelineStore.svelte';
  import { Users } from 'lucide-svelte';
  import { modelShort, modelClass, initial, shortenPath, getMissingAgents } from './helpers';
  import AgentDetail from './AgentDetail.svelte';
  import WeplexAgentDetail from './WeplexAgentDetail.svelte';
  import PipelineFlowEditor from './PipelineFlowEditor.svelte';
  import ProjectTab from './ProjectTab.svelte';
  import type {
    AgentConfig,
    PipelineConfig,
    PipelineStage,
    ProjectConfig,
    SkillInfo,
  } from './types';
  import {
    X,
    ChevronDown,
    Plus,
    Trash2,
    Play,
    Save,
    FileText,
    Workflow,
    FolderOpen,
    AlertCircle,
  } from 'lucide-svelte';

  // ── State ──────────────────────────────────────────────────────────────
  let activeTab = $state<'agents' | 'pipelines' | 'project'>('agents');
  let selectedAgent = $state<AgentConfig | null>(null);
  let selectedWeplexAgent = $state<(typeof weplexAgents)[0] | null>(null);
  let selectedPipeline = $state<PipelineConfig | null>(null);
  let weplexAgentEditor = $state<WeplexAgentDetail>();
  let agents = $state<AgentConfig[]>([]);
  let weplexAgents = $state<
    {
      name: string;
      description: string;
      binary: string;
      model: string;
      prompt: string;
      file_path: string;
    }[]
  >([]);
  let pipelines = $state<PipelineConfig[]>([]);
  let skills = $state<SkillInfo[]>([]);
  let projectAgents = $state<AgentConfig[]>([]);
  let projectConfig = $state<ProjectConfig | null>(null);
  let loading = $state(true);

  // Editor state
  let editMode = $state<'view' | 'edit-agent' | 'new-agent' | 'run-pipeline'>('view');
  let editAgent = $state<AgentConfig | null>(null);
  let editError = $state<string | null>(null);

  // Run pipeline state
  let runPipelineCwd = $state('~');
  let runPipelineTask = $state('');

  // Owner assignment for collaborative runs
  let stageOwners = $state<Record<string, string>>({});
  let team = $derived(teamStore.activeTeam ?? null);
  let teamMembers = $derived(team?.members ?? []);
  let hasAnyOwner = $derived(Object.values(stageOwners).some((v) => v !== ''));
  let isCollabMode = $derived(hasAnyOwner && team !== null);

  // Pipeline flow editor ref
  let pipelineEditor = $state<PipelineFlowEditor>();

  let sidebarWidth = $derived(Math.max(uiStore.sidebarWidthRaw, 260));
  let activeSession = $derived(sessionStore.activeSession);

  $effect(() => {
    if (activeSession?.cwd) {
      loadProjectData(activeSession.cwd);
    } else {
      projectConfig = null;
      projectAgents = [];
    }
  });

  // Escape to close
  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      if (editMode !== 'view') {
        editMode = 'view';
        editError = null;
      } else close();
    }
  }
  onMount(() => {
    window.addEventListener('keydown', handleKeydown);
    loadAll();
    return () => window.removeEventListener('keydown', handleKeydown);
  });

  async function loadAll() {
    loading = true;
    try {
      const [a, p, s, da] = await Promise.all([
        invoke<AgentConfig[]>('list_agents'),
        invoke<PipelineConfig[]>('list_pipelines'),
        invoke<SkillInfo[]>('list_skills'),
        invoke<typeof weplexAgents>('list_weplex_agents'),
      ]);
      agents = a;
      pipelines = p;
      skills = s;
      weplexAgents = da;
      if (agents.length > 0 && !selectedAgent) selectedAgent = agents[0];
      if (activeSession?.cwd) await loadProjectData(activeSession.cwd);
    } catch (e) {
      console.error('Failed to load:', e);
    } finally {
      loading = false;
    }
  }

  async function loadProjectData(cwd: string) {
    try {
      const [config, pAgents] = await Promise.all([
        invoke<ProjectConfig>('get_project_config', { cwd }),
        invoke<AgentConfig[]>('list_project_agents', { cwd }),
      ]);
      projectConfig = config;
      projectAgents = pAgents;
    } catch {
      projectConfig = null;
      projectAgents = [];
    }
  }

  function close() {
    uiStore.closeOverlay();
  }

  function selectAgent(agent: AgentConfig) {
    activeTab = 'agents';
    selectedAgent = agent;
    selectedWeplexAgent = null;
    selectedPipeline = null;
    editMode = 'view';
  }

  function selectWeplexAgent(agent: (typeof weplexAgents)[0]) {
    activeTab = 'agents';
    selectedWeplexAgent = agent;
    selectedAgent = null;
    selectedPipeline = null;
    editMode = 'view';
  }

  function startNewWeplexAgent() {
    selectedAgent = null;
    selectedWeplexAgent = null;
    activeTab = 'agents';
    editMode = 'view';
    requestAnimationFrame(() => weplexAgentEditor?.startNew());
  }

  function selectPipeline(pipeline: PipelineConfig) {
    activeTab = 'pipelines';
    selectedPipeline = pipeline;
    selectedAgent = null;
    selectedWeplexAgent = null;
    editMode = 'view';
  }

  let sortedAgents = $derived([...agents].sort((a, b) => a.name.localeCompare(b.name)));

  let agentNameSet = $derived(
    new Set([...agents.map((a) => a.name), ...weplexAgents.map((a) => a.name)]),
  );

  // ── Agent CRUD ─────────────────────────────────────────────────────────
  function startNewAgent() {
    editAgent = {
      name: '',
      description: '',
      model: 'sonnet',
      tools: ['Read', 'Grep', 'Glob', 'Edit', 'Write', 'Bash'],
      disallowed_tools: [],
      permission_mode: 'default',
      memory: null,
      max_turns: null,
      background: null,
      isolation: null,
      skills: [],
      system_prompt: '',
      file_path: '',
      source: 'user',
    };
    editMode = 'new-agent';
    editError = null;
    activeTab = 'agents';
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
    if (!editAgent.name.trim()) {
      editError = 'Name is required';
      return;
    }
    if (!editAgent.description.trim()) {
      editError = 'Description is required';
      return;
    }
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

  let confirmDelete = $state<{ type: 'agent' | 'pipeline'; name: string } | null>(null);

  async function deleteCurrentAgent() {
    if (!selectedAgent) return;
    confirmDelete = { type: 'agent', name: selectedAgent.name };
  }

  async function deleteCurrentPipeline() {
    if (!selectedPipeline) return;
    confirmDelete = { type: 'pipeline', name: selectedPipeline.name };
  }

  async function confirmDeleteAction() {
    if (!confirmDelete) return;
    try {
      if (confirmDelete.type === 'agent') {
        await invoke('delete_agent', { name: confirmDelete.name });
        selectedAgent = null;
      } else {
        if (selectedPipeline) {
          await invoke('delete_pipeline', { filePath: selectedPipeline.file_path });
          selectedPipeline = null;
        }
      }
      await loadAll();
      if (confirmDelete.type === 'agent' && agents.length > 0) selectedAgent = agents[0];
      if (confirmDelete.type === 'pipeline' && pipelines.length > 0) {
        selectedPipeline = pipelines[0];
      }
    } catch (e: unknown) {
      console.error('Delete failed:', e);
    } finally {
      confirmDelete = null;
    }
  }

  // ── Pipeline: new ──────────────────────────────────────────────────────
  function startNewPipeline() {
    selectedPipeline = null;
    selectedAgent = null;
    activeTab = 'pipelines';
    editMode = 'view';
    editError = null;
    // Defer to next tick so binding is ready
    requestAnimationFrame(() => pipelineEditor?.startNew());
  }

  // ── Pipeline Runner ────────────────────────────────────────────────────
  let runPipelineLaunching = $state(false);

  function startRunPipeline(pipeline?: PipelineConfig) {
    if (pipeline) selectedPipeline = pipeline;
    if (!selectedPipeline) return;
    runPipelineCwd = activeSession?.cwd || '~';
    runPipelineTask = '';
    editMode = 'run-pipeline';
    editError = null;

    // Initialize owner assignments from YAML owner fields
    const owners: Record<string, string> = {};
    for (const stage of selectedPipeline.stages) {
      if (stage.parallel) {
        for (const ps of stage.parallel) {
          const key = ps.name || ps.agent || '';
          owners[key] = ps.owner || '';
        }
      } else {
        const key = stage.name || stage.agent || '';
        owners[key] = stage.owner || '';
      }
    }
    stageOwners = owners;
  }

  async function launchPipelineEngine() {
    if (!selectedPipeline || !runPipelineTask.trim()) {
      editError = 'Task description is required';
      return;
    }
    runPipelineLaunching = true;
    editError = null;
    try {
      if (isCollabMode) {
        // Launch as collaborative run via relay
        const stageDefs = selectedPipeline.stages.flatMap((stage) => {
          if (stage.parallel) {
            return stage.parallel.map((ps) => {
              const key = ps.name || ps.agent || '';
              return {
                name: key,
                agent: ps.agent || undefined,
                role: ps.role || undefined,
                receives: ps.receives || [],
                optional: ps.optional || undefined,
                ownerEmail: stageOwners[key] || undefined,
              };
            });
          }
          const key = stage.name || stage.agent || '';
          return [{
            name: key,
            agent: stage.agent || undefined,
            role: stage.role || undefined,
            receives: stage.receives || [],
            optional: stage.optional || undefined,
            ownerEmail: stageOwners[key] || undefined,
          }];
        });
        await collabPipelineStore.startCollabRun(
          selectedPipeline.name,
          runPipelineTask,
          stageDefs,
        );
      } else {
        // Solo pipeline run
        await pipelineRunStore.startRun(selectedPipeline.file_path, runPipelineTask, runPipelineCwd, undefined, undefined);
      }
      editMode = 'view';
      close();
    } catch (e: unknown) {
      editError = e instanceof Error ? e.message : String(e);
    } finally {
      runPipelineLaunching = false;
    }
  }

  const ALL_TOOLS = [
    'Read',
    'Grep',
    'Glob',
    'Edit',
    'Write',
    'Bash',
    'Agent',
    'WebFetch',
    'WebSearch',
    'NotebookEdit',
  ];

  // ── Pipeline saved callback ─────────────────────────────────────────────
  async function onPipelineSaved() {
    await loadAll();
  }
</script>

<!-- ═══ Sidebar ═══ -->
<aside class="ap-sidebar" style="width: {sidebarWidth}px; min-width: {sidebarWidth}px">
  <div class="traffic-light-area" data-tauri-drag-region></div>

  <div class="ap-tabs">
    <button
      class="ap-tab"
      class:active={activeTab === 'agents'}
      onclick={() => {
        activeTab = 'agents';
        editMode = 'view';
      }}
    >
      <FileText size={13} />
      Agents {#if agents.length}<span class="ap-tab-count">{agents.length}</span>{/if}
    </button>
    <button
      class="ap-tab"
      class:active={activeTab === 'pipelines'}
      onclick={() => {
        activeTab = 'pipelines';
        editMode = 'view';
      }}
    >
      <Workflow size={13} />
      Pipelines {#if pipelines.length}<span class="ap-tab-count">{pipelines.length}</span>{/if}
    </button>
    <button
      class="ap-tab"
      class:active={activeTab === 'project'}
      onclick={() => {
        activeTab = 'project';
        editMode = 'view';
      }}
    >
      <FolderOpen size={13} />
      Project
      {#if projectConfig?.exists || projectAgents.length > 0}<span class="ap-tab-dot"></span>{/if}
    </button>
  </div>

  <nav class="ap-nav">
    {#if loading}
      <div class="ap-loading">Loading...</div>
    {:else if activeTab === 'agents'}
      <!-- Weplex agents (YAML, agent-agnostic) -->
      {#if weplexAgents.length > 0}
        <div class="nav-section-label">Weplex Agents</div>
        {#each weplexAgents as da}
          <button
            class="agent-row"
            class:selected={selectedWeplexAgent?.name === da.name && editMode === 'view'}
            onclick={() => selectWeplexAgent(da)}
          >
            <span class="row-letter weplex">D</span>
            <span class="row-name">{da.name}</span>
            <span class="row-model">{da.binary}</span>
          </button>
        {/each}
      {/if}
      <!-- Claude agents (.md) -->
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
    {:else if activeTab === 'pipelines'}
      {#each pipelines as pipeline}
        <button
          class="agent-row"
          class:selected={selectedPipeline?.name === pipeline.name && editMode === 'view'}
          onclick={() => selectPipeline(pipeline)}
        >
          <span class="row-letter pipeline">
            <Workflow size={12} />
          </span>
          <span class="row-name">{pipeline.name}</span>
          <span class="row-stages">{pipeline.stages.length}</span>
        </button>
      {/each}
      {#if pipelines.length === 0}
        <div class="ap-empty-hint">No pipelines yet. Create one to orchestrate your agents.</div>
      {/if}
    {:else}
      <!-- Project tab uses main area -->
    {/if}
  </nav>

  <div class="ap-footer">
    {#if activeTab === 'agents'}
      <button class="ap-action-btn" onclick={startNewWeplexAgent}>
        <Plus size={13} />
        <span>New Weplex Agent</span>
      </button>
      <button class="ap-action-btn" onclick={startNewAgent}>
        <Plus size={13} />
        <span>New Claude Agent</span>
      </button>
    {:else if activeTab === 'pipelines'}
      <button class="ap-action-btn" onclick={startNewPipeline}>
        <Plus size={13} />
        <span>New Pipeline</span>
      </button>
    {/if}
    <button class="ap-close" onclick={close} title="Close (Esc)">
      <X size={14} />
      <span>Close</span>
      <kbd>Esc</kbd>
    </button>
  </div>
</aside>

<!-- ═══ Main content ═══ -->
<div class="ap-main">
  {#if loading}
    <div class="ap-center-msg">Loading...</div>

    <!-- ── Agent Editor ─────────────────────────────────────────────── -->
  {:else if editMode === 'new-agent' || editMode === 'edit-agent'}
    {#if editAgent}
      <div class="editor">
        <div class="editor-header">
          <h2>{editMode === 'new-agent' ? 'New Agent' : `Edit: ${editAgent.name}`}</h2>
          <div class="editor-actions">
            <Button variant="secondary" onclick={() => {
                editMode = 'view';
                editError = null;
              }}>Cancel</Button>
            <Button variant="primary" onclick={saveAgentForm}>
              <Save size={13} />
              Save
            </Button>
          </div>
        </div>

        {#if editError}
          <div class="editor-error"><AlertCircle size={13} />{editError}</div>
        {/if}

        <div class="editor-form">
          <div class="form-row">
            <label
              >Name
              <input
                type="text"
                bind:value={editAgent.name}
                placeholder="my-agent"
                disabled={editMode === 'edit-agent'}
              />
            </label>
          </div>
          <div class="form-row">
            <label
              >Description
              <input
                type="text"
                bind:value={editAgent.description}
                placeholder="What this agent does"
              />
            </label>
          </div>
          <div class="form-row">
            <label
              >Model
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
            <label
              >Permission Mode
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
            <label
              >Tools
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
            <label
              >Memory
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
            <label
              >System Prompt
              <textarea
                bind:value={editAgent.system_prompt}
                rows={12}
                placeholder="Instructions for this agent..."
              ></textarea>
            </label>
          </div>
        </div>
      </div>
    {/if}

    <!-- ── Pipeline Runner ──────────────────────────────────────────── -->
  {:else if editMode === 'run-pipeline' && selectedPipeline}
    {@const runnerMissing = getMissingAgents(selectedPipeline.stages, agentNameSet)}
    <div class="editor">
      <div class="editor-header">
        <h2><Play size={15} /> Run: {selectedPipeline.name}</h2>
        <div class="editor-actions">
          <Button variant="secondary" onclick={() => {
              editMode = 'view';
              editError = null;
            }}>Cancel</Button>
          <Button
            variant="primary"
            onclick={launchPipelineEngine}
            disabled={runPipelineLaunching || !runPipelineTask.trim()}
          >
            <Play size={13} />
            {runPipelineLaunching ? 'Starting...' : isCollabMode ? 'Launch Collaborative' : 'Launch Pipeline'}
          </Button>
        </div>
      </div>

      {#if editError}
        <div class="editor-error"><AlertCircle size={13} />{editError}</div>
      {/if}

      <div class="editor-form">
        <div class="form-row">
          <label
            >Working Directory
            <input type="text" bind:value={runPipelineCwd} placeholder="~/projects/my-app" />
          </label>
        </div>
        <div class="form-row">
          <label
            >Task Description
            <textarea
              bind:value={runPipelineTask}
              rows={4}
              placeholder="Describe the task for this pipeline..."
            ></textarea>
          </label>
        </div>

        <div class="form-row"><span class="form-label">Stages</span></div>
        <div class="pipeline-flow">
          {#each selectedPipeline.stages as stage, i}
            <div class="flow-node" class:parallel={!!stage.parallel}>
              {#if stage.parallel}
                <div class="flow-parallel">
                  {#each stage.parallel as ps}
                    <div class="flow-parallel-item">
                      <span class="flow-agent">{ps.agent}</span>
                      {#if ps.optional}<span class="flow-opt">opt</span>{/if}
                    </div>
                  {/each}
                </div>
              {:else}
                <span class="flow-agent">{stage.agent}</span>
                {#if stage.optional}<span class="flow-opt">opt</span>{/if}
              {/if}
            </div>
            {#if i < selectedPipeline.stages.length - 1}
              <div class="flow-arrow"><ChevronDown size={14} /></div>
            {/if}
          {/each}
        </div>

        <!-- Owner assignment for collaborative runs -->
        <div class="form-row">
          <span class="form-label"><Users size={12} /> Owner Assignment</span>
        </div>
        {#if team && teamMembers.length > 0}
          <div class="owner-grid">
            {#each selectedPipeline.stages as stage}
              {#if stage.parallel}
                {#each stage.parallel as ps}
                  {@const key = ps.name || ps.agent || ''}
                  <div class="owner-row">
                    <span class="owner-stage-name">{key}</span>
                    <Select
                      class="owner-select"
                      value={stageOwners[key] || ''}
                      options={[
                        { value: '', label: 'Unassigned' },
                        ...teamMembers.map((m) => ({
                          value: m.email,
                          label: m.displayName || m.email,
                        })),
                      ]}
                      onchange={(v) => {
                        stageOwners[key] = v;
                        stageOwners = { ...stageOwners };
                      }}
                    />
                  </div>
                {/each}
              {:else}
                {@const key = stage.name || stage.agent || ''}
                <div class="owner-row">
                  <span class="owner-stage-name">{key}</span>
                  <Select
                    class="owner-select"
                    value={stageOwners[key] || ''}
                    options={[
                      { value: '', label: 'Unassigned' },
                      ...teamMembers.map((m) => ({
                        value: m.email,
                        label: m.displayName || m.email,
                      })),
                    ]}
                    onchange={(v) => {
                      stageOwners[key] = v;
                      stageOwners = { ...stageOwners };
                    }}
                  />
                </div>
              {/if}
            {/each}
          </div>
          {#if isCollabMode}
            <div class="collab-notice info">
              <Users size={12} />
              This pipeline will run as a collaborative run via relay.
            </div>
          {:else}
            <div class="collab-notice">
              Assign owners to run collaboratively, or leave all unassigned for solo mode.
            </div>
          {/if}
        {:else}
          <div class="collab-notice muted">
            <Users size={12} />
            Create or join a team in Settings &gt; Team for collaborative pipelines.
          </div>
        {/if}

        <!-- Missing agents warning in runner -->
        {#if runnerMissing.length > 0}
          <div class="missing-warning">
            <AlertCircle size={13} />
            Missing Weplex agents: {runnerMissing.join(', ')} — create them in ~/.weplex/agents/ before
            running.
          </div>
        {/if}
      </div>
    </div>

    <!-- ── Agent Detail View ────────────────────────────────────────── -->
  {:else if activeTab === 'agents'}
    {#if selectedWeplexAgent || (!selectedAgent && !selectedWeplexAgent)}
      <WeplexAgentDetail
        agent={selectedWeplexAgent}
        bind:this={weplexAgentEditor}
        onSaved={async () => {
          await loadAll();
          if (selectedWeplexAgent)
            selectedWeplexAgent =
              weplexAgents.find((a) => a.name === selectedWeplexAgent!.name) ||
              weplexAgents[0] ||
              null;
        }}
        onDeleted={async () => {
          selectedWeplexAgent = null;
          await loadAll();
        }}
      />
    {:else if selectedAgent}
      <AgentDetail agent={selectedAgent} onEdit={startEditAgent} onDelete={deleteCurrentAgent} />
    {/if}

    <!-- ── Pipeline Detail View (Inline Editable) ──────────────────── -->
  {:else if activeTab === 'pipelines'}
    <PipelineFlowEditor
      pipeline={selectedPipeline}
      {agents}
      bind:this={pipelineEditor}
      onRunPipeline={() => startRunPipeline()}
      onDeletePipeline={deleteCurrentPipeline}
      onSaved={onPipelineSaved}
    />

    <!-- ── Project Tab ──────────────────────────────────────────────── -->
  {:else if activeTab === 'project'}
    <ProjectTab
      activeSession={activeSession?.cwd ? { cwd: activeSession.cwd } : null}
      {projectConfig}
      {projectAgents}
      {skills}
      onSelectAgent={selectAgent}
    />
  {/if}
</div>

<!-- Delete confirmation dialog -->
{#if confirmDelete}
  <Modal onclose={() => (confirmDelete = null)} position="center" label="Confirm deletion" class="confirm-dialog">
      <p class="confirm-text">Delete {confirmDelete.type} <strong>{confirmDelete.name}</strong>?</p>
      <p class="confirm-hint">This cannot be undone.</p>
      <div class="confirm-actions">
        <Button variant="secondary" onclick={() => (confirmDelete = null)}>Cancel</Button>
        <Button variant="danger" onclick={confirmDeleteAction}>Delete</Button>
      </div>
  </Modal>
{/if}

<style>
  /* ═══ Sidebar ═══════════════════════════════════════════════════════════ */
  .ap-sidebar {
    position: relative;
    height: 100%;
    background: var(--weplex-sidebar-bg);
    display: flex;
    flex-direction: column;
    flex-shrink: 0;
    overflow: hidden;
    z-index: 20;
  }
  .traffic-light-area {
    height: 38px;
    flex-shrink: 0;
    -webkit-app-region: drag;
  }

  .ap-tabs {
    display: flex;
    gap: 0;
    padding: 0 8px;
    border-bottom: 1px solid var(--weplex-border);
    flex-shrink: 0;
  }
  .ap-tab {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 8px 8px;
    border: none;
    border-bottom: 2px solid transparent;
    background: transparent;
    color: var(--weplex-text-muted);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition:
      color var(--weplex-duration-fast),
      border-color var(--weplex-duration-fast);
    margin-bottom: -1px;
    white-space: nowrap;
  }
  .ap-tab:hover {
    color: var(--weplex-text);
  }
  .ap-tab.active {
    color: var(--weplex-text);
    border-bottom-color: var(--weplex-accent);
  }
  .ap-tab-count {
    font-size: 10px;
    color: var(--weplex-text-muted);
    font-weight: 400;
  }
  .ap-tab-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--weplex-accent);
  }

  .ap-nav {
    flex: 1;
    overflow-y: auto;
    padding: 4px 0;
  }
  .ap-loading {
    padding: 24px 16px;
    color: var(--weplex-text-muted);
    font-size: 12px;
  }
  .ap-empty-hint {
    padding: 16px;
    color: var(--weplex-text-muted);
    font-size: 11px;
    line-height: 1.5;
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
  .agent-row:hover {
    background: var(--weplex-surface-hover);
  }
  .agent-row.selected {
    background: color-mix(in srgb, var(--weplex-accent) 10%, transparent);
  }

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
  .row-letter.opus {
    background: color-mix(in srgb, var(--weplex-model-opus) 15%, transparent);
    color: var(--weplex-model-opus);
  }
  .row-letter.sonnet {
    background: color-mix(in srgb, var(--weplex-model-sonnet) 15%, transparent);
    color: var(--weplex-model-sonnet);
  }
  .row-letter.haiku {
    background: color-mix(in srgb, var(--weplex-model-haiku) 15%, transparent);
    color: var(--weplex-model-haiku);
  }
  .row-letter.pipeline {
    background: color-mix(in srgb, var(--weplex-accent) 10%, transparent);
    color: var(--weplex-accent);
  }

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
  .agent-row.selected .row-name {
    color: var(--weplex-accent);
  }
  .row-model {
    font-size: 10px;
    font-weight: 600;
    font-family: var(--weplex-font-mono);
    opacity: 0.45;
    transition: opacity var(--weplex-duration-fast);
    flex-shrink: 0;
  }
  .agent-row:hover .row-model,
  .agent-row.selected .row-model {
    opacity: 1;
  }
  .row-model.opus {
    color: var(--weplex-model-opus);
  }
  .row-model.sonnet {
    color: var(--weplex-model-sonnet);
  }
  .row-model.haiku {
    color: var(--weplex-model-haiku);
  }
  .row-stages {
    font-size: 10px;
    color: var(--weplex-text-muted);
    font-family: var(--weplex-font-mono);
    opacity: 0.5;
    flex-shrink: 0;
  }
  .row-letter.deck {
    background: color-mix(in srgb, var(--weplex-active) 12%, transparent);
    color: var(--weplex-active);
    font-size: 10px;
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

  .ap-footer {
    padding: 8px;
    border-top: 1px solid var(--weplex-border);
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .ap-action-btn {
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
  .ap-action-btn:hover {
    border-color: var(--weplex-accent);
    color: var(--weplex-accent);
    border-style: solid;
  }
  .ap-close {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 7px 10px;
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-md);
    background: transparent;
    color: var(--weplex-text-muted);
    font-size: 12px;
    cursor: pointer;
    transition: all var(--weplex-duration-fast);
  }
  .ap-close:hover {
    border-color: var(--weplex-accent);
    color: var(--weplex-text);
  }
  .ap-close kbd {
    margin-left: auto;
    font-size: 10px;
    font-family: var(--weplex-font-mono);
    opacity: 0.5;
  }

  /* ═══ Main ══════════════════════════════════════════════════════════════ */
  .ap-main {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    position: relative;
    margin: 6px;
    border-radius: 10px;
    box-shadow:
      0 0 0 1px rgba(0, 0, 0, 0.4),
      0 2px 8px rgba(0, 0, 0, 0.3);
    background: var(--weplex-bg);
    overflow: hidden;
  }
  .ap-center-msg {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--weplex-text-muted);
    font-size: 13px;
  }

  /* ── Editor ─────────────────────────────────────────────────────── */
  .editor {
    flex: 1;
    overflow-y: auto;
    padding: 24px 32px;
    display: flex;
    flex-direction: column;
    gap: 0;
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
  .editor-actions {
    display: flex;
    gap: 6px;
  }
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
  .editor-form {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .form-row {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .form-row label,
  .form-row .form-label {
    display: flex;
    flex-direction: column;
    gap: 5px;
    font-size: 11px;
    font-weight: 600;
    color: var(--weplex-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .form-row input[type='text'],
  .form-row textarea,
  .form-row select {
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
  .form-row input:focus,
  .form-row textarea:focus,
  .form-row select:focus {
    border-color: var(--weplex-accent);
  }
  .form-row textarea {
    resize: vertical;
    line-height: 1.5;
  }
  .form-row select {
    cursor: pointer;
  }
  .form-row input:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .tool-grid {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 12px;
  }
  .tool-check {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 12px;
    color: var(--weplex-text-secondary);
    font-family: var(--weplex-font-mono);
    cursor: pointer;
  }
  .tool-check input {
    cursor: pointer;
    accent-color: var(--weplex-accent);
  }

  .generated-preview {
    font-size: 11px;
    font-family: var(--weplex-font-mono);
    color: var(--weplex-text-secondary);
    line-height: 1.5;
    background: var(--weplex-surface);
    border: 1px solid var(--weplex-border);
    border-radius: 6px;
    padding: 12px 14px;
    white-space: pre-wrap;
    word-break: break-word;
    max-height: 300px;
    overflow-y: auto;
  }

  /* ── Pipeline flow (runner read-only) ────────────────────────── */
  .pipeline-flow {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0;
  }
  .flow-node {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 14px;
    border: 1px solid var(--weplex-border);
    border-radius: 8px;
    background: var(--weplex-surface);
    min-width: 200px;
  }
  .flow-node.parallel {
    background: transparent;
    border-style: dashed;
    border-color: color-mix(in srgb, var(--weplex-accent) 20%, transparent);
  }
  .flow-parallel {
    display: flex;
    flex-direction: column;
    gap: 6px;
    width: 100%;
  }
  .flow-parallel-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border-radius: 6px;
    background: var(--weplex-surface);
    border: 1px solid var(--weplex-border);
  }
  .flow-agent {
    font-size: 12px;
    font-weight: 600;
    color: var(--weplex-text);
    font-family: var(--weplex-font-mono);
  }
  .flow-opt {
    font-size: 9px;
    color: var(--weplex-text-muted);
    border: 1px solid var(--weplex-border);
    border-radius: 3px;
    padding: 0 4px;
    flex-shrink: 0;
  }
  .flow-arrow {
    color: var(--weplex-text-muted);
    opacity: 0.4;
    padding: 2px 0;
  }
  .missing-warning {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 16px;
    padding: 8px 12px;
    border-radius: 6px;
    background: color-mix(in srgb, var(--weplex-error) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--weplex-error) 20%, transparent);
    color: var(--weplex-error);
    font-size: 12px;
  }

  /* Owner assignment */
  .owner-grid {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-bottom: 12px;
  }

  .owner-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .owner-stage-name {
    flex: 1;
    font-size: 12px;
    font-family: var(--weplex-font-mono);
    color: var(--weplex-text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .owner-select {
    width: 180px;
    padding: 4px 8px;
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-sm);
    background: var(--weplex-bg);
    color: var(--weplex-text);
    font-size: 11px;
    outline: none;
  }

  .owner-select:focus {
    border-color: var(--weplex-accent);
  }

  .collab-notice {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    border-radius: 6px;
    font-size: 11px;
    color: var(--weplex-text-muted);
    margin-bottom: 8px;
  }

  .collab-notice.info {
    background: color-mix(in srgb, var(--weplex-info) 8%, transparent);
    border: 1px solid color-mix(in srgb, var(--weplex-info) 20%, transparent);
    color: var(--weplex-info);
  }

  .collab-notice.muted {
    background: var(--weplex-surface);
    border: 1px solid var(--weplex-border);
  }

  /* ── Confirm dialog ──────────────────────────────────────────────── */
  :global(.confirm-dialog) {
    background: var(--weplex-surface);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-xl);
    padding: 20px 24px;
    box-shadow: var(--weplex-shadow-overlay);
    max-width: 360px;
    width: 100%;
  }
  .confirm-text {
    margin: 0 0 4px;
    font-size: 13px;
    color: var(--weplex-text);
  }
  .confirm-hint {
    font-size: 11px;
    color: var(--weplex-text-muted);
    margin-bottom: 16px;
  }
  .confirm-actions {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
  }
</style>

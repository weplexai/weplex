<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { Select, Button, Modal } from '../ui';
  import { uiStore } from '../../stores/uiStore.svelte';
  import { sessionStore } from '../../stores/sessionStore.svelte';
  import { pipelineRunStore } from '../../stores/pipelineRunStore.svelte';
  import { collabPipelineStore } from '../../stores/collabPipelineStore.svelte';
  import { teamStore } from '../../stores/teamStore.svelte';
  import { Plus, Play, Workflow, ChevronDown, AlertCircle, Users } from 'lucide-svelte';
  import { getMissingAgents } from '../overlays/helpers';
  import PipelineFlowEditor from '../overlays/PipelineFlowEditor.svelte';
  import type { AgentConfig, PipelineConfig, SkillInfo } from '../overlays/types';

  // State
  let selectedPipeline = $state<PipelineConfig | null>(null);
  let pipelines = $state<PipelineConfig[]>([]);
  let agents = $state<AgentConfig[]>([]);
  let loading = $state(true);

  // Editor
  let editMode = $state<'view' | 'run-pipeline'>('view');
  let editError = $state<string | null>(null);
  let pipelineEditor = $state<PipelineFlowEditor>();

  // Run pipeline state
  let runPipelineCwd = $state('~');
  let runPipelineTask = $state('');
  let runPipelineLaunching = $state(false);

  // Owner assignment
  let stageOwners = $state<Record<string, string>>({});
  let team = $derived(teamStore.activeTeam ?? null);
  let teamMembers = $derived(team?.members ?? []);
  let hasAnyOwner = $derived(Object.values(stageOwners).some((v) => v !== ''));
  let isCollabMode = $derived(hasAnyOwner && team !== null);

  let activeSession = $derived(sessionStore.activeSession);
  let agentNameSet = $derived(new Set(agents.map((a) => a.name)));

  onMount(() => { loadAll(); });

  async function loadAll() {
    loading = true;
    try {
      const [p, a] = await Promise.all([
        invoke<PipelineConfig[]>('list_pipelines'),
        invoke<AgentConfig[]>('list_agents'),
      ]);
      pipelines = p;
      agents = a;
      if (pipelines.length > 0 && !selectedPipeline) selectedPipeline = pipelines[0];
    } catch (e) {
      console.error('Failed to load pipelines:', e);
    } finally {
      loading = false;
    }
  }

  function selectPipeline(pipeline: PipelineConfig) {
    selectedPipeline = pipeline;
    editMode = 'view';
  }

  function startNewPipeline() {
    selectedPipeline = null;
    editMode = 'view';
    editError = null;
    requestAnimationFrame(() => pipelineEditor?.startNew());
  }

  function startRunPipeline(pipeline?: PipelineConfig) {
    if (pipeline) selectedPipeline = pipeline;
    if (!selectedPipeline) return;
    runPipelineCwd = activeSession?.cwd || '~';
    runPipelineTask = '';
    editMode = 'run-pipeline';
    editError = null;

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
        const stageDefs = selectedPipeline.stages.flatMap((stage) => {
          if (stage.parallel) {
            return stage.parallel.map((ps) => {
              const key = ps.name || ps.agent || '';
              return {
                name: key, agent: ps.agent || undefined, role: ps.role || undefined,
                receives: ps.receives || [], optional: ps.optional || undefined,
                ownerEmail: stageOwners[key] || undefined,
              };
            });
          }
          const key = stage.name || stage.agent || '';
          return [{
            name: key, agent: stage.agent || undefined, role: stage.role || undefined,
            receives: stage.receives || [], optional: stage.optional || undefined,
            ownerEmail: stageOwners[key] || undefined,
          }];
        });
        await collabPipelineStore.startCollabRun(selectedPipeline.name, runPipelineTask, stageDefs);
      } else {
        await pipelineRunStore.startRun(selectedPipeline.file_path, runPipelineTask, runPipelineCwd, undefined, undefined);
      }
      editMode = 'view';
      uiStore.exitHubMode();
    } catch (e: unknown) {
      editError = e instanceof Error ? e.message : String(e);
    } finally {
      runPipelineLaunching = false;
    }
  }

  let confirmDelete = $state<{ name: string } | null>(null);

  function deleteCurrentPipeline() {
    if (!selectedPipeline) return;
    confirmDelete = { name: selectedPipeline.name };
  }

  async function confirmDeleteAction() {
    if (!confirmDelete || !selectedPipeline) return;
    try {
      await invoke('delete_pipeline', { filePath: selectedPipeline.file_path });
      selectedPipeline = null;
      await loadAll();
      if (pipelines.length > 0) selectedPipeline = pipelines[0];
    } catch (e) {
      console.error('Delete failed:', e);
    } finally {
      confirmDelete = null;
    }
  }

  async function onPipelineSaved() {
    await loadAll();
  }
</script>

<div class="pipelines-layout">
  <!-- Sidebar: pipeline list -->
  <div class="pipelines-sidebar">
    <div class="pipelines-sidebar-header">
      <h3 class="pipelines-sidebar-title">Pipelines</h3>
    </div>

    <nav class="pipelines-nav">
      {#if loading}
        <div class="pipelines-loading">Loading...</div>
      {:else}
        {#each pipelines as pipeline}
          <button
            class="pipeline-row"
            class:selected={selectedPipeline?.name === pipeline.name && editMode === 'view'}
            onclick={() => selectPipeline(pipeline)}
          >
            <span class="row-letter pipeline"><Workflow size={12} /></span>
            <span class="row-name">{pipeline.name}</span>
            <span class="row-stages">{pipeline.stages.length}</span>
          </button>
        {/each}
        {#if pipelines.length === 0}
          <div class="pipelines-empty">No pipelines yet. Create one to orchestrate your agents.</div>
        {/if}
      {/if}
    </nav>

    <div class="pipelines-footer">
      <button class="pipelines-action-btn" onclick={startNewPipeline}>
        <Plus size={13} />
        <span>New Pipeline</span>
      </button>
    </div>
  </div>

  <!-- Main content -->
  <div class="pipelines-main">
    {#if loading}
      <div class="pipelines-center-msg">Loading...</div>

    {:else if editMode === 'run-pipeline' && selectedPipeline}
      {@const runnerMissing = getMissingAgents(selectedPipeline.stages, agentNameSet)}
      <div class="editor">
        <div class="editor-header">
          <h2><Play size={15} /> Run: {selectedPipeline.name}</h2>
          <div class="editor-actions">
            <Button variant="secondary" onclick={() => { editMode = 'view'; editError = null; }}>Cancel</Button>
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
            <label>Working Directory
              <input type="text" bind:value={runPipelineCwd} placeholder="~/projects/my-app" />
            </label>
          </div>
          <div class="form-row">
            <label>Task Description
              <textarea bind:value={runPipelineTask} rows={4} placeholder="Describe the task for this pipeline..."></textarea>
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

          <div class="form-row"><span class="form-label"><Users size={12} /> Owner Assignment</span></div>
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
                          ...teamMembers.map((m) => ({ value: m.email, label: m.displayName || m.email })),
                        ]}
                        onchange={(v) => { stageOwners[key] = v; stageOwners = { ...stageOwners }; }}
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
                        ...teamMembers.map((m) => ({ value: m.email, label: m.displayName || m.email })),
                      ]}
                      onchange={(v) => { stageOwners[key] = v; stageOwners = { ...stageOwners }; }}
                    />
                  </div>
                {/if}
              {/each}
            </div>
            {#if isCollabMode}
              <div class="collab-notice info"><Users size={12} /> This pipeline will run as a collaborative run via relay.</div>
            {:else}
              <div class="collab-notice">Assign owners to run collaboratively, or leave all unassigned for solo mode.</div>
            {/if}
          {:else}
            <div class="collab-notice muted"><Users size={12} /> Create or join a team in Settings &gt; Team for collaborative pipelines.</div>
          {/if}

          {#if runnerMissing.length > 0}
            <div class="missing-warning"><AlertCircle size={13} /> Missing agents: {runnerMissing.join(', ')}</div>
          {/if}
        </div>
      </div>

    {:else}
      <PipelineFlowEditor
        pipeline={selectedPipeline}
        {agents}
        bind:this={pipelineEditor}
        onRunPipeline={() => startRunPipeline()}
        onDeletePipeline={deleteCurrentPipeline}
        onSaved={onPipelineSaved}
      />
    {/if}
  </div>
</div>

{#if confirmDelete}
  <Modal onclose={() => (confirmDelete = null)} position="center" label="Confirm deletion" class="confirm-dialog">
    <p class="confirm-text">Delete pipeline <strong>{confirmDelete.name}</strong>?</p>
    <p class="confirm-hint">This cannot be undone.</p>
    <div class="confirm-actions">
      <Button variant="secondary" onclick={() => (confirmDelete = null)}>Cancel</Button>
      <Button variant="danger" onclick={confirmDeleteAction}>Delete</Button>
    </div>
  </Modal>
{/if}

<style>
  .pipelines-layout {
    display: flex;
    width: 100%;
    height: 100%;
    background: var(--weplex-bg);
    overflow: hidden;
  }

  .pipelines-sidebar {
    width: 240px;
    min-width: 240px;
    display: flex;
    flex-direction: column;
    border-right: 1px solid var(--weplex-border);
    background: var(--weplex-sidebar-bg);
    position: relative;
  }
  .pipelines-sidebar::before {
    content: '';
    position: absolute;
    inset: 0;
    background-image: radial-gradient(circle, rgba(255, 255, 255, 0.06) 0.5px, transparent 0.5px);
    background-size: 12px 12px;
    pointer-events: none;
  }

  .pipelines-sidebar-header { padding: 16px 14px 12px; flex-shrink: 0; }
  .pipelines-sidebar-title { font-size: var(--weplex-text-md); font-weight: 600; margin: 0; }

  .pipelines-nav { flex: 1; overflow-y: auto; padding: 0; }
  .pipelines-loading, .pipelines-empty {
    padding: 16px;
    color: var(--weplex-text-muted);
    font-size: 12px;
    line-height: 1.5;
  }

  .pipeline-row {
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
  .pipeline-row:hover { background: var(--weplex-surface-hover); }
  .pipeline-row.selected { background: color-mix(in srgb, var(--weplex-accent) 10%, transparent); }

  .row-letter {
    display: flex; align-items: center; justify-content: center;
    width: 26px; height: 26px; border-radius: 6px; flex-shrink: 0;
    font-size: 11px; font-weight: 700; font-family: var(--weplex-font-mono);
    background: var(--weplex-surface-hover); color: var(--weplex-text-muted);
  }
  .row-letter.pipeline {
    background: color-mix(in srgb, var(--weplex-accent) 10%, transparent);
    color: var(--weplex-accent);
  }
  .row-name {
    flex: 1; font-size: 13px; font-weight: 500; color: var(--weplex-text);
    font-family: var(--weplex-font-mono); white-space: nowrap;
    overflow: hidden; text-overflow: ellipsis;
  }
  .pipeline-row.selected .row-name { color: var(--weplex-accent); }
  .row-stages {
    font-size: 10px; color: var(--weplex-text-muted);
    font-family: var(--weplex-font-mono); opacity: 0.5; flex-shrink: 0;
  }

  .pipelines-footer {
    padding: 8px;
    border-top: 1px solid var(--weplex-border);
    flex-shrink: 0;
  }
  .pipelines-action-btn {
    display: flex; align-items: center; gap: 6px; width: 100%;
    padding: 7px 10px; border: 1px dashed var(--weplex-border);
    border-radius: var(--weplex-radius-md); background: transparent;
    color: var(--weplex-text-muted); font-size: 12px; cursor: pointer;
    transition: all var(--weplex-duration-fast);
  }
  .pipelines-action-btn:hover { border-color: var(--weplex-accent); color: var(--weplex-accent); border-style: solid; }

  .pipelines-main {
    flex: 1; display: flex; flex-direction: column; min-width: 0; overflow: hidden;
  }
  .pipelines-center-msg {
    flex: 1; display: flex; align-items: center; justify-content: center;
    color: var(--weplex-text-muted); font-size: 13px;
  }

  /* Editor / Runner */
  .editor { flex: 1; overflow-y: auto; padding: 24px 32px; display: flex; flex-direction: column; }
  .editor-header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 20px; }
  .editor-header h2 {
    display: flex; align-items: center; gap: 8px;
    font-size: 15px; font-weight: 700; color: var(--weplex-text);
    font-family: var(--weplex-font-mono); margin: 0;
  }
  .editor-actions { display: flex; gap: 6px; }
  .editor-error {
    display: flex; align-items: center; gap: 6px; margin-bottom: 16px;
    padding: 8px 12px; border-radius: 6px;
    background: color-mix(in srgb, var(--weplex-error) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--weplex-error) 20%, transparent);
    color: var(--weplex-error); font-size: 12px;
  }
  .editor-form { display: flex; flex-direction: column; gap: 14px; }
  .form-row { display: flex; flex-direction: column; gap: 5px; }
  .form-row label, .form-row .form-label {
    display: flex; flex-direction: column; gap: 5px;
    font-size: 11px; font-weight: 600; color: var(--weplex-text-muted);
    text-transform: uppercase; letter-spacing: 0.04em;
  }
  .form-row input[type='text'], .form-row textarea {
    width: 100%; padding: 8px 10px; border: 1px solid var(--weplex-border);
    border-radius: 6px; background: var(--weplex-surface); color: var(--weplex-text);
    font-size: 13px; font-family: var(--weplex-font-mono); outline: none;
    transition: border-color var(--weplex-duration-fast);
  }
  .form-row input:focus, .form-row textarea:focus { border-color: var(--weplex-accent); }
  .form-row textarea { resize: vertical; line-height: 1.5; }

  .pipeline-flow { display: flex; flex-direction: column; align-items: center; gap: 0; }
  .flow-node {
    display: flex; align-items: center; gap: 10px; padding: 10px 14px;
    border: 1px solid var(--weplex-border); border-radius: 8px;
    background: var(--weplex-surface); min-width: 200px;
  }
  .flow-node.parallel { background: transparent; border-style: dashed; border-color: color-mix(in srgb, var(--weplex-accent) 20%, transparent); }
  .flow-parallel { display: flex; flex-direction: column; gap: 6px; width: 100%; }
  .flow-parallel-item {
    display: flex; align-items: center; gap: 8px; padding: 6px 10px;
    border-radius: 6px; background: var(--weplex-surface); border: 1px solid var(--weplex-border);
  }
  .flow-agent { font-size: 12px; font-weight: 600; color: var(--weplex-text); font-family: var(--weplex-font-mono); }
  .flow-opt { font-size: 9px; color: var(--weplex-text-muted); border: 1px solid var(--weplex-border); border-radius: 3px; padding: 0 4px; flex-shrink: 0; }
  .flow-arrow { color: var(--weplex-text-muted); opacity: 0.4; padding: 2px 0; }

  .missing-warning {
    display: flex; align-items: center; gap: 6px; margin-top: 16px;
    padding: 8px 12px; border-radius: 6px;
    background: color-mix(in srgb, var(--weplex-error) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--weplex-error) 20%, transparent);
    color: var(--weplex-error); font-size: 12px;
  }

  .owner-grid { display: flex; flex-direction: column; gap: 6px; margin-bottom: 12px; }
  .owner-row { display: flex; align-items: center; gap: 8px; }
  .owner-stage-name { flex: 1; font-size: 12px; font-family: var(--weplex-font-mono); color: var(--weplex-text-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  .collab-notice {
    display: flex; align-items: center; gap: 6px; padding: 6px 10px;
    border-radius: 6px; font-size: 11px; color: var(--weplex-text-muted); margin-bottom: 8px;
  }
  .collab-notice.info { background: color-mix(in srgb, var(--weplex-info) 8%, transparent); border: 1px solid color-mix(in srgb, var(--weplex-info) 20%, transparent); color: var(--weplex-info); }
  .collab-notice.muted { background: var(--weplex-surface); border: 1px solid var(--weplex-border); }

  :global(.confirm-dialog) {
    background: var(--weplex-surface); border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-xl); padding: 20px 24px;
    box-shadow: var(--weplex-shadow-overlay); max-width: 360px; width: 100%;
  }
  .confirm-text { margin: 0 0 4px; font-size: 13px; color: var(--weplex-text); }
  .confirm-hint { font-size: 11px; color: var(--weplex-text-muted); margin-bottom: 16px; }
  .confirm-actions { display: flex; gap: 8px; justify-content: flex-end; }
</style>

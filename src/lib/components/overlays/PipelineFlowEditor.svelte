<script lang="ts">
  import { onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { Select, Button } from '../ui';
  import { modelClass, initial, shortenPath, getMissingAgents } from './helpers';
  import {
    ChevronDown,
    ExternalLink,
    Plus,
    Trash2,
    Play,
    AlertCircle,
    Workflow,
  } from 'lucide-svelte';
  import type { AgentConfig, PipelineConfig, PipelineStage } from './types';

  let {
    pipeline,
    agents,
    onRunPipeline,
    onDeletePipeline,
    onSaved,
  }: {
    pipeline: PipelineConfig | null;
    agents: AgentConfig[];
    onRunPipeline: () => void;
    onDeletePipeline: () => void;
    onSaved: () => void;
  } = $props();

  // ── Internal editing state ──────────────────────────────────────────────
  let editingPipeline = $state<{
    name: string;
    description: string;
    stages: PipelineStage[];
  } | null>(null);
  let selectedNodeIndex = $state<number | null>(null);
  let undoStack = $state<string[]>([]);
  let redoStack = $state<string[]>([]);
  let saveStatus = $state<'saved' | 'saving' | 'unsaved' | 'error'>('saved');
  let saveDebounceTimer: ReturnType<typeof setTimeout> | null = null;
  let saveGeneration = 0;
  let editError = $state<string | null>(null);
  let lastLoadedPath: string | null = null;

  // Canvas pan / zoom
  let canvasEl: HTMLDivElement | undefined;
  let panX = $state(0);
  let panY = $state(0);
  let zoom = $state(1);
  let isPanning = false;
  let panAnchor = { mx: 0, my: 0, px: 0, py: 0 };

  // Node drag (pointer-based, replaces HTML5 DnD)
  let dragIdx = $state<number | null>(null);
  let dragDy = $state(0);
  let dragInternal: { index: number; startY: number; active: boolean } | null = null;

  // Derived
  let sortedAgents = $derived(
    [...agents].sort((a: AgentConfig, b: AgentConfig) => a.name.localeCompare(b.name)),
  );
  let agentNameSet = $derived(new Set(agents.map((a) => a.name)));

  // ── Sync with pipeline prop (skip if same pipeline reloaded after save) ─
  $effect(() => {
    if (pipeline) {
      if (pipeline.file_path !== lastLoadedPath) {
        lastLoadedPath = pipeline.file_path;
        beginEditingPipeline(pipeline);
      }
    } else {
      lastLoadedPath = null;
    }
  });

  onDestroy(() => {
    if (saveDebounceTimer) clearTimeout(saveDebounceTimer);
  });

  // ── Keyboard shortcuts (Cmd+Z / Cmd+Shift+Z / Escape) ──────────────────
  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape' && selectedNodeIndex !== null) {
      selectedNodeIndex = null;
      e.stopImmediatePropagation();
      return;
    }
    // Skip undo/redo when user is typing in form elements
    const tag = (e.target as HTMLElement)?.tagName;
    if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return;
    if ((e.metaKey || e.ctrlKey) && editingPipeline) {
      if (e.shiftKey && e.key === 'z') {
        e.preventDefault();
        redo();
      } else if (!e.shiftKey && e.key === 'z') {
        e.preventDefault();
        undo();
      }
    }
  }

  // ── Public methods (called by parent via bind:this) ─────────────────────
  export function startNew() {
    lastLoadedPath = null;
    editingPipeline = {
      name: 'New Pipeline',
      description: '',
      stages: [{ agent: '', role: '', optional: null, parallel: null, owner: null }],
    };
    selectedNodeIndex = 0;
    undoStack = [];
    redoStack = [];
    saveStatus = 'unsaved';
    editError = null;
  }

  // ── Pipeline editing logic ──────────────────────────────────────────────
  function beginEditingPipeline(p: PipelineConfig) {
    editingPipeline = {
      name: p.name,
      description: p.description,
      stages: JSON.parse(JSON.stringify(p.stages)),
    };
    undoStack = [];
    redoStack = [];
    saveStatus = 'saved';
    editError = null;
  }

  function selectPipelineNode(index: number) {
    selectedNodeIndex = selectedNodeIndex === index ? null : index;
  }

  function pushUndo() {
    if (!editingPipeline) return;
    undoStack = [...undoStack.slice(-19), JSON.stringify(editingPipeline)];
    redoStack = [];
  }

  function undo() {
    if (undoStack.length === 0 || !editingPipeline) return;
    redoStack = [...redoStack.slice(-19), JSON.stringify(editingPipeline)];
    const prev = undoStack[undoStack.length - 1];
    undoStack = undoStack.slice(0, -1);
    editingPipeline = JSON.parse(prev);
    selectedNodeIndex = null;
    scheduleSave();
  }

  function redo() {
    if (redoStack.length === 0 || !editingPipeline) return;
    undoStack = [...undoStack.slice(-19), JSON.stringify(editingPipeline)];
    const next = redoStack[redoStack.length - 1];
    redoStack = redoStack.slice(0, -1);
    editingPipeline = JSON.parse(next);
    selectedNodeIndex = null;
    scheduleSave();
  }

  function scheduleSave() {
    if (saveDebounceTimer) clearTimeout(saveDebounceTimer);
    saveStatus = 'unsaved';
    saveDebounceTimer = setTimeout(() => saveInline(), 800);
  }

  async function saveInline() {
    if (!editingPipeline) return;
    if (!editingPipeline.name.trim()) {
      saveStatus = 'error';
      editError = 'Name is required';
      return;
    }
    const cleanStages = editingPipeline.stages.filter((s) => {
      if (s.parallel) return s.parallel.some((ps) => ps.agent?.trim());
      return s.agent?.trim();
    });
    if (cleanStages.length === 0) {
      saveStatus = 'unsaved';
      return;
    }
    const gen = ++saveGeneration;
    saveStatus = 'saving';
    try {
      await invoke('save_pipeline', {
        name: editingPipeline.name,
        description: editingPipeline.description,
        stages: cleanStages,
        oldFilePath: pipeline?.file_path || null,
      });
      if (gen !== saveGeneration) {
        saveStatus = 'unsaved';
        return;
      }
      saveStatus = 'saved';
      editError = null;
      onSaved();
    } catch (e: unknown) {
      if (gen !== saveGeneration) {
        saveStatus = 'unsaved';
        return;
      }
      saveStatus = 'error';
      editError = e instanceof Error ? e.message : String(e);
    }
  }

  function mutateStages(fn: (stages: PipelineStage[]) => PipelineStage[]) {
    if (!editingPipeline) return;
    pushUndo();
    editingPipeline = { ...editingPipeline, stages: fn([...editingPipeline.stages]) };
    scheduleSave();
  }

  function mutatePipelineMeta(field: 'name' | 'description', value: string) {
    if (!editingPipeline) return;
    pushUndo();
    editingPipeline = { ...editingPipeline, [field]: value };
    scheduleSave();
  }

  function addStageAt(index: number) {
    mutateStages((stages) => {
      stages.splice(index, 0, { agent: '', role: '', optional: null, parallel: null, owner: null });
      return stages;
    });
    selectedNodeIndex = index;
  }

  function addParallelGroupAt(index: number) {
    mutateStages((stages) => {
      stages.splice(index, 0, {
        agent: null,
        role: null,
        optional: null,
        parallel: [
          { agent: '', role: '', optional: null, parallel: null, owner: null },
          { agent: '', role: '', optional: null, parallel: null, owner: null },
        ],
        owner: null,
      });
      return stages;
    });
    selectedNodeIndex = index;
  }

  function removeStageInline(index: number) {
    mutateStages((stages) => {
      stages.splice(index, 1);
      return stages;
    });
    if (selectedNodeIndex === index) selectedNodeIndex = null;
    else if (selectedNodeIndex !== null && selectedNodeIndex > index) selectedNodeIndex--;
    if (
      editingPipeline &&
      selectedNodeIndex !== null &&
      selectedNodeIndex >= editingPipeline.stages.length
    ) {
      selectedNodeIndex =
        editingPipeline.stages.length > 0 ? editingPipeline.stages.length - 1 : null;
    }
  }

  function updateStageField(index: number, field: 'agent' | 'role', value: string) {
    if (!editingPipeline) return;
    pushUndo();
    const stages: PipelineStage[] = JSON.parse(JSON.stringify(editingPipeline.stages));
    stages[index][field] = value;
    editingPipeline = { ...editingPipeline, stages };
    scheduleSave();
  }

  function toggleStageOptional(index: number) {
    if (!editingPipeline) return;
    pushUndo();
    const stages: PipelineStage[] = JSON.parse(JSON.stringify(editingPipeline.stages));
    stages[index].optional = stages[index].optional ? null : true;
    editingPipeline = { ...editingPipeline, stages };
    scheduleSave();
  }

  function addParallelSubStageInline(stageIndex: number) {
    mutateStages((stages) => {
      const stage = stages[stageIndex];
      if (stage.parallel) {
        stage.parallel = [
          ...stage.parallel,
          { agent: '', role: '', optional: null, parallel: null, owner: null },
        ];
      }
      return stages;
    });
  }

  function removeParallelSubStageInline(stageIndex: number, subIndex: number) {
    mutateStages((stages) => {
      const stage = stages[stageIndex];
      if (stage.parallel) {
        stage.parallel = stage.parallel.filter((_, i) => i !== subIndex);
        if (stage.parallel.length === 1) {
          stages[stageIndex] = { ...stage.parallel[0] };
        } else if (stage.parallel.length === 0) {
          stages.splice(stageIndex, 1);
        }
      }
      return stages;
    });
    if (
      editingPipeline &&
      selectedNodeIndex !== null &&
      selectedNodeIndex >= editingPipeline.stages.length
    ) {
      selectedNodeIndex =
        editingPipeline.stages.length > 0 ? editingPipeline.stages.length - 1 : null;
    }
  }

  function updateParallelSubField(
    stageIndex: number,
    subIndex: number,
    field: 'agent' | 'role',
    value: string,
  ) {
    if (!editingPipeline) return;
    pushUndo();
    const stages: PipelineStage[] = JSON.parse(JSON.stringify(editingPipeline.stages));
    if (stages[stageIndex].parallel) {
      stages[stageIndex].parallel![subIndex][field] = value;
    }
    editingPipeline = { ...editingPipeline, stages };
    scheduleSave();
  }

  function toggleParallelSubOptional(stageIndex: number, subIndex: number) {
    if (!editingPipeline) return;
    pushUndo();
    const stages: PipelineStage[] = JSON.parse(JSON.stringify(editingPipeline.stages));
    if (stages[stageIndex].parallel) {
      stages[stageIndex].parallel![subIndex].optional = stages[stageIndex].parallel![subIndex]
        .optional
        ? null
        : true;
    }
    editingPipeline = { ...editingPipeline, stages };
    scheduleSave();
  }

  // ── Canvas pan / zoom ───────────────────────────────────────────────────
  function handleWheel(e: WheelEvent) {
    if (e.ctrlKey || e.metaKey) {
      e.preventDefault();
      const prev = zoom;
      zoom = Math.max(0.25, Math.min(2.5, zoom - e.deltaY * 0.003));
      if (canvasEl) {
        const r = canvasEl.getBoundingClientRect();
        const cx = e.clientX - r.left;
        const cy = e.clientY - r.top;
        const ratio = zoom / prev;
        panX = cx - (cx - panX) * ratio;
        panY = cy - (cy - panY) * ratio;
      }
    } else {
      panX -= e.deltaX;
      panY -= e.deltaY;
    }
  }

  function handleBgDown(e: PointerEvent) {
    if (e.button !== 0) return;
    isPanning = true;
    panAnchor = { mx: e.clientX, my: e.clientY, px: panX, py: panY };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }
  function handleBgMove(e: PointerEvent) {
    if (!isPanning) return;
    panX = panAnchor.px + e.clientX - panAnchor.mx;
    panY = panAnchor.py + e.clientY - panAnchor.my;
  }
  function handleBgUp() {
    isPanning = false;
  }

  function resetView() {
    panX = 0;
    panY = 0;
    zoom = 1;
  }

  // ── Node drag (pointer-based) ─────────────────────────────────────────
  function handleNodeDown(e: PointerEvent, index: number) {
    if (e.button !== 0) return;
    const tag = (e.target as HTMLElement).tagName;
    if (
      tag === 'INPUT' ||
      tag === 'TEXTAREA' ||
      tag === 'SELECT' ||
      tag === 'BUTTON' ||
      tag === 'LABEL'
    )
      return;
    dragInternal = { index, startY: e.clientY, active: false };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function handleNodeMove(e: PointerEvent) {
    if (!dragInternal) return;
    const dy = e.clientY - dragInternal.startY;
    if (!dragInternal.active && Math.abs(dy) > 5) {
      dragInternal.active = true;
      dragIdx = dragInternal.index;
    }
    if (dragInternal.active) {
      dragDy = dy / zoom;
    }
  }

  function handleNodeUp() {
    if (!dragInternal) return;
    if (!dragInternal.active) {
      // Was a click — toggle selection
      selectPipelineNode(dragInternal.index);
    } else {
      // Commit reorder
      commitDrag();
    }
    dragInternal = null;
  }

  function commitDrag() {
    if (dragIdx === null || !editingPipeline) {
      dragIdx = null;
      dragDy = 0;
      return;
    }
    const NODE_H = 96;
    const steps = Math.round(dragDy / NODE_H);
    const from = dragIdx;
    const to = Math.max(0, Math.min(editingPipeline.stages.length - 1, from + steps));
    if (to !== from) {
      mutateStages((stages) => {
        const [moved] = stages.splice(from, 1);
        stages.splice(to, 0, moved);
        return stages;
      });
      if (selectedNodeIndex === from) selectedNodeIndex = to;
      else if (selectedNodeIndex !== null) {
        if (from < selectedNodeIndex && to >= selectedNodeIndex) selectedNodeIndex--;
        else if (from > selectedNodeIndex && to <= selectedNodeIndex) selectedNodeIndex++;
      }
    }
    dragIdx = null;
    dragDy = 0;
  }

  // ── Helpers for node color ──────────────────────────────────────────────
  function nodeColor(agentName: string | null): string {
    if (!agentName) return 'var(--weplex-text-muted)';
    const agent = agents.find((a) => a.name === agentName);
    if (!agent) return 'var(--weplex-text-muted)';
    const mc = modelClass(agent.model || '');
    if (mc === 'opus') return 'var(--weplex-model-opus)';
    if (mc === 'sonnet') return 'var(--weplex-model-sonnet)';
    if (mc === 'haiku') return 'var(--weplex-model-haiku)';
    return 'var(--weplex-accent)';
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if editingPipeline}
  {@const ep = editingPipeline}
  {@const missingAgents = getMissingAgents(ep.stages, agentNameSet)}
  <div class="detail">
    <!-- Header -->
    <div class="d-header">
      <span class="d-icon pipeline"><Workflow size={15} /></span>
      <input
        class="d-name-input"
        type="text"
        value={ep.name}
        spellcheck="false"
        placeholder="Pipeline name"
        oninput={(e) => mutatePipelineMeta('name', (e.target as HTMLInputElement).value)}
      />
      <span class="d-tag">{ep.stages.length} stages</span>
      <span class="save-status {saveStatus}">
        {#if saveStatus === 'saved'}saved
        {:else if saveStatus === 'saving'}saving...
        {:else if saveStatus === 'unsaved'}unsaved
        {:else}error{/if}
      </span>
      {#if undoStack.length > 0}
        <button class="undo-btn" onclick={undo} title="Undo (Cmd+Z)">Undo</button>
      {/if}
      {#if redoStack.length > 0}
        <button class="undo-btn" onclick={redo} title="Redo (Cmd+Shift+Z)">Redo</button>
      {/if}
    </div>

    <input
      class="d-desc-input"
      type="text"
      value={ep.description}
      spellcheck="false"
      placeholder="Click to add description..."
      oninput={(e) => mutatePipelineMeta('description', (e.target as HTMLInputElement).value)}
    />

    {#if editError}
      <div class="editor-error" style="margin-top: 12px"><AlertCircle size={13} />{editError}</div>
    {/if}

    <!-- n8n-style Canvas (pan + zoom) -->
    <div class="n8n-canvas" bind:this={canvasEl} onwheel={handleWheel}>
      <!-- Background layer for panning -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="n8n-canvas-bg"
        onpointerdown={handleBgDown}
        onpointermove={handleBgMove}
        onpointerup={handleBgUp}
      ></div>

      <div
        class="n8n-canvas-inner"
        style="transform: translate({panX}px, {panY}px) scale({zoom}); transform-origin: 0 0;"
      >
        {#each ep.stages as stage, i}
          <!-- Connection line BEFORE node (except first) -->
          {#if i > 0}
            <div class="n8n-connection">
              <svg class="n8n-connection-svg" viewBox="0 0 2 40" preserveAspectRatio="none">
                <line
                  x1="1"
                  y1="0"
                  x2="1"
                  y2="40"
                  stroke="var(--weplex-border-active)"
                  stroke-width="2"
                />
              </svg>
              <button class="n8n-connection-add" onclick={() => addStageAt(i)} title="Add stage">
                <Plus size={10} />
              </button>
            </div>
          {/if}

          <!-- Node -->
          {#if stage.parallel}
            <!-- Parallel group: horizontal branch -->
            <!-- svelte-ignore a11y_no_static_element_interactions a11y_click_events_have_key_events -->
            <div
              class="n8n-parallel-group"
              class:selected={selectedNodeIndex === i}
              class:dragging={dragIdx === i}
              style={dragIdx === i
                ? `transform: translateY(${dragDy}px); z-index: 100; position: relative;`
                : ''}
              onpointerdown={(e) => handleNodeDown(e, i)}
              onpointermove={handleNodeMove}
              onpointerup={handleNodeUp}
            >
              <!-- Fork indicator -->
              <div class="n8n-branch-lines">
                <svg
                  class="n8n-branch-svg"
                  viewBox="0 0 {Math.max(stage.parallel.length * 200, 200)} 32"
                  preserveAspectRatio="none"
                >
                  {#each stage.parallel as _, j}
                    {@const totalWidth = stage.parallel!.length * 200}
                    {@const nodeX = j * 200 + 100}
                    {@const centerX = totalWidth / 2}
                    <path
                      d="M {centerX} 0 C {centerX} 16, {nodeX} 16, {nodeX} 32"
                      fill="none"
                      stroke="var(--weplex-border-active)"
                      stroke-width="2"
                    />
                  {/each}
                </svg>
              </div>

              <!-- Parallel nodes row -->
              <div class="n8n-parallel-row">
                {#each stage.parallel as ps, j}
                  {@const pmc = modelClass(agents.find((a) => a.name === ps.agent)?.model || '')}
                  <div class="n8n-node parallel-child" style="--node-accent: {nodeColor(ps.agent)}">
                    <div class="n8n-node-icon {pmc}">
                      <span>{initial(ps.agent || '?')}</span>
                    </div>
                    <div class="n8n-node-body">
                      {#if selectedNodeIndex === i}
                        <div class="n8n-node-edit">
                          <Select
                            value={ps.agent || ''}
                            options={[
                              { value: '', label: 'Select agent...' },
                              ...sortedAgents.map((a) => ({ value: a.name, label: a.name })),
                            ]}
                            onchange={(v) => updateParallelSubField(i, j, 'agent', v)}
                          />
                          <input
                            type="text"
                            value={ps.role || ''}
                            placeholder="Role..."
                            oninput={(e) =>
                              updateParallelSubField(
                                i,
                                j,
                                'role',
                                (e.target as HTMLInputElement).value,
                              )}
                            onclick={(e) => e.stopPropagation()}
                          />
                          <div class="n8n-node-edit-actions">
                            <label class="opt-check" onclick={(e) => e.stopPropagation()}>
                              <input
                                type="checkbox"
                                checked={!!ps.optional}
                                onchange={() => toggleParallelSubOptional(i, j)}
                              />
                              opt
                            </label>
                            <button
                              class="node-del-btn"
                              onclick={(e) => {
                                e.stopPropagation();
                                removeParallelSubStageInline(i, j);
                              }}
                            >
                              <Trash2 size={11} />
                            </button>
                          </div>
                        </div>
                      {:else}
                        <span class="n8n-node-name">{ps.agent || 'Select agent...'}</span>
                        {#if ps.role}<span class="n8n-node-desc">{ps.role}</span>{/if}
                      {/if}
                    </div>
                    {#if ps.optional && selectedNodeIndex !== i}<span class="n8n-opt-badge"
                        >opt</span
                      >{/if}
                    <!-- Port dots -->
                    <div class="n8n-port top"></div>
                    <div class="n8n-port bottom"></div>
                  </div>
                {/each}
              </div>

              <!-- Merge indicator -->
              <div class="n8n-branch-lines merge">
                <svg
                  class="n8n-branch-svg"
                  viewBox="0 0 {Math.max(stage.parallel.length * 200, 200)} 32"
                  preserveAspectRatio="none"
                >
                  {#each stage.parallel as _, j}
                    {@const totalWidth = stage.parallel!.length * 200}
                    {@const nodeX = j * 200 + 100}
                    {@const centerX = totalWidth / 2}
                    <path
                      d="M {nodeX} 0 C {nodeX} 16, {centerX} 16, {centerX} 32"
                      fill="none"
                      stroke="var(--weplex-border-active)"
                      stroke-width="2"
                    />
                  {/each}
                </svg>
              </div>

              {#if selectedNodeIndex === i}
                <div class="n8n-parallel-actions">
                  <button
                    class="n8n-add-parallel-btn"
                    onclick={(e) => {
                      e.stopPropagation();
                      addParallelSubStageInline(i);
                    }}
                  >
                    <Plus size={11} /> Add branch
                  </button>
                  <button
                    class="node-del-btn"
                    onclick={(e) => {
                      e.stopPropagation();
                      removeStageInline(i);
                    }}
                  >
                    <Trash2 size={11} /> Remove group
                  </button>
                </div>
              {/if}
            </div>
          {:else}
            <!-- Sequential node -->
            {@const smc = modelClass(agents.find((a) => a.name === stage.agent)?.model || '')}
            <!-- svelte-ignore a11y_no_static_element_interactions a11y_click_events_have_key_events -->
            <div
              class="n8n-node"
              class:selected={selectedNodeIndex === i}
              class:dragging={dragIdx === i}
              style="--node-accent: {nodeColor(stage.agent)}; {dragIdx === i
                ? `transform: translateY(${dragDy}px); z-index: 100; position: relative;`
                : ''}"
              onpointerdown={(e) => handleNodeDown(e, i)}
              onpointermove={handleNodeMove}
              onpointerup={handleNodeUp}
            >
              <div class="n8n-node-icon {smc}">
                <span>{initial(stage.agent || '?')}</span>
              </div>
              <div class="n8n-node-body">
                {#if selectedNodeIndex === i}
                  <div class="n8n-node-edit">
                    <Select
                      value={stage.agent || ''}
                      options={[
                        { value: '', label: 'Select agent...' },
                        ...sortedAgents.map((a) => ({ value: a.name, label: a.name })),
                      ]}
                      onchange={(v) => updateStageField(i, 'agent', v)}
                    />
                    <input
                      type="text"
                      value={stage.role || ''}
                      placeholder="Role / instruction..."
                      oninput={(e) =>
                        updateStageField(i, 'role', (e.target as HTMLInputElement).value)}
                      onclick={(e) => e.stopPropagation()}
                    />
                    <div class="n8n-node-edit-actions">
                      <label class="opt-check" onclick={(e) => e.stopPropagation()}>
                        <input
                          type="checkbox"
                          checked={!!stage.optional}
                          onchange={() => toggleStageOptional(i)}
                        />
                        opt
                      </label>
                      <button
                        class="node-del-btn"
                        onclick={(e) => {
                          e.stopPropagation();
                          removeStageInline(i);
                        }}
                      >
                        <Trash2 size={12} />
                      </button>
                    </div>
                  </div>
                {:else}
                  <span class="n8n-node-name">{stage.agent || 'Click to configure...'}</span>
                  {#if stage.role}<span class="n8n-node-desc">{stage.role}</span>{/if}
                {/if}
              </div>
              {#if stage.optional && selectedNodeIndex !== i}<span class="n8n-opt-badge">opt</span
                >{/if}
              <!-- Port dots -->
              <div class="n8n-port top"></div>
              <div class="n8n-port bottom"></div>
            </div>
          {/if}
        {/each}

        <!-- Final connection + add buttons -->
        {#if ep.stages.length > 0}
          <div class="n8n-connection">
            <svg class="n8n-connection-svg" viewBox="0 0 2 40" preserveAspectRatio="none">
              <line
                x1="1"
                y1="0"
                x2="1"
                y2="40"
                stroke="var(--weplex-border-active)"
                stroke-width="2"
              />
            </svg>
          </div>
        {/if}

        <div class="n8n-add-buttons">
          <button class="n8n-add-btn" onclick={() => addStageAt(ep.stages.length)}>
            <Plus size={13} /> Stage
          </button>
          <button class="n8n-add-btn" onclick={() => addParallelGroupAt(ep.stages.length)}>
            <Plus size={13} /> Parallel
          </button>
        </div>
      </div>

      <!-- Zoom controls -->
      <div class="canvas-controls">
        <button
          onclick={() => {
            zoom = Math.min(2.5, zoom + 0.15);
          }}
          title="Zoom in">+</button
        >
        <button onclick={resetView} title="Reset view">{Math.round(zoom * 100)}%</button>
        <button
          onclick={() => {
            zoom = Math.max(0.25, zoom - 0.15);
          }}
          title="Zoom out">−</button
        >
      </div>
    </div>

    <!-- Missing agents warning -->
    {#if missingAgents.length > 0}
      <div class="missing-warning">
        <AlertCircle size={13} />
        Missing agents: {missingAgents.join(', ')}
      </div>
    {/if}

    <div class="d-footer-actions">
      {#if pipeline}
        <Button variant="primary" onclick={onRunPipeline}><Play size={13} /> Run Pipeline</Button>
        <Button variant="danger" onclick={onDeletePipeline}><Trash2 size={12} /> Delete</Button>
      {/if}
    </div>

    {#if pipeline}
      <span class="d-filepath">
        <ExternalLink size={11} />
        {shortenPath(pipeline.file_path)}
      </span>
    {/if}
  </div>
{:else}
  <div class="ap-center-msg">Select a pipeline or create a new one</div>
{/if}

<style>
  /* ── Detail layout ────────────────────────────────────────── */
  .detail {
    flex: 1;
    overflow-y: auto;
    padding: 28px 40px;
    display: flex;
    flex-direction: column;
    gap: 0;
  }
  .d-header {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }
  .d-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border-radius: 8px;
    flex-shrink: 0;
    font-size: 14px;
    font-weight: 700;
    font-family: var(--weplex-font-mono);
    background: var(--weplex-surface-hover);
    color: var(--weplex-text-muted);
  }
  .d-icon.pipeline {
    background: color-mix(in srgb, var(--weplex-accent) 10%, transparent);
    color: var(--weplex-accent);
  }

  .d-tag {
    font-size: 10px;
    font-weight: 500;
    padding: 2px 8px;
    border-radius: 4px;
    border: 1px solid var(--weplex-border);
    color: var(--weplex-text-muted);
    font-family: var(--weplex-font-mono);
  }
  .d-filepath {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    margin-top: 20px;
    color: var(--weplex-text-muted);
    font-size: 11px;
    font-family: var(--weplex-font-mono);
    opacity: 0.5;
  }
  .d-footer-actions {
    display: flex;
    gap: 8px;
    margin-top: 20px;
  }

  .ap-center-msg {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--weplex-text-muted);
    font-size: 13px;
  }

  /* ── Name / description inputs ─────────────────────────────── */
  .d-name-input {
    font-size: 17px;
    font-weight: 700;
    color: var(--weplex-text);
    font-family: var(--weplex-font-mono);
    letter-spacing: -0.02em;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 4px;
    padding: 2px 6px;
    margin: 0;
    outline: none;
    flex: 1;
    min-width: 120px;
    transition: all var(--weplex-duration-fast);
  }
  .d-name-input:hover {
    background: var(--weplex-surface-hover);
  }
  .d-name-input:focus {
    background: var(--weplex-surface);
    border-color: var(--weplex-accent);
  }
  .d-desc-input {
    font-size: 13px;
    color: var(--weplex-text-muted);
    line-height: 1.5;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 4px;
    padding: 4px 6px;
    margin: 8px 0 0;
    width: 100%;
    max-width: 600px;
    outline: none;
    font-family: inherit;
    transition: all var(--weplex-duration-fast);
  }
  .d-desc-input:hover {
    background: var(--weplex-surface-hover);
  }
  .d-desc-input:focus {
    background: var(--weplex-surface);
    border-color: var(--weplex-accent);
  }
  .d-desc-input::placeholder {
    color: var(--weplex-text-muted);
    opacity: 0.5;
  }

  /* ── Save status / undo ────────────────────────────────────── */
  .save-status {
    font-size: 10px;
    font-weight: 500;
    font-family: var(--weplex-font-mono);
    padding: 2px 8px;
    border-radius: 4px;
  }
  .save-status.saved {
    color: var(--weplex-success);
    opacity: 0.5;
  }
  .save-status.saving {
    color: var(--weplex-warning);
  }
  .save-status.unsaved {
    color: var(--weplex-warning);
  }
  .save-status.error {
    color: var(--weplex-error);
  }

  .undo-btn {
    border: 1px solid var(--weplex-border);
    border-radius: 4px;
    background: transparent;
    color: var(--weplex-text-muted);
    font-size: 10px;
    font-family: var(--weplex-font-mono);
    padding: 2px 8px;
    cursor: pointer;
    transition: all var(--weplex-duration-fast);
  }
  .undo-btn:hover {
    border-color: var(--weplex-accent);
    color: var(--weplex-accent);
  }

  /* ── Error ─────────────────────────────────────────────────── */
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

  /* ══════════════════════════════════════════════════════════════
     n8n-STYLE CANVAS
     ══════════════════════════════════════════════════════════════ */

  .n8n-canvas {
    margin-top: 20px;
    border-radius: 12px;
    border: 1px solid var(--weplex-border);
    background-color: var(--weplex-bg);
    min-height: 200px;
    flex: 1;
    overflow: hidden;
    position: relative;
  }

  .n8n-canvas-bg {
    position: absolute;
    inset: 0;
    cursor: grab;
    z-index: 0;
    background: radial-gradient(circle, var(--weplex-border) 1px, transparent 1px);
    background-size: 20px 20px;
  }
  .n8n-canvas-bg:active {
    cursor: grabbing;
  }

  .n8n-canvas-inner {
    position: relative;
    z-index: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 40px 32px;
    min-width: fit-content;
    pointer-events: none;
  }
  .n8n-canvas-inner > * {
    pointer-events: auto;
  }

  .canvas-controls {
    position: absolute;
    bottom: 12px;
    right: 12px;
    display: flex;
    gap: 2px;
    z-index: 10;
    background: var(--weplex-surface);
    border: 1px solid var(--weplex-border);
    border-radius: 6px;
    overflow: hidden;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.15);
  }
  .canvas-controls button {
    border: none;
    background: transparent;
    color: var(--weplex-text-muted);
    font-size: 12px;
    font-family: var(--weplex-font-mono);
    padding: 4px 10px;
    cursor: pointer;
    transition: all 120ms ease;
    min-width: 36px;
  }
  .canvas-controls button:hover {
    background: var(--weplex-surface-hover);
    color: var(--weplex-text);
  }
  .canvas-controls button:not(:last-child) {
    border-right: 1px solid var(--weplex-border);
  }

  /* ── Connection lines ──────────────────────────────────────── */
  .n8n-connection {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    height: 40px;
    width: 2px;
  }

  .n8n-connection-svg {
    width: 2px;
    height: 40px;
  }

  .n8n-connection-add {
    position: absolute;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    width: 22px;
    height: 22px;
    border-radius: 50%;
    border: 2px solid var(--weplex-border);
    background: var(--weplex-bg);
    color: var(--weplex-text-muted);
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    opacity: 0;
    transition: all 150ms ease;
    z-index: 2;
  }
  .n8n-connection:hover .n8n-connection-add {
    opacity: 1;
  }
  .n8n-connection-add:hover {
    border-color: var(--weplex-accent);
    color: var(--weplex-accent);
    background: color-mix(in srgb, var(--weplex-accent) 12%, var(--weplex-bg));
    transform: translate(-50%, -50%) scale(1.1);
  }

  /* ── Node card (n8n style) ─────────────────────────────────── */
  .n8n-node {
    position: relative;
    display: flex;
    align-items: stretch;
    min-width: 220px;
    max-width: 340px;
    border-radius: 10px;
    border: 2px solid var(--weplex-border);
    background: var(--weplex-surface);
    cursor: pointer;
    transition: all 150ms ease;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.15);
  }

  .n8n-node:hover {
    border-color: var(--node-accent, var(--weplex-accent));
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.25);
  }

  .n8n-node.selected {
    border-color: var(--node-accent, var(--weplex-accent));
    box-shadow:
      0 0 0 3px color-mix(in srgb, var(--node-accent, var(--weplex-accent)) 20%, transparent),
      0 4px 16px rgba(0, 0, 0, 0.25);
  }

  .n8n-node.dragging {
    opacity: 0.95;
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.35);
    border-color: var(--node-accent, var(--weplex-accent));
    cursor: grabbing;
  }

  /* Left icon strip */
  .n8n-node-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 44px;
    min-height: 44px;
    border-radius: 8px 0 0 8px;
    flex-shrink: 0;
    background: var(--weplex-surface-hover);
    color: var(--weplex-text-muted);
    font-size: 14px;
    font-weight: 800;
    font-family: var(--weplex-font-mono);
  }
  .n8n-node-icon.opus {
    background: color-mix(in srgb, var(--weplex-model-opus) 18%, var(--weplex-surface));
    color: var(--weplex-model-opus);
  }
  .n8n-node-icon.sonnet {
    background: color-mix(in srgb, var(--weplex-model-sonnet) 18%, var(--weplex-surface));
    color: var(--weplex-model-sonnet);
  }
  .n8n-node-icon.haiku {
    background: color-mix(in srgb, var(--weplex-model-haiku) 18%, var(--weplex-surface));
    color: var(--weplex-model-haiku);
  }

  /* Body content */
  .n8n-node-body {
    flex: 1;
    padding: 10px 14px;
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 2px;
    min-width: 0;
  }

  .n8n-node-name {
    font-size: 13px;
    font-weight: 600;
    color: var(--weplex-text);
    font-family: var(--weplex-font-mono);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .n8n-node-desc {
    font-size: 11px;
    color: var(--weplex-text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 260px;
  }

  .n8n-opt-badge {
    position: absolute;
    top: -6px;
    right: 10px;
    font-size: 9px;
    font-weight: 600;
    font-family: var(--weplex-font-mono);
    color: var(--weplex-text-muted);
    background: var(--weplex-surface);
    border: 1px solid var(--weplex-border);
    border-radius: 3px;
    padding: 0 5px;
    line-height: 14px;
  }

  /* Port dots */
  .n8n-port {
    position: absolute;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--weplex-border-active);
    border: 2px solid var(--weplex-bg);
    left: 50%;
    transform: translateX(-50%);
    z-index: 1;
    transition: background 150ms ease;
  }
  .n8n-port.top {
    top: -6px;
  }
  .n8n-port.bottom {
    bottom: -6px;
  }
  .n8n-node:hover .n8n-port,
  .n8n-node.selected .n8n-port {
    background: var(--node-accent, var(--weplex-accent));
  }

  .n8n-node {
    touch-action: none;
    user-select: none;
  }
  .n8n-parallel-group {
    touch-action: none;
    user-select: none;
  }

  /* ── Inline edit form inside node ──────────────────────────── */
  .n8n-node-edit {
    display: flex;
    flex-direction: column;
    gap: 5px;
    min-width: 0;
  }

  .n8n-node-edit select,
  .n8n-node-edit input[type='text'] {
    width: 100%;
    padding: 5px 8px;
    border: 1px solid var(--weplex-border);
    border-radius: 5px;
    background: var(--weplex-bg);
    color: var(--weplex-text);
    font-size: 12px;
    font-family: var(--weplex-font-mono);
    outline: none;
  }
  .n8n-node-edit select:focus,
  .n8n-node-edit input[type='text']:focus {
    border-color: var(--weplex-accent);
  }

  .n8n-node-edit-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .opt-check {
    display: flex;
    align-items: center;
    gap: 3px;
    font-size: 10px;
    color: var(--weplex-text-muted);
    cursor: pointer;
    font-family: var(--weplex-font-mono);
  }
  .opt-check input {
    cursor: pointer;
    accent-color: var(--weplex-accent);
  }

  .node-del-btn {
    border: none;
    background: transparent;
    color: var(--weplex-text-muted);
    cursor: pointer;
    padding: 3px;
    border-radius: 4px;
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 10px;
    font-family: var(--weplex-font-mono);
    transition: all 150ms ease;
  }
  .node-del-btn:hover {
    color: var(--weplex-error);
    background: color-mix(in srgb, var(--weplex-error) 10%, transparent);
  }

  /* ── Parallel group ────────────────────────────────────────── */
  .n8n-parallel-group {
    display: flex;
    flex-direction: column;
    align-items: center;
    cursor: pointer;
  }

  .n8n-parallel-row {
    display: flex;
    gap: 16px;
    align-items: flex-start;
  }

  .n8n-parallel-row .n8n-node {
    min-width: 180px;
  }

  .n8n-branch-lines {
    width: 100%;
    height: 32px;
    display: flex;
    justify-content: center;
  }

  .n8n-branch-svg {
    width: 100%;
    height: 32px;
    overflow: visible;
  }

  .n8n-parallel-actions {
    display: flex;
    gap: 8px;
    margin-top: 8px;
  }

  .n8n-add-parallel-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 10px;
    border: 1px dashed var(--weplex-border);
    border-radius: 6px;
    background: transparent;
    color: var(--weplex-text-muted);
    font-size: 11px;
    font-family: var(--weplex-font-mono);
    cursor: pointer;
    transition: all 150ms ease;
  }
  .n8n-add-parallel-btn:hover {
    border-color: var(--weplex-accent);
    color: var(--weplex-accent);
  }

  /* ── Add buttons at bottom ─────────────────────────────────── */
  .n8n-add-buttons {
    display: flex;
    gap: 8px;
    margin-top: 8px;
  }

  .n8n-add-btn {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 8px 16px;
    border: 2px dashed var(--weplex-border);
    border-radius: 10px;
    background: transparent;
    color: var(--weplex-text-muted);
    font-size: 12px;
    font-weight: 500;
    font-family: var(--weplex-font-mono);
    cursor: pointer;
    transition: all 150ms ease;
  }
  .n8n-add-btn:hover {
    border-color: var(--weplex-accent);
    color: var(--weplex-accent);
    background: color-mix(in srgb, var(--weplex-accent) 6%, transparent);
  }

  /* ── Missing warning ───────────────────────────────────────── */
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

</style>

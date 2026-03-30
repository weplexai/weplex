<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { ChevronRight, ExternalLink, Trash2, Save, Terminal } from 'lucide-svelte';
  import { Select, Button } from '../ui';
  import { modelClass, initial, shortenPath } from './helpers';

  interface WeplexAgentData {
    name: string;
    description: string;
    binary: string;
    model: string | null;
    prompt: string;
    one_shot: string | null;
    env: Record<string, string>;
    file_path: string;
  }

  let {
    agent,
    onSaved,
    onDeleted,
  }: {
    agent: WeplexAgentData | null;
    onSaved: () => void;
    onDeleted: () => void;
  } = $props();

  let editing = $state(false);
  let isNew = $state(false);
  let promptExpanded = $state(false);
  let saveError = $state<string | null>(null);

  // Edit form state
  let formName = $state('');
  let formDescription = $state('');
  let formBinary = $state('claude');
  let formModel = $state('');
  let formPrompt = $state('');

  $effect(() => {
    if (agent) {
      promptExpanded = false;
      editing = false;
      isNew = false;
    }
  });

  const BINARIES = ['claude', 'codex', 'aider', 'gemini'];

  export function startNew() {
    formName = '';
    formDescription = '';
    formBinary = 'claude';
    formModel = 'sonnet';
    formPrompt = '';
    editing = true;
    isNew = true;
    saveError = null;
  }

  function startEdit() {
    if (!agent) return;
    formName = agent.name;
    formDescription = agent.description;
    formBinary = agent.binary;
    formModel = agent.model || '';
    formPrompt = agent.prompt;
    editing = true;
    isNew = false;
    saveError = null;
  }

  async function save() {
    if (!formName.trim()) {
      saveError = 'Name is required';
      return;
    }
    if (!formDescription.trim()) {
      saveError = 'Description is required';
      return;
    }
    try {
      await invoke('save_weplex_agent', {
        agent: {
          name: formName.trim(),
          description: formDescription.trim(),
          binary: formBinary,
          model: formModel || null,
          prompt: formPrompt,
          one_shot: null,
          env: {},
          file_path: '',
        },
      });
      editing = false;
      saveError = null;
      onSaved();
    } catch (e: unknown) {
      saveError = e instanceof Error ? e.message : String(e);
    }
  }

  async function remove() {
    if (!agent) return;
    try {
      await invoke('delete_weplex_agent', { name: agent.name });
      onDeleted();
    } catch (e: unknown) {
      saveError = e instanceof Error ? e.message : String(e);
    }
  }

  function binaryIcon(binary: string): string {
    switch (binary) {
      case 'claude':
        return '⚡';
      case 'codex':
        return '◎';
      case 'aider':
        return '✎';
      case 'gemini':
        return '✦';
      default:
        return '>';
    }
  }
</script>

{#if editing}
  <div class="detail">
    <div class="d-header">
      <span class="d-icon weplex"><Terminal size={15} /></span>
      <h2 class="d-name">{isNew ? 'New Weplex Agent' : `Edit: ${formName}`}</h2>
    </div>

    {#if saveError}
      <div class="save-error">{saveError}</div>
    {/if}

    <div class="edit-form">
      <div class="form-row">
        <label
          >Name
          <input type="text" bind:value={formName} placeholder="my-agent" disabled={!isNew} />
        </label>
      </div>
      <div class="form-row">
        <label
          >Description
          <input type="text" bind:value={formDescription} placeholder="What this agent does" />
        </label>
      </div>
      <div class="form-row">
        <label
          >Binary
          <Select
            value={formBinary}
            options={[
              ...BINARIES.map((bin) => ({ value: bin, label: `${binaryIcon(bin)} ${bin}` })),
              { value: '', label: 'Custom path...' },
            ]}
            onchange={(v) => { formBinary = v; }}
          />
        </label>
      </div>
      <div class="form-row">
        <label
          >Model
          <input type="text" bind:value={formModel} placeholder="opus, sonnet, haiku, gpt-4..." />
        </label>
      </div>
      <div class="form-row">
        <label
          >System Prompt
          <textarea bind:value={formPrompt} rows={10} placeholder="Instructions for this agent..."
          ></textarea>
        </label>
      </div>
    </div>

    <div class="d-footer-actions">
      <Button
        variant="secondary"
        onclick={() => {
          editing = false;
          saveError = null;
        }}>Cancel</Button>
      <Button variant="primary" onclick={save}><Save size={13} /> Save</Button>
    </div>
  </div>
{:else if agent}
  <div class="detail">
    <div class="d-header">
      <span class="d-icon weplex"><Terminal size={15} /></span>
      <h2 class="d-name">{agent.name}</h2>
      <span class="d-tag binary">{binaryIcon(agent.binary)} {agent.binary}</span>
      {#if agent.model}<span class="d-tag {modelClass(agent.model)}">{agent.model}</span>{/if}
      <span class="d-tag weplex-badge">weplex</span>
    </div>

    <p class="d-desc">{agent.description}</p>

    <div class="d-divider"></div>

    {#if agent.prompt}
      <button class="prompt-toggle" onclick={() => (promptExpanded = !promptExpanded)}>
        <span class="prompt-chevron" class:expanded={promptExpanded}>
          <ChevronRight size={13} />
        </span>
        <span class="prompt-label">System prompt</span>
        <span class="prompt-lines">{agent.prompt.split('\n').length} lines</span>
      </button>
      {#if promptExpanded}
        <pre class="prompt-body">{agent.prompt}</pre>
      {/if}
    {/if}

    <div class="d-footer-actions">
      <Button variant="secondary" onclick={startEdit}>Edit</Button>
      <Button variant="danger" onclick={remove}><Trash2 size={12} /> Delete</Button>
    </div>

    <span class="d-filepath">
      <ExternalLink size={11} />
      {shortenPath(agent.file_path)}
    </span>
  </div>
{:else}
  <div class="ap-center-msg">Select an agent or create a new one</div>
{/if}

<style>
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
  .d-icon.weplex {
    background: color-mix(in srgb, var(--weplex-active) 12%, transparent);
    color: var(--weplex-active);
  }
  .d-name {
    font-size: 17px;
    font-weight: 700;
    color: var(--weplex-text);
    font-family: var(--weplex-font-mono);
    margin: 0;
    letter-spacing: -0.02em;
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
  .d-tag.binary {
    color: var(--weplex-active);
    border-color: color-mix(in srgb, var(--weplex-active) 30%, transparent);
  }
  .d-tag.weplex-badge {
    color: var(--weplex-accent);
    border-color: color-mix(in srgb, var(--weplex-accent) 30%, transparent);
    font-weight: 600;
  }
  .d-tag.opus {
    color: var(--weplex-model-opus);
    border-color: color-mix(in srgb, var(--weplex-model-opus) 30%, transparent);
  }
  .d-tag.sonnet {
    color: var(--weplex-model-sonnet);
    border-color: color-mix(in srgb, var(--weplex-model-sonnet) 30%, transparent);
  }
  .d-tag.haiku {
    color: var(--weplex-model-haiku);
    border-color: color-mix(in srgb, var(--weplex-model-haiku) 30%, transparent);
  }
  .d-desc {
    font-size: 13px;
    color: var(--weplex-text-muted);
    line-height: 1.5;
    margin: 12px 0 0;
    max-width: 600px;
  }
  .d-divider {
    height: 1px;
    background: var(--weplex-border);
    margin: 20px 0 16px;
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

  .prompt-toggle {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 4px 0;
    border: none;
    background: transparent;
    cursor: pointer;
    color: var(--weplex-text-muted);
  }
  .prompt-toggle:hover {
    color: var(--weplex-text);
  }
  .prompt-label {
    font-size: 12px;
    font-weight: 500;
  }
  .prompt-chevron {
    display: flex;
    align-items: center;
    transition: transform 0.15s ease;
  }
  .prompt-chevron.expanded {
    transform: rotate(90deg);
  }
  .prompt-lines {
    font-size: 10px;
    opacity: 0.5;
    font-family: var(--weplex-font-mono);
    margin-left: 2px;
  }
  .prompt-body {
    font-size: 11.5px;
    font-family: var(--weplex-font-mono);
    color: var(--weplex-text-secondary);
    line-height: 1.6;
    background: var(--weplex-surface);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-md);
    padding: 14px 16px;
    margin: 8px 0 0;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .edit-form {
    display: flex;
    flex-direction: column;
    gap: 14px;
    margin-top: 16px;
  }
  .form-row {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
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
  .form-row input,
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
  .form-row input:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .save-error {
    margin-top: 12px;
    padding: 8px 12px;
    border-radius: 6px;
    background: color-mix(in srgb, var(--weplex-error) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--weplex-error) 20%, transparent);
    color: var(--weplex-error);
    font-size: 12px;
  }

  .ap-center-msg {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--weplex-text-muted);
    font-size: 13px;
  }
</style>

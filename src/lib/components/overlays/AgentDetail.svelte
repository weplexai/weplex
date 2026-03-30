<script lang="ts">
  import { ChevronRight, ExternalLink, Trash2 } from 'lucide-svelte';
  import { modelShort, modelClass, initial, shortenPath } from './helpers';
  import type { AgentConfig } from './types';
  import { Button } from '../ui';

  let {
    agent,
    onEdit,
    onDelete,
  }: {
    agent: AgentConfig;
    onEdit: () => void;
    onDelete: () => void;
  } = $props();

  let promptExpanded = $state(false);

  // Reset prompt expansion when agent changes
  $effect(() => {
    agent; // track
    promptExpanded = false;
  });

  let mc = $derived(modelClass(agent.model));
</script>

<div class="detail">
  <div class="d-header">
    <span class="d-icon {mc}">{initial(agent.name)}</span>
    <h2 class="d-name">{agent.name}</h2>
    <span class="d-tag {mc}">{modelShort(agent.model)}</span>
    <span class="d-tag">{agent.permission_mode || 'default'}</span>
    {#if agent.memory}<span class="d-tag">memory: {agent.memory}</span>{/if}
    {#if agent.background}<span class="d-tag">background</span>{/if}
    {#if agent.isolation}<span class="d-tag">{agent.isolation}</span>{/if}
    {#if agent.max_turns}<span class="d-tag">max: {agent.max_turns}</span>{/if}
    {#if agent.source === 'project'}<span class="d-tag project">project</span>{/if}
  </div>

  <p class="d-desc">{agent.description}</p>

  {#if agent.tools.length > 0}
    <div class="d-tools">
      <span class="d-tools-label">Tools</span>
      {#each agent.tools as tool}
        <span class="tool-chip">{tool}</span>
      {/each}
    </div>
  {/if}

  {#if agent.disallowed_tools.length > 0}
    <div class="d-tools">
      <span class="d-tools-label denied">Denied</span>
      {#each agent.disallowed_tools as tool}
        <span class="tool-chip denied">{tool}</span>
      {/each}
    </div>
  {/if}

  {#if agent.skills.length > 0}
    <div class="d-tools">
      <span class="d-tools-label">Skills</span>
      {#each agent.skills as skill}
        <span class="tool-chip skill">{skill}</span>
      {/each}
    </div>
  {/if}

  <div class="d-divider"></div>

  {#if agent.system_prompt}
    <button class="prompt-toggle" onclick={() => (promptExpanded = !promptExpanded)}>
      <span class="prompt-chevron" class:expanded={promptExpanded}>
        <ChevronRight size={13} />
      </span>
      <span class="prompt-label">System prompt</span>
      <span class="prompt-lines">{agent.system_prompt.split('\n').length} lines</span>
    </button>
    {#if promptExpanded}
      <pre class="prompt-body">{agent.system_prompt}</pre>
    {/if}
  {/if}

  <div class="d-footer-actions">
    {#if agent.source === 'user'}
      <Button variant="secondary" onclick={onEdit}>Edit</Button>
      <Button variant="danger" onclick={onDelete}><Trash2 size={12} /> Delete</Button>
    {/if}
  </div>

  <span class="d-filepath">
    <ExternalLink size={11} />
    {shortenPath(agent.file_path)}
  </span>
</div>

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
  .d-icon.opus {
    background: color-mix(in srgb, var(--weplex-model-opus) 15%, transparent);
    color: var(--weplex-model-opus);
  }
  .d-icon.sonnet {
    background: color-mix(in srgb, var(--weplex-model-sonnet) 15%, transparent);
    color: var(--weplex-model-sonnet);
  }
  .d-icon.haiku {
    background: color-mix(in srgb, var(--weplex-model-haiku) 15%, transparent);
    color: var(--weplex-model-haiku);
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
  .d-tag.project {
    color: var(--weplex-warning);
    border-color: color-mix(in srgb, var(--weplex-warning) 30%, transparent);
  }

  .d-desc {
    font-size: 13px;
    color: var(--weplex-text-muted);
    line-height: 1.5;
    margin: 12px 0 0;
    max-width: 600px;
  }
  .d-tools {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 5px;
    margin-top: 16px;
  }
  .d-tools-label {
    font-size: 11px;
    color: var(--weplex-text-muted);
    margin-right: 4px;
  }
  .d-tools-label.denied {
    color: var(--weplex-error);
  }
  .tool-chip {
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 5px;
    background: var(--weplex-surface);
    border: 1px solid var(--weplex-border);
    color: var(--weplex-text-secondary);
    font-family: var(--weplex-font-mono);
  }
  .tool-chip.denied {
    border-color: color-mix(in srgb, var(--weplex-error) 30%, transparent);
    color: var(--weplex-error);
  }
  .tool-chip.skill {
    border-color: color-mix(in srgb, var(--weplex-warning) 30%, transparent);
    color: var(--weplex-warning);
  }
  .d-divider {
    height: 1px;
    background: var(--weplex-border);
    margin: 20px 0 16px;
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
    transition: color var(--weplex-duration-fast);
  }
  .prompt-toggle:hover {
    color: var(--weplex-text);
  }
  .prompt-label {
    font-size: 12px;
    font-weight: 500;
    color: var(--weplex-text-muted);
    transition: color var(--weplex-duration-fast);
  }
  .prompt-toggle:hover .prompt-label {
    color: var(--weplex-text);
  }
  .prompt-chevron {
    display: flex;
    align-items: center;
    transition: transform 0.15s ease;
    color: var(--weplex-text-muted);
  }
  .prompt-chevron.expanded {
    transform: rotate(90deg);
  }
  .prompt-lines {
    font-size: 10px;
    color: var(--weplex-text-muted);
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

</style>

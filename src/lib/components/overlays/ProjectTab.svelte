<script lang="ts">
  import { modelShort, modelClass, initial, shortenPath } from './helpers';
  import type { AgentConfig, ProjectConfig, SkillInfo } from './types';

  let {
    activeSession,
    projectConfig,
    projectAgents,
    skills,
    onSelectAgent,
  }: {
    activeSession: { cwd: string } | null;
    projectConfig: ProjectConfig | null;
    projectAgents: AgentConfig[];
    skills: SkillInfo[];
    onSelectAgent: (agent: AgentConfig) => void;
  } = $props();

  function parseProjectSections(content: string): { title: string; lines: string[] }[] {
    const sections: { title: string; lines: string[] }[] = [];
    let current: { title: string; lines: string[] } | null = null;
    for (const line of content.split('\n')) {
      if (/^#{1,3}\s/.test(line)) {
        if (current) sections.push(current);
        current = { title: line.replace(/^#+\s*/, ''), lines: [] };
      } else if (current && line.trim()) {
        current.lines.push(line);
      }
    }
    if (current) sections.push(current);
    return sections;
  }
</script>

<div class="project-view">
  {#if !activeSession?.cwd}
    <div class="pv-empty">
      <p>Open a Claude session to see project configuration.</p>
    </div>
  {:else}
    <div class="pv-content">
      <div class="pv-path">{shortenPath(activeSession.cwd)}</div>

      {#if projectAgents.length > 0}
        <div class="pv-section">
          <div class="pv-section-title">Project Agents</div>
          <div class="pv-section-body">
            {#each projectAgents as agent}
              {@const mc = modelClass(agent.model)}
              <button class="agent-row compact" onclick={() => onSelectAgent(agent)}>
                <span class="row-letter small {mc}">{initial(agent.name)}</span>
                <span class="row-name">{agent.name}</span>
                <span class="row-model {mc}">{modelShort(agent.model)}</span>
              </button>
            {/each}
          </div>
        </div>
      {/if}

      {#if projectConfig?.exists}
        {#each parseProjectSections(projectConfig.content) as section}
          {#if section.lines.length > 0}
            <div class="pv-section">
              <div class="pv-section-title">{section.title}</div>
              <div class="pv-section-body">
                {#each section.lines.slice(0, 16) as line}
                  <div class="pv-line">{line}</div>
                {/each}
                {#if section.lines.length > 16}
                  <div class="pv-line muted">+{section.lines.length - 16} more lines...</div>
                {/if}
              </div>
            </div>
          {/if}
        {/each}
      {:else if projectAgents.length === 0}
        <div class="pv-empty">
          <p>No project-specific configuration.</p>
          <div class="pv-hint">
            Ask Claude: <code>"set up a project pipeline for this codebase"</code>
          </div>
        </div>
      {/if}

      {#if skills.length > 0}
        <div class="pv-section">
          <div class="pv-section-title">Available Skills</div>
          <div class="pv-section-body">
            {#each skills as skill}
              <div class="skill-row">
                <span class="skill-name">{skill.name}</span>
                {#if skill.description}<span class="skill-desc">{skill.description}</span>{/if}
              </div>
            {/each}
          </div>
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  /* ── Project view ─────────────────────────────────────────── */
  .project-view {
    flex: 1;
    overflow-y: auto;
    padding: 32px 40px;
  }
  .pv-empty {
    max-width: 480px;
    padding-top: 48px;
  }
  .pv-path {
    font-size: 11px;
    font-family: var(--weplex-font-mono);
    color: var(--weplex-text-muted);
    margin-bottom: 16px;
  }
  .pv-empty p {
    font-size: 14px;
    color: var(--weplex-text);
    margin: 0 0 6px;
  }
  .pv-hint {
    margin-top: 20px;
    font-size: 12px;
    color: var(--weplex-text-muted);
  }
  .pv-hint code {
    font-family: var(--weplex-font-mono);
    background: var(--weplex-surface-hover);
    padding: 2px 6px;
    border-radius: 4px;
    color: var(--weplex-accent);
  }
  .pv-content {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .pv-section {
    background: var(--weplex-surface);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-md);
    overflow: hidden;
  }
  .pv-section-title {
    padding: 6px 14px;
    font-size: 10px;
    font-weight: 600;
    color: var(--weplex-text-muted);
    letter-spacing: 0.06em;
    text-transform: uppercase;
    background: var(--weplex-surface-hover);
    border-bottom: 1px solid var(--weplex-border);
  }
  .pv-section-body {
    padding: 10px 14px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .pv-line {
    font-size: 11.5px;
    color: var(--weplex-text-secondary);
    font-family: var(--weplex-font-mono);
    line-height: 1.5;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .pv-line.muted {
    color: var(--weplex-text-muted);
    font-style: italic;
  }
  .skill-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 2px 0;
  }
  .skill-name {
    font-size: 12px;
    font-weight: 600;
    color: var(--weplex-text);
    font-family: var(--weplex-font-mono);
  }
  .skill-desc {
    font-size: 11px;
    color: var(--weplex-text-muted);
  }

  /* ── Agent rows (reused from sidebar style) ───────────────── */
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
  .agent-row.compact {
    padding: 4px 10px;
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
  .row-letter.small {
    width: 22px;
    height: 22px;
    font-size: 10px;
    border-radius: 5px;
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
  .row-model {
    font-size: 10px;
    font-weight: 600;
    font-family: var(--weplex-font-mono);
    opacity: 0.45;
    transition: opacity var(--weplex-duration-fast);
    flex-shrink: 0;
  }
  .agent-row:hover .row-model {
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
</style>

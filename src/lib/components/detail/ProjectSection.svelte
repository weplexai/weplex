<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';

  type AgentConfig = {
    name: string;
    model: string;
    description: string;
    file_path: string;
    source: string;
  };

  type ProjectConfig = {
    exists: boolean;
    content: string;
    cwd: string;
    config_path: string;
  };

  type SkillInfo = { name: string; description: string };

  let { cwd }: { cwd: string } = $props();

  let projectConfig = $state<ProjectConfig | null>(null);
  let projectAgents = $state<AgentConfig[]>([]);
  let skills = $state<SkillInfo[]>([]);
  let loading = $state(true);

  $effect(() => {
    if (!cwd) return;
    loading = true;
    Promise.all([
      invoke<ProjectConfig>('get_project_config', { cwd }),
      invoke<AgentConfig[]>('list_agents').then((all) =>
        all.filter((a) => a.source === 'project' || a.file_path.startsWith(cwd)),
      ),
      invoke<SkillInfo[]>('list_skills'),
    ])
      .then(([config, agents, sk]) => {
        projectConfig = config;
        projectAgents = agents;
        skills = sk;
      })
      .finally(() => {
        loading = false;
      });
  });

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

  function modelShort(m: string | null | undefined): string {
    if (!m) return '\u2014';
    if (m.includes('opus')) return 'opus';
    if (m.includes('sonnet')) return 'sonnet';
    if (m.includes('haiku')) return 'haiku';
    return m || '\u2014';
  }

  function modelClass(m: string | null | undefined): string {
    if (!m) return '';
    if (m.includes('opus')) return 'opus';
    if (m.includes('sonnet')) return 'sonnet';
    if (m.includes('haiku')) return 'haiku';
    return '';
  }
</script>

{#if loading}
  <div class="ps-loading">Loading…</div>
{:else}
  {#if projectAgents.length > 0}
    <section class="section">
      <h3 class="section-title">PROJECT AGENTS</h3>
      <div class="agent-list">
        {#each projectAgents as agent}
          {@const mc = modelClass(agent.model)}
          <div class="agent-row">
            <span class="agent-letter {mc}">{agent.name.charAt(0).toUpperCase()}</span>
            <span class="agent-name">{agent.name}</span>
            <span class="agent-model {mc}">{modelShort(agent.model)}</span>
          </div>
        {/each}
      </div>
    </section>
  {/if}

  {#if projectConfig?.exists}
    {#each parseProjectSections(projectConfig.content) as section}
      {#if section.lines.length > 0}
        <section class="section">
          <h3 class="section-title">{section.title.toUpperCase()}</h3>
          <div class="config-lines">
            {#each section.lines.slice(0, 12) as line}
              <div class="config-line">{line}</div>
            {/each}
            {#if section.lines.length > 12}
              <div class="config-line muted">+{section.lines.length - 12} more…</div>
            {/if}
          </div>
        </section>
      {/if}
    {/each}
  {/if}

  {#if skills.length > 0}
    <section class="section">
      <h3 class="section-title">SKILLS</h3>
      <div class="skill-list">
        {#each skills as skill}
          <div class="skill-row">
            <span class="skill-name">{skill.name}</span>
            {#if skill.description}<span class="skill-desc">{skill.description}</span>{/if}
          </div>
        {/each}
      </div>
    </section>
  {/if}

  {#if !projectConfig?.exists && projectAgents.length === 0 && skills.length === 0}
    <div class="ps-empty">
      <p>No project configuration.</p>
      <p class="ps-hint">Ask Claude: <code>"set up a project pipeline for this codebase"</code></p>
    </div>
  {/if}
{/if}

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

  .agent-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .agent-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 0;
    font-size: var(--weplex-text-xs);
  }

  .agent-letter {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    border-radius: 5px;
    flex-shrink: 0;
    font-size: 10px;
    font-weight: 700;
    font-family: var(--weplex-font-mono);
    background: var(--weplex-surface-hover);
    color: var(--weplex-text-muted);
  }

  .agent-letter.opus {
    background: color-mix(in srgb, var(--weplex-model-opus) 15%, transparent);
    color: var(--weplex-model-opus);
  }
  .agent-letter.sonnet {
    background: color-mix(in srgb, var(--weplex-model-sonnet) 15%, transparent);
    color: var(--weplex-model-sonnet);
  }
  .agent-letter.haiku {
    background: color-mix(in srgb, var(--weplex-model-haiku) 15%, transparent);
    color: var(--weplex-model-haiku);
  }

  .agent-name {
    flex: 1;
    font-weight: 500;
    color: var(--weplex-text);
    font-family: var(--weplex-font-mono);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .agent-model {
    font-size: 10px;
    font-weight: 600;
    font-family: var(--weplex-font-mono);
    opacity: 0.45;
    flex-shrink: 0;
  }
  .agent-model.opus { color: var(--weplex-model-opus); }
  .agent-model.sonnet { color: var(--weplex-model-sonnet); }
  .agent-model.haiku { color: var(--weplex-model-haiku); }

  .config-lines {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .config-line {
    font-size: 11px;
    color: var(--weplex-text-secondary);
    font-family: var(--weplex-font-mono);
    line-height: 1.5;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .config-line.muted {
    color: var(--weplex-text-muted);
    font-style: italic;
  }

  .skill-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .skill-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 2px 0;
  }

  .skill-name {
    font-size: 11px;
    font-weight: 600;
    color: var(--weplex-text);
    font-family: var(--weplex-font-mono);
  }

  .skill-desc {
    font-size: 10px;
    color: var(--weplex-text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .ps-loading {
    padding: 16px 0;
    font-size: var(--weplex-text-xs);
    color: var(--weplex-text-muted);
  }

  .ps-empty p {
    font-size: var(--weplex-text-sm);
    color: var(--weplex-text-muted);
    margin: 0 0 6px;
  }

  .ps-hint {
    margin-top: 12px;
    font-size: 11px;
  }

  .ps-hint code {
    font-family: var(--weplex-font-mono);
    background: var(--weplex-surface-hover);
    padding: 2px 5px;
    border-radius: 4px;
    color: var(--weplex-accent);
    font-size: 10px;
  }
</style>

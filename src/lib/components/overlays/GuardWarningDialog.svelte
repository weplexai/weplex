<script lang="ts">
  import { Modal, Button } from '../ui';
  import { guardStore } from '../../stores/guardStore.svelte';
  import type { ResourceVerdict, Severity } from '../../types/guard';

  interface Props {
    profileConfigDir: string;
    resource: ResourceVerdict;
    open: boolean;
    onclose: () => void;
  }

  let { profileConfigDir, resource, open, onclose }: Props = $props();

  let working = $state(false);
  let errorMsg = $state<string | null>(null);

  function isOverridden(ruleId: string): boolean {
    return resource.overriddenFindings.includes(ruleId);
  }

  function severityLabel(s: Severity): string {
    if (s === 'block') return 'Block';
    if (s === 'warn') return 'Warning';
    return 'Info';
  }

  async function allowFinding(ruleId: string): Promise<void> {
    if (working) return;
    working = true;
    errorMsg = null;
    try {
      await guardStore.setOverride(profileConfigDir, {
        ruleId,
        resourcePath: resource.resourcePath,
        bodySha256: resource.bodySha256,
        decision: 'accept',
        decidedAt: new Date().toISOString(),
      });
      // Refresh local view: pull updated verdict from the store after
      // setOverride triggered a re-scan.
      const updated = guardStore.findingsFor(resource.resourcePath);
      if (updated) {
        resource = updated;
      }
    } catch (e) {
      errorMsg = e instanceof Error ? e.message : String(e);
    } finally {
      working = false;
    }
  }
</script>

{#if open}
  <Modal {onclose} position="center" label="Guard findings" class="guard-dialog">
    <header class="guard-header">
      <h2 class="guard-title">Guard findings</h2>
      <p class="guard-subtitle" title={resource.resourcePath}>
        {resource.resourceId}
      </p>
    </header>

    <div class="guard-body">
      {#if resource.findings.length === 0}
        <p class="guard-empty">No findings — this resource passed all checks.</p>
      {:else}
        <ul class="finding-list">
          {#each resource.findings as f (f.ruleId)}
            {@const overridden = isOverridden(f.ruleId)}
            <li class="finding finding-{f.severity}" class:overridden>
              <div class="finding-head">
                <span class="severity-chip severity-{f.severity}">
                  {severityLabel(f.severity)}
                </span>
                <span class="finding-rule">{f.ruleId}</span>
                {#if overridden}
                  <span class="overridden-tag">Overridden</span>
                {/if}
              </div>

              <p class="finding-message">{f.message}</p>

              {#if f.explanation}
                <p class="finding-explanation">{f.explanation}</p>
              {/if}

              {#if f.location}
                <p class="finding-location">{f.location}</p>
              {/if}

              {#if f.snippet}
                <pre class="finding-snippet">{f.snippet}</pre>
              {/if}

              {#if !overridden && (f.severity === 'warn' || f.severity === 'block')}
                <div class="finding-actions">
                  <Button
                    variant="secondary"
                    disabled={working}
                    onclick={() => allowFinding(f.ruleId)}
                  >
                    Allow this rule for this resource
                  </Button>
                </div>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}

      {#if errorMsg}
        <p class="guard-error">{errorMsg}</p>
      {/if}
    </div>

    <footer class="guard-footer">
      <Button variant="secondary" onclick={onclose}>Close</Button>
    </footer>
  </Modal>
{/if}

<style>
  :global(.guard-dialog) {
    width: 540px;
    max-width: 92vw;
    background: var(--weplex-surface);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-xl);
    box-shadow: var(--weplex-shadow-overlay);
    display: flex;
    flex-direction: column;
  }

  .guard-header {
    padding: 18px 20px 12px;
    border-bottom: 1px solid var(--weplex-border);
  }

  .guard-title {
    margin: 0;
    font-size: 15px;
    font-weight: 700;
    color: var(--weplex-text);
  }

  .guard-subtitle {
    margin: 4px 0 0;
    font-size: 12px;
    color: var(--weplex-text-muted);
    font-family: var(--weplex-font-mono);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .guard-body {
    padding: 12px 20px;
    overflow-y: auto;
    max-height: 60vh;
  }

  .guard-empty {
    margin: 8px 0;
    font-size: 13px;
    color: var(--weplex-text-muted);
  }

  .finding-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .finding {
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-md);
    padding: 12px;
    background: var(--weplex-bg);
  }

  .finding-warn {
    border-left: 3px solid var(--weplex-warning, #f59e0b);
  }

  .finding-block {
    border-left: 3px solid var(--weplex-error, #ef4444);
  }

  .finding-info {
    border-left: 3px solid var(--weplex-info, #3b82f6);
  }

  .finding.overridden {
    opacity: 0.6;
  }

  .finding-head {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 6px;
    flex-wrap: wrap;
  }

  .severity-chip {
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    padding: 2px 8px;
    border-radius: var(--weplex-radius-full, 999px);
  }

  .severity-warn {
    background: color-mix(in srgb, var(--weplex-warning, #f59e0b) 15%, transparent);
    color: var(--weplex-warning, #f59e0b);
  }

  .severity-block {
    background: color-mix(in srgb, var(--weplex-error, #ef4444) 15%, transparent);
    color: var(--weplex-error, #ef4444);
  }

  .severity-info {
    background: color-mix(in srgb, var(--weplex-info, #3b82f6) 15%, transparent);
    color: var(--weplex-info, #3b82f6);
  }

  .finding-rule {
    font-family: var(--weplex-font-mono);
    font-size: 11px;
    color: var(--weplex-text-muted);
  }

  .overridden-tag {
    font-size: 10px;
    text-transform: uppercase;
    font-weight: 600;
    letter-spacing: 0.05em;
    color: var(--weplex-text-muted);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-full, 999px);
    padding: 1px 8px;
    margin-left: auto;
  }

  .finding-message {
    margin: 0 0 4px;
    font-size: 13px;
    font-weight: 600;
    color: var(--weplex-text);
  }

  .finding-explanation {
    margin: 0 0 8px;
    font-size: 12px;
    color: var(--weplex-text-secondary);
    line-height: 1.5;
  }

  .finding-location {
    margin: 0 0 6px;
    font-family: var(--weplex-font-mono);
    font-size: 11px;
    color: var(--weplex-text-muted);
  }

  .finding-snippet {
    margin: 0 0 8px;
    padding: 8px 10px;
    background: var(--weplex-surface);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-sm);
    font-family: var(--weplex-font-mono);
    font-size: 11px;
    color: var(--weplex-text-secondary);
    max-height: 120px;
    overflow: auto;
    white-space: pre-wrap;
    word-break: break-all;
  }

  .finding-actions {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
    margin-top: 8px;
  }

  .guard-error {
    margin-top: 12px;
    padding: 8px 10px;
    border-radius: var(--weplex-radius-sm);
    background: color-mix(in srgb, var(--weplex-error, #ef4444) 10%, transparent);
    color: var(--weplex-error, #ef4444);
    font-size: 12px;
  }

  .guard-footer {
    display: flex;
    justify-content: flex-end;
    padding: 12px 20px 16px;
    border-top: 1px solid var(--weplex-border);
  }
</style>

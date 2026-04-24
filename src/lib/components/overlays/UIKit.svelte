<script lang="ts">
  import { uiStore } from '../../stores/uiStore.svelte';
  import {
    Button,
    Modal,
    Input,
    Textarea,
    Select,
    Tabs,
    Badge,
    Toggle,
    Checkbox,
    Tooltip,
  } from '../ui';
  import { Save, Trash2, Plus, Settings, Copy } from 'lucide-svelte';

  let selectValue = $state('dark');
  let inputValue = $state('Hello world');
  let textareaValue = $state('Multi-line\ncontent here');
  let toggleValue = $state(true);
  let checkValue = $state(true);
  let activeTab = $state('buttons');

  const tabs = [
    { id: 'buttons', label: 'Buttons' },
    { id: 'inputs', label: 'Inputs' },
    { id: 'feedback', label: 'Feedback' },
    { id: 'navigation', label: 'Navigation' },
  ];
</script>

<Modal onclose={() => uiStore.closeOverlay()} position="center" label="UI Kit" class="uikit">
  <div class="uikit-sidebar">
    <h2 class="uikit-title">UI Kit</h2>
    <Tabs
      tabs={tabs}
      active={activeTab}
      onchange={(id) => (activeTab = id)}
      orientation="vertical"
    />
  </div>

  <div class="uikit-content">
    {#if activeTab === 'buttons'}
      <section class="uikit-section">
        <h3 class="section-title">Button Variants</h3>
        <div class="uikit-row">
          <Button variant="primary">Primary</Button>
          <Button variant="secondary">Secondary</Button>
          <Button variant="danger">Danger</Button>
          <Button variant="ghost">Ghost</Button>
        </div>
      </section>

      <section class="uikit-section">
        <h3 class="section-title">Button Sizes</h3>
        <div class="uikit-row">
          <Button variant="primary" size="sm">Small</Button>
          <Button variant="primary" size="md">Medium</Button>
        </div>
      </section>

      <section class="uikit-section">
        <h3 class="section-title">With Icons</h3>
        <div class="uikit-row">
          <Button variant="primary"><Save size={13} /> Save</Button>
          <Button variant="danger"><Trash2 size={13} /> Delete</Button>
          <Button variant="secondary"><Plus size={13} /> Add</Button>
          <Button variant="ghost"><Copy size={13} /> Copy</Button>
        </div>
      </section>

      <section class="uikit-section">
        <h3 class="section-title">Icon Only</h3>
        <div class="uikit-row">
          <Button variant="primary" icon size="sm"><Plus size={14} /></Button>
          <Button variant="secondary" icon size="sm"><Settings size={14} /></Button>
          <Button variant="danger" icon size="sm"><Trash2 size={14} /></Button>
          <Button variant="primary" icon size="md"><Plus size={16} /></Button>
          <Button variant="secondary" icon size="md"><Settings size={16} /></Button>
        </div>
      </section>

      <section class="uikit-section">
        <h3 class="section-title">Disabled</h3>
        <div class="uikit-row">
          <Button variant="primary" disabled>Disabled</Button>
          <Button variant="secondary" disabled>Disabled</Button>
          <Button variant="danger" disabled>Disabled</Button>
        </div>
      </section>

    {:else if activeTab === 'inputs'}
      <section class="uikit-section">
        <h3 class="section-title">Input</h3>
        <div class="uikit-stack">
          <Input bind:value={inputValue} placeholder="Default input" />
          <Input placeholder="Monospace" mono value="/usr/local/bin" />
          <Input type="password" placeholder="Password" />
          <Input size="sm" placeholder="Small input" />
          <Input disabled placeholder="Disabled" value="Can't edit" />
        </div>
      </section>

      <section class="uikit-section">
        <h3 class="section-title">Textarea</h3>
        <div class="uikit-stack">
          <Textarea bind:value={textareaValue} placeholder="Write something..." rows={3} />
          <Textarea mono placeholder="Code block..." rows={3} />
        </div>
      </section>

      <section class="uikit-section">
        <h3 class="section-title">Select</h3>
        <div class="uikit-row">
          <Select
            value={selectValue}
            options={[
              { value: 'dark', label: 'Dark' },
              { value: 'light', label: 'Light' },
              { value: 'system', label: 'System' },
            ]}
            onchange={(v) => (selectValue = v)}
          />
        </div>
      </section>

      <section class="uikit-section">
        <h3 class="section-title">Toggle</h3>
        <div class="uikit-row">
          <Toggle bind:checked={toggleValue} />
          <span class="uikit-label">{toggleValue ? 'On' : 'Off'}</span>
          <Toggle checked={false} disabled />
          <span class="uikit-label">Disabled</span>
        </div>
      </section>

      <section class="uikit-section">
        <h3 class="section-title">Checkbox</h3>
        <div class="uikit-stack">
          <Checkbox bind:checked={checkValue}>Checked option</Checkbox>
          <Checkbox checked={false}>Unchecked option</Checkbox>
          <Checkbox checked={true} disabled>Disabled checked</Checkbox>
        </div>
      </section>

    {:else if activeTab === 'feedback'}
      <section class="uikit-section">
        <h3 class="section-title">Badge</h3>
        <div class="uikit-row">
          <Badge>Default</Badge>
          <Badge variant="success">Success</Badge>
          <Badge variant="warning">Warning</Badge>
          <Badge variant="error">Error</Badge>
          <Badge variant="info">Info</Badge>
          <Badge variant="accent">Accent</Badge>
        </div>
        <div class="uikit-row" style="margin-top: 8px;">
          <Badge size="md">Medium</Badge>
          <Badge variant="success" size="md">Running</Badge>
          <Badge variant="error" size="md">Failed</Badge>
        </div>
      </section>

      <section class="uikit-section">
        <h3 class="section-title">Tooltip</h3>
        <div class="uikit-row">
          <Tooltip text="Top tooltip" position="top">
            <Button variant="secondary">Top</Button>
          </Tooltip>
          <Tooltip text="Bottom tooltip" position="bottom">
            <Button variant="secondary">Bottom</Button>
          </Tooltip>
          <Tooltip text="Left tooltip" position="left">
            <Button variant="secondary">Left</Button>
          </Tooltip>
          <Tooltip text="Right tooltip" position="right">
            <Button variant="secondary">Right</Button>
          </Tooltip>
        </div>
      </section>

    {:else if activeTab === 'navigation'}
      <section class="uikit-section">
        <h3 class="section-title">Tabs — Horizontal</h3>
        <Tabs
          tabs={[
            { id: 'tab1', label: 'First' },
            { id: 'tab2', label: 'Second' },
            { id: 'tab3', label: 'Third' },
          ]}
          active="tab1"
          onchange={() => {}}
          orientation="horizontal"
        />
      </section>

      <section class="uikit-section">
        <h3 class="section-title">Tabs — Vertical</h3>
        <div style="width: 160px;">
          <Tabs
            tabs={[
              { id: 'v1', label: 'General' },
              { id: 'v2', label: 'Appearance' },
              { id: 'v3', label: 'Profiles' },
            ]}
            active="v1"
            onchange={() => {}}
            orientation="vertical"
          />
        </div>
      </section>

      <section class="uikit-section">
        <h3 class="section-title">Modal</h3>
        <p class="uikit-note">You're looking at it. Supports position="center" and position="top".</p>
      </section>
    {/if}
  </div>
</Modal>

<style>
  :global(.uikit) {
    width: 680px;
    height: 480px;
    background: var(--weplex-surface);
    border: 1px solid var(--weplex-border);
    border-radius: var(--weplex-radius-xl);
    box-shadow: var(--weplex-shadow-lg);
    display: flex;
    overflow: hidden;
  }

  .uikit-sidebar {
    width: 140px;
    padding: 16px 10px;
    border-right: 1px solid var(--weplex-border);
    flex-shrink: 0;
  }

  .uikit-title {
    font-size: var(--weplex-text-md);
    font-weight: 600;
    padding: 0 8px 12px;
    color: var(--weplex-text);
  }

  .uikit-content {
    flex: 1;
    padding: 20px;
    overflow-y: auto;
  }

  .uikit-section {
    margin-bottom: 24px;
  }

  .section-title {
    font-size: var(--weplex-text-sm);
    font-weight: 600;
    color: var(--weplex-text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    margin-bottom: 10px;
  }

  .uikit-row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .uikit-stack {
    display: flex;
    flex-direction: column;
    gap: 8px;
    max-width: 320px;
  }

  .uikit-label {
    font-size: var(--weplex-text-sm);
    color: var(--weplex-text-secondary);
  }

  .uikit-note {
    font-size: var(--weplex-text-sm);
    color: var(--weplex-text-muted);
  }
</style>

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import { invoke } from '@tauri-apps/api/core';
import GuardWarningDialog from './GuardWarningDialog.svelte';
import type { ResourceVerdict, ScanReport } from '../../types/guard';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

const mockedInvoke = vi.mocked(invoke);

const PROFILE = '/tmp/profile';

function buildResource(overrides: Partial<ResourceVerdict> = {}): ResourceVerdict {
  return {
    resourcePath: '/p/agents/a.md',
    manifestPath: '/p/agents/a.weplex.yaml',
    resourceId: 'agents/a',
    kind: 'agent',
    bodySha256: 'abc123',
    verdict: 'yellow',
    findings: [
      {
        ruleId: 'wildcard-tools',
        severity: 'warn',
        message: 'Wildcard tool grant',
        explanation: 'Tools include "*", which grants every tool',
        location: 'frontmatter.tools',
        snippet: 'tools: ["*"]',
      },
      {
        ruleId: 'mcp-tos-agent-cli',
        severity: 'block',
        message: 'MCP server uses agent CLI',
        explanation: 'Headless agent invocation breaks ToS',
      },
      {
        ruleId: 'permissions-broad',
        severity: 'info',
        message: 'Broad permission scope',
        explanation: 'Permissions include shell:*',
      },
    ],
    overriddenFindings: [],
    ...overrides,
  };
}

const okScan: ScanReport = {
  profileDir: PROFILE,
  resources: [],
  overall: 'green',
  deepScanRan: false,
  deepScanSkippedReason: null,
};

beforeEach(() => {
  mockedInvoke.mockReset();
});

describe('GuardWarningDialog', () => {
  it('renders one section per finding', () => {
    const resource = buildResource();
    const { container } = render(GuardWarningDialog, {
      props: {
        profileConfigDir: PROFILE,
        resource,
        open: true,
        onclose: () => {},
      },
    });
    const items = container.querySelectorAll('.finding');
    expect(items).toHaveLength(3);
  });

  it('renders nothing when open=false', () => {
    const resource = buildResource();
    const { container } = render(GuardWarningDialog, {
      props: {
        profileConfigDir: PROFILE,
        resource,
        open: false,
        onclose: () => {},
      },
    });
    expect(container.querySelectorAll('.finding')).toHaveLength(0);
  });

  it('shows Allow button only for warn/block findings (not info)', () => {
    const resource = buildResource();
    const { container } = render(GuardWarningDialog, {
      props: {
        profileConfigDir: PROFILE,
        resource,
        open: true,
        onclose: () => {},
      },
    });
    const buttons = container.querySelectorAll('.finding-actions button');
    // 2 buttons: one for the warn finding, one for the block finding.
    // The info finding does not get an Allow button.
    expect(buttons).toHaveLength(2);
  });

  it('shows Overridden tag for findings in overriddenFindings', () => {
    const resource = buildResource({
      overriddenFindings: ['wildcard-tools'],
    });
    const { container } = render(GuardWarningDialog, {
      props: {
        profileConfigDir: PROFILE,
        resource,
        open: true,
        onclose: () => {},
      },
    });
    const tags = container.querySelectorAll('.overridden-tag');
    expect(tags).toHaveLength(1);
    expect(tags[0].textContent?.trim()).toBe('Overridden');
  });

  it('does not show Allow button for already-overridden findings', () => {
    const resource = buildResource({
      overriddenFindings: ['wildcard-tools'],
    });
    const { container } = render(GuardWarningDialog, {
      props: {
        profileConfigDir: PROFILE,
        resource,
        open: true,
        onclose: () => {},
      },
    });
    // Only 1 Allow button now — wildcard-tools is overridden, mcp-tos-agent-cli is not.
    const buttons = container.querySelectorAll('.finding-actions button');
    expect(buttons).toHaveLength(1);
  });

  it('clicking Allow sends set_override_decision with correct payload', async () => {
    const resource = buildResource();
    mockedInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'set_override_decision') return undefined;
      if (cmd === 'scan_profile') return okScan;
      throw new Error(`unexpected ${cmd}`);
    });

    const { container } = render(GuardWarningDialog, {
      props: {
        profileConfigDir: PROFILE,
        resource,
        open: true,
        onclose: () => {},
      },
    });

    // First Allow button is for the wildcard-tools warn finding.
    const allowBtn = container.querySelectorAll('.finding-actions button')[0] as HTMLButtonElement;
    expect(allowBtn).toBeTruthy();
    await fireEvent.click(allowBtn);
    // Wait for any pending microtasks.
    await Promise.resolve();
    await Promise.resolve();

    const overrideCall = mockedInvoke.mock.calls.find(
      (c) => c[0] === 'set_override_decision',
    );
    expect(overrideCall).toBeDefined();
    const payload = overrideCall![1] as {
      profileConfigDir: string;
      decision: { ruleId: string; resourcePath: string; bodySha256: string; decision: string };
    };
    expect(payload.profileConfigDir).toBe(PROFILE);
    expect(payload.decision.ruleId).toBe('wildcard-tools');
    expect(payload.decision.resourcePath).toBe(resource.resourcePath);
    expect(payload.decision.bodySha256).toBe(resource.bodySha256);
    expect(payload.decision.decision).toBe('accept');
  });
});

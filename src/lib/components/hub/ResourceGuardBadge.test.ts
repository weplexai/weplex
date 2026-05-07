import { describe, it, expect } from 'vitest';
import { render } from '@testing-library/svelte';
import ResourceGuardBadge from './ResourceGuardBadge.svelte';
import type { ResourceVerdict } from '../../types/guard';

function buildVerdict(active: number, overridden = 0): ResourceVerdict {
  const findings = [];
  for (let i = 0; i < active; i++) {
    findings.push({
      ruleId: `rule-${i}`,
      severity: 'warn' as const,
      message: 'Test',
      explanation: 'Why',
      fingerprint: `fp-rule-${i}`,
    });
  }
  // overriddenFindings is keyed by per-instance fingerprint, not rule_id.
  const overriddenFingerprints: string[] = [];
  for (let i = 0; i < overridden; i++) {
    const fp = `fp-over-${i}`;
    findings.push({
      ruleId: `over-${i}`,
      severity: 'warn' as const,
      message: 'Test',
      explanation: 'Why',
      fingerprint: fp,
    });
    overriddenFingerprints.push(fp);
  }
  return {
    resourcePath: '/p/agents/a.md',
    manifestPath: '/p/agents/a.weplex.yaml',
    resourceId: 'a',
    kind: 'agent',
    bodySha256: 'abc',
    verdict: 'yellow',
    findings,
    overriddenFindings: overriddenFingerprints,
  };
}

describe('ResourceGuardBadge', () => {
  it('renders nothing for green + size=sm with no findings', () => {
    const { container } = render(ResourceGuardBadge, {
      props: { verdict: 'green', size: 'sm' },
    });
    expect(container.querySelector('.guard-badge')).toBeNull();
  });

  it('renders the green icon at size=md', () => {
    const { container } = render(ResourceGuardBadge, {
      props: { verdict: 'green', size: 'md' },
    });
    const badge = container.querySelector('.guard-badge');
    expect(badge).not.toBeNull();
    expect(badge!.classList.contains('guard-green')).toBe(true);
  });

  it('renders yellow badge with singular tooltip for one warning', () => {
    const { container } = render(ResourceGuardBadge, {
      props: { verdict: 'yellow', findings: buildVerdict(1) },
    });
    const badge = container.querySelector('.guard-badge') as HTMLElement;
    expect(badge).not.toBeNull();
    expect(badge.classList.contains('guard-yellow')).toBe(true);
    expect(badge.getAttribute('title')).toBe('1 warning — click to review');
  });

  it('renders yellow badge with plural tooltip for multiple warnings', () => {
    const { container } = render(ResourceGuardBadge, {
      props: { verdict: 'yellow', findings: buildVerdict(3) },
    });
    const badge = container.querySelector('.guard-badge') as HTMLElement;
    expect(badge.getAttribute('title')).toBe('3 warnings — click to review');
  });

  it('renders red badge with block tooltip', () => {
    const v = buildVerdict(2);
    v.verdict = 'red';
    const { container } = render(ResourceGuardBadge, {
      props: { verdict: 'red', findings: v },
    });
    const badge = container.querySelector('.guard-badge') as HTMLElement;
    expect(badge.classList.contains('guard-red')).toBe(true);
    expect(badge.getAttribute('title')).toBe('Blocked: 2 issues');
  });

  it('counts only non-overridden findings in tooltip', () => {
    // 1 active warning + 2 overridden ones → tooltip says "1 warning"
    const { container } = render(ResourceGuardBadge, {
      props: { verdict: 'yellow', findings: buildVerdict(1, 2) },
    });
    const badge = container.querySelector('.guard-badge') as HTMLElement;
    expect(badge.getAttribute('title')).toBe('1 warning — click to review');
  });
});

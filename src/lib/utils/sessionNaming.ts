import type { AgentType } from '../types';

/** Extract prompt from -p flag in agent command. */
export function extractPrompt(command: string): string | undefined {
  // Match: -p "some prompt" or -p 'some prompt' or --prompt "..."
  const match = command.match(/(?:^|\s)(?:-p|--prompt)\s+(?:"([^"]+)"|'([^']+)')/);
  if (match) {
    const prompt = (match[1] || match[2]).trim();
    // Truncate to first 40 chars, cut at word boundary
    if (prompt.length <= 40) return prompt;
    const truncated = prompt.slice(0, 40);
    const lastSpace = truncated.lastIndexOf(' ');
    return lastSpace > 20 ? truncated.slice(0, lastSpace) : truncated;
  }
  return undefined;
}

/** Generate a meaningful session name from agent type, command, and cwd. */
export function smartName(
  agentType: AgentType | undefined,
  cwd: string | undefined,
  id: number,
  command?: string,
): string {
  const prefix = agentType || 'session';

  // Priority 1: extract task from -p flag
  if (command) {
    const prompt = extractPrompt(command);
    if (prompt) return `${prefix}: ${prompt}`;
  }

  // Priority 2: use last dir component from cwd
  if (cwd && cwd !== '~') {
    const dir = cwd.replace(/\/+$/, '').split('/').pop();
    if (dir && dir !== '~') {
      return `${prefix}: ${dir}`;
    }
  }

  return agentType || `session-${id}`;
}

/** Strip ANSI escape sequences from terminal output. */
export function stripAnsi(input: string): string {
  return input
    .replace(/\x1b\[[0-9;?>=!]*[\x40-\x7e]/g, '') // CSI sequences
    .replace(/\x1b\][^\x07]*(?:\x07|\x1b\\)/g, '') // OSC sequences
    .replace(/\x1bP[^\x1b]*\x1b\\/g, '')           // DCS sequences
    .replace(/\x1b[^[\]P]/g, '')                    // other ESC sequences
    .replace(/\[\?[0-9;]*[\x40-\x7e]/g, '')         // orphaned CSI (DA responses)
    .replace(/[\x00-\x1f\x7f-\x9f]/g, '');          // control chars
}

/** Build auto-rename label from first user input. Returns null if input is unsuitable. */
export function buildAutoRenameLabel(
  agentType: string,
  userInput: string,
): string | null {
  const cleaned = userInput.trim().split('\n')[0];
  const stripped = stripAnsi(cleaned).replace(/\r/g, '').trim();
  if (stripped.length < 3 || stripped.length > 200) return null;

  const label = stripped.length <= 40
    ? stripped
    : stripped.slice(0, 40).replace(/\s+\S*$/, '');

  return `${agentType}: ${label}`;
}

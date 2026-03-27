import type { SessionType, AgentType } from '../types';

export function detectSessionType(command: string): SessionType {
  const cmd = command.toLowerCase().trim();
  if (cmd.startsWith('ssh ') || cmd === 'ssh') return 'ssh';
  if (
    cmd.includes('claude') ||
    cmd.includes('opencode') ||
    cmd.includes('aider') ||
    cmd.includes('gemini') ||
    cmd.includes('codex')
  )
    return 'agent';
  return 'terminal';
}

export function detectAgentType(command: string): AgentType | undefined {
  const cmd = command.toLowerCase().trim();
  if (cmd.includes('claude')) return 'claude';
  if (cmd.includes('opencode')) return 'opencode';
  if (cmd.includes('aider')) return 'aider';
  if (cmd.includes('gemini')) return 'gemini';
  if (cmd.includes('codex')) return 'codex';
  return undefined;
}

export function detectAgentFromOutput(output: string): AgentType | undefined {
  if (output.includes('Claude Code') || output.includes('claude-')) return 'claude';
  if (output.includes('OpenCode') || output.includes('opencode')) return 'opencode';
  if (output.includes('aider v') || output.includes('Aider')) return 'aider';
  if (output.includes('Gemini')) return 'gemini';
  if (output.includes('Codex')) return 'codex';
  return undefined;
}

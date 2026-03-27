export type AgentConfig = {
  name: string;
  description: string;
  model: string;
  tools: string[];
  disallowed_tools: string[];
  permission_mode: string | null;
  memory: string | null;
  max_turns: number | null;
  background: boolean | null;
  isolation: string | null;
  skills: string[];
  system_prompt: string;
  file_path: string;
  source: string;
};

export type PipelineStage = {
  name: string | null;
  agent: string | null;
  role: string | null;
  receives: string[];
  optional: boolean | null;
  parallel: PipelineStage[] | null;
};

export type PipelineConfig = {
  name: string;
  description: string;
  stages: PipelineStage[];
  layout: Record<string, { x: number; y: number }>;
  file_path: string;
};

export type ProjectConfig = {
  exists: boolean;
  content: string;
  cwd: string;
  config_path: string;
};

export type SkillInfo = { name: string; description: string };

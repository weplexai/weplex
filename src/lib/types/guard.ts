// Cross-agent guard types — mirror src-tauri/src/guard.rs and manifest.rs.
// All Tauri commands serialize with serde camelCase (rename_all = "camelCase").

export type Severity = 'info' | 'warn' | 'block';
export type GuardVerdict = 'green' | 'yellow' | 'red';
export type ResourceKind = 'agent' | 'rule' | 'skill' | 'command';
export type OverrideKind = 'accept' | 'reject';

export interface GuardFinding {
  ruleId: string;
  severity: Severity;
  message: string;
  explanation: string;
  snippet?: string;
  location?: string;
}

export interface ResourceVerdict {
  resourcePath: string;
  manifestPath: string;
  resourceId: string;
  kind: ResourceKind;
  bodySha256: string;
  verdict: GuardVerdict;
  findings: GuardFinding[];
  overriddenFindings: string[];
}

export interface ScanReport {
  profileDir: string;
  resources: ResourceVerdict[];
  overall: GuardVerdict;
  deepScanRan: boolean;
  deepScanSkippedReason: string | null;
}

export interface OverrideDecision {
  ruleId: string;
  resourcePath: string;
  bodySha256: string;
  decision: OverrideKind;
  decidedAt: string;
  decidedBy?: string | null;
}

// Mirror of `compiler::CompileReport` from Rust (camelCase).
export interface CompileReport {
  profileDir: string;
  manifestsSeen: number;
  targetsWritten: string[];
  targetsUnchanged: string[];
  orphansRemoved: string[];
  errors: string[];
}

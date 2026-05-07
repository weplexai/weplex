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
  /**
   * 16-hex-char per-instance identifier — distinguishes different
   * matches of the same rule in one body. Used as the override key so
   * accepting one finding does not silence its siblings.
   */
  fingerprint: string;
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
  /**
   * Deep-scan findings that don't map to a known resource — profile-wide
   * concerns such as overlapping permissions in `.claude/settings.json`.
   * Reserved for a future Hub indicator; not surfaced in the per-resource
   * UI today.
   */
  profileFindings: GuardFinding[];
}

export interface OverrideDecision {
  ruleId: string;
  resourcePath: string;
  bodySha256: string;
  /**
   * Per-instance fingerprint of the finding being overridden. `null` /
   * `undefined` is the legacy "all instances of this rule" semantics
   * (preserved for v2 store entries that pre-date per-instance
   * overrides).
   */
  fingerprint?: string | null;
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

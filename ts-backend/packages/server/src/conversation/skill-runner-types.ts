// server/conversation/skill-runner-types.ts — shared skill types.
//
// SkillResult mirrors the contract domain SkillResult (crates/.../models/skill.rs):
//   { schema_version, payload, risk_signals, analysis }.
// Kept in a standalone module so the registry and runner can share it without a
// circular import.

import type { domain } from '../contract/index.js';
import type { ColumnSummary } from '../state.js';
import type { z } from 'zod';

export type RiskSignal = z.infer<typeof domain.riskSignal>;
export type SkillResult = z.infer<typeof domain.skillResult>;

export type ResultAvailability = 'available' | 'not_computed' | 'not_applicable';
export type ConvergenceStatus = 'converged' | 'failed' | 'not_applicable' | 'unknown';
export type DiagnosticStatus = 'passed' | 'failed' | 'warning' | 'not_evaluated';

export interface ResultContractEstimate {
  term: string;
  estimate: number;
  ci_95: { lower: number; upper: number } | null;
  p_value: number | null;
  effect_unit:
    | 'Beta'
    | 'OR'
    | 'HR'
    | 'Mean difference'
    | 'Median survival'
    | 'Correlation coefficient'
    | 'Eta squared';
  adjustment: 'adjusted' | 'unadjusted' | 'descriptive';
}

export interface StandardResultContract {
  schema_version: '1.0';
  method: { algorithm_id: string; method_version: string };
  estimates: ResultContractEstimate[];
  counts: {
    input_n: number;
    complete_case_n: number;
    missing_n: number;
    event_n: number | null;
    person_time: number | null;
  };
  analysis_availability: {
    unadjusted: ResultAvailability;
    adjusted: ResultAvailability;
  };
  effect_unit: ResultContractEstimate['effect_unit'] | null;
  convergence: { status: ConvergenceStatus };
  assumption_diagnostics: Array<{
    code: string;
    status: DiagnosticStatus;
    message: string;
  }>;
  exclusions: Array<{ reason: string; n: number | null }>;
  interpretation: {
    statistical: string | null;
    practical_significance: string | null;
    unsupported_conclusions: string[];
  };
  provenance: {
    engine_name: '@stats-code/engine';
    engine_version: string;
    validation_coverage: Record<string, string>;
  };
}

/** Analysis metadata attached for output-level algorithms (Requirement 5.3). */
export interface AnalysisResultMeta {
  algorithm_id: string;
  dataset_id: string;
  dataset_sha256: string | null;
  columns: ColumnSummary[];
  params: Record<string, unknown>;
  run_id: string;
  run_status: 'completed';
  result_contract: StandardResultContract;
  [key: string]: unknown;
}

export type SkillRunError =
  | { kind: 'timeout'; wallSecs: number }
  | { kind: 'invalid_args'; missing: string[]; message: string }
  | { kind: 'execution_failed'; diagnosticExcerpt: string };

/** A thrown wrapper so callers can `catch` a structured SkillRunError. */
export class SkillRunErrorException extends Error {
  constructor(public readonly detail: SkillRunError) {
    super(detail.kind === 'invalid_args' ? detail.message : detail.kind);
    this.name = 'SkillRunErrorException';
  }
}

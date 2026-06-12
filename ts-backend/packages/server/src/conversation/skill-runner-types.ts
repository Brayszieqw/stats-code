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

/** Analysis metadata attached for output-level algorithms (Requirement 5.3). */
export interface AnalysisResultMeta {
  algorithm_id: string;
  dataset_id: string;
  dataset_sha256: string | null;
  columns: ColumnSummary[];
  params: Record<string, unknown>;
  run_id: string;
  run_status: 'completed';
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

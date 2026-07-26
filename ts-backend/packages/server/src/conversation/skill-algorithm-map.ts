// server/conversation/skill-algorithm-map.ts — pure, total skill→algorithm map.
//
// Mirrors crates/agent-core/src/skill/algorithm_map.rs verbatim. Resolves a
// skill id to at most one Output-Level Algorithm id present in the coverage
// matrix; non-output-level skills (inspect, power) and unknown ids → null.
//
// The mapping is a compile-time constant (a switch), so it resolves the same
// skill id to the same algorithm id on every host at every time (Requirement
// 4.6).

import type { AlgorithmId } from '@stats-code/engine';

/**
 * Resolve a skill id to its Output-Level Algorithm id, or null when the skill
 * is not an output-level analysis.
 */
export function skillToAlgorithm(skillId: string): AlgorithmId | null {
  switch (skillId) {
    case 'tableone':
      return 'tableone';
    case 'ttest':
      return 'ttest';
    case 'model_linear':
      return 'linear';
    case 'model_logistic':
      return 'logistic';
    case 'model_cox':
      return 'cox';
    case 'survival_km':
      return 'kaplan_meier';
    case 'correlation':
      return 'correlation';
    case 'anova':
      return 'anova';
    default:
      return null;
  }
}

// stats/ — the 17 Output_Level_Algorithm implementations (Phases 2-5).

export type AlgorithmId =
  | 'tableone'
  | 'ttest'
  | 'anova'
  | 'nonparametric'
  | 'correlation'
  | 'standardization'
  | 'or_rr'
  | 'attributable_risk'
  | 'kaplan_meier'
  | 'cox'
  | 'life_table'
  | 'linear'
  | 'logistic'
  | 'power_single_arm'
  | 'power_phase2'
  | 'power_phase3'
  | 'diagnostic_roc';

export const ALGORITHM_IDS: readonly AlgorithmId[] = [
  'tableone',
  'ttest',
  'anova',
  'nonparametric',
  'correlation',
  'standardization',
  'or_rr',
  'attributable_risk',
  'kaplan_meier',
  'cox',
  'life_table',
  'linear',
  'logistic',
  'power_single_arm',
  'power_phase2',
  'power_phase3',
  'diagnostic_roc',
] as const;

// Batch A (Phase 2, task 5.3).
export * as ttest from './ttest.js';
export * as anova from './anova.js';
export * as correlation from './correlation.js';
export * as nonparametric from './nonparametric.js';
export * as tableone from './tableone.js';
export { rankWithTies } from './rank.js';

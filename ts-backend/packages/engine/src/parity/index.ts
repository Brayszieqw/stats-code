// parity/ — internal parity runner, hidden entry point (Phases 2-8).

export const PARITY_EXIT = {
  ALL_PASS: 0,
  FAIL_ROW: 2,
  UNKNOWN_FILTER: 3,
  MISSING_TOLERANCE: 4,
  MATRIX_CONTRADICTION: 5,
} as const;

export {
  type Tolerance,
  type ComparisonStatus,
  type ComparisonResult,
  DEFAULT_NON_ITERATIVE,
  DEFAULT_ITERATIVE,
  THRESHOLDS,
  ITERATIVE_ALGORITHMS,
  isIterativeAlgorithm,
  toleranceForAlgorithm,
  differences,
  failPredicate,
  compareScalar,
} from './threshold.js';

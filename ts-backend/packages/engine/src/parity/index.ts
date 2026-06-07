// parity/ — internal parity runner, hidden entry point (Phases 2-8).

export const PARITY_EXIT = {
  ALL_PASS: 0,
  FAIL_ROW: 2,
  UNKNOWN_FILTER: 3,
  MISSING_TOLERANCE: 4,
  MATRIX_CONTRADICTION: 5,
} as const;

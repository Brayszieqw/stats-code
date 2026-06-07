// parity/threshold.ts — parity threshold comparison predicate (task 5.2).
//
// Transcribed from:
//   crates/stats-code/src/parity/tolerance.rs   (THRESHOLDS + class selection)
//   crates/stats-code/validation/parity/threshold.py  (fail_predicate)
//   crates/stats-code/validation/parity/result.py     (abs/rel difference)
//
// The predicate is pure and dependency-free: it must be safely usable by both
// the parity reporter and the snapshot --replay numeric drift gate.

/** A single Parity Threshold pair (absolute, relative). */
export interface Tolerance {
  absolute: number;
  relative: number;
}

/** Default for non-iterative algorithms (Requirement 2.2): rel 1e-6, abs 1e-9. */
export const DEFAULT_NON_ITERATIVE: Tolerance = { absolute: 1e-9, relative: 1e-6 };

/** Default for iterative algorithms (Requirement 2.3): rel 1e-4, abs 1e-7. */
export const DEFAULT_ITERATIVE: Tolerance = { absolute: 1e-7, relative: 1e-4 };

/** Algorithm-class thresholds, mirroring the design's THRESHOLDS constant. */
export const THRESHOLDS = {
  default: DEFAULT_NON_ITERATIVE,
  iterative: DEFAULT_ITERATIVE,
} as const;

/** Iterative Output-Level algorithms (Cox, logistic). */
export const ITERATIVE_ALGORITHMS: readonly string[] = ['cox', 'logistic'];

/** Whether an algorithm id is iterative (selects the relaxed tolerance class). */
export function isIterativeAlgorithm(algorithmId: string): boolean {
  return ITERATIVE_ALGORITHMS.includes(algorithmId);
}

/**
 * Select the tolerance for an algorithm by class. With no per-algorithm
 * override, iterative algorithms get the relaxed pair and everything else the
 * default pair (Requirements 2.2, 2.3).
 */
export function toleranceForAlgorithm(
  algorithmId: string,
  overrides: Readonly<Record<string, Tolerance>> = {},
): Tolerance {
  const explicit = overrides[algorithmId];
  if (explicit) {
    return explicit;
  }
  return isIterativeAlgorithm(algorithmId) ? THRESHOLDS.iterative : THRESHOLDS.default;
}

/**
 * Compute (absoluteDifference, relativeDifference) for one comparison.
 *
 * - absoluteDifference = |stats - reference|
 * - if |reference| <= absTol → relativeDifference is null (the "n/a" case);
 * - else relativeDifference = absoluteDifference / |reference|.
 */
export function differences(
  statsValue: number,
  referenceValue: number,
  absTol: number,
): { absoluteDifference: number; relativeDifference: number | null } {
  const absoluteDifference = Math.abs(statsValue - referenceValue);
  if (Math.abs(referenceValue) <= absTol) {
    return { absoluteDifference, relativeDifference: null };
  }
  return { absoluteDifference, relativeDifference: absoluteDifference / Math.abs(referenceValue) };
}

/**
 * The fail predicate (Requirements 2.2, 2.3, 2.5):
 *
 *   fail = (absDiff > absTol) AND (relDiff is defined) AND (relDiff > relTol)
 *
 * Comparisons use strict `>` ("exceeds"): a difference exactly equal to its
 * tolerance does not trip the gate. When relDiff is null (reference magnitude
 * at or below absTol) the row never fails.
 */
export function failPredicate(
  absDiff: number,
  relDiff: number | null,
  absTol: number,
  relTol: number,
): boolean {
  if (relDiff === null) {
    return false;
  }
  return absDiff > absTol && relDiff > relTol;
}

export type ComparisonStatus = 'pass' | 'fail' | 'error';

export interface ComparisonResult {
  status: ComparisonStatus;
  absoluteDifference: number | null;
  relativeDifference: number | null;
  message: string;
}

/**
 * Compare one scalar metric against a reference within `tolerance`.
 * Non-finite values produce an `error` (tolerance comparison is undefined).
 */
export function compareScalar(
  statsValue: number,
  referenceValue: number,
  tolerance: Tolerance,
): ComparisonResult {
  if (!Number.isFinite(referenceValue)) {
    return {
      status: 'error',
      absoluteDifference: null,
      relativeDifference: null,
      message: `non-finite reference value: ${referenceValue}`,
    };
  }
  if (!Number.isFinite(statsValue)) {
    return {
      status: 'error',
      absoluteDifference: null,
      relativeDifference: null,
      message: `non-finite stats value: ${statsValue}`,
    };
  }

  const { absoluteDifference, relativeDifference } = differences(
    statsValue,
    referenceValue,
    tolerance.absolute,
  );
  const failed = failPredicate(
    absoluteDifference,
    relativeDifference,
    tolerance.absolute,
    tolerance.relative,
  );

  return {
    status: failed ? 'fail' : 'pass',
    absoluteDifference,
    relativeDifference,
    message: failed
      ? `difference ${absoluteDifference.toExponential(6)} exceeds tolerance ` +
        `(abs=${tolerance.absolute.toExponential(6)}, rel=${tolerance.relative.toExponential(6)})`
      : '',
  };
}

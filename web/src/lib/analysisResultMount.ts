/**
 * Pure reducer that determines whether the AnalysisResultView component
 * should be mounted in the chat result bubble.
 *
 * The decision is based on three conditions:
 *   1. `meta` must be present (not undefined/null)
 *   2. `meta.dataset_sha256` must be non-null
 *   3. `meta.algorithm_id` must be a recognized algorithm in the coverage matrix
 *
 * Validates: Requirements 2.5, 4.4, 8.6
 */

import type { AnalysisResultMeta } from '../api/types';

/**
 * The set of Output-Level Algorithm identifiers present in the Algorithm
 * Coverage Matrix. These correspond to the Rust `skill_to_algorithm` map
 * outputs: "linear", "logistic", "cox", "kaplan_meier".
 *
 * Exported for testability.
 */
export const MATRIX_ALGORITHM_IDS: ReadonlySet<string> = new Set([
  'linear',
  'logistic',
  'cox',
  'kaplan_meier',
]);

/**
 * Returns `true` iff the AnalysisResultView should be mounted for the given
 * meta. The three conditions are:
 *   - `meta` is present (Requirement 4.4 — zero-dataset path yields no meta)
 *   - `meta.dataset_sha256` is non-null (Requirement 8.6)
 *   - `meta.algorithm_id` is in the coverage matrix (Requirement 2.5)
 */
export function shouldMountAnalysisResultView(
  meta: AnalysisResultMeta | undefined | null,
): boolean {
  if (!meta) return false;
  if (meta.dataset_sha256 == null) return false;
  return MATRIX_ALGORITHM_IDS.has(meta.algorithm_id);
}

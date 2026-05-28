/**
 * Typed client for the Algorithm Coverage Matrix.
 *
 * Mirrors the wire shape produced by the Rust DTOs in
 * `crates/api/src/sidecar.rs` (`CoverageMatrixDto`, `AlgorithmEntryDto`,
 * `ReferenceImplDto`, `CoverageValueDto`, `ReferenceSoftware`).
 *
 * The Rust side serializes its `BTreeMap<ReferenceSoftware, _>` fields as
 * JSON objects keyed by the exact strings `"R" | "SAS" | "Python" | "SPSS"`,
 * and `CoverageValueDto` as one of `"live" | "recorded" | "sidecar_only" | "none"`.
 *
 * This module provides:
 *   - Type aliases that line up byte-for-byte with the wire shape.
 *   - `fetchCoverageMatrix` — pure HTTP fetch wrapper for `GET /api/coverage-matrix`.
 *   - `lookup` / `coverageOf` — small pure helpers used by the SPA.
 *
 * Validates: Requirements 6.2
 */

// ---------------------------------------------------------------------------
// Wire types — must stay in lock-step with `crates/api/src/sidecar.rs`.
// ---------------------------------------------------------------------------

/**
 * One of the four reference software products tracked by the matrix. The
 * tokens match the Rust enum variant names verbatim (no rename).
 */
export type ReferenceSoftware = 'R' | 'SAS' | 'Python' | 'SPSS';

/**
 * Coverage state for one (algorithm, software) cell.
 *   - `live`         — Live Reference Suite asserts parity in process.
 *   - `recorded`     — Recorded Reference Suite asserts parity from a Known-Values Table.
 *   - `sidecar_only` — Sidecar Code Generator emits a snippet, no parity assertion.
 *   - `none`         — no snippet emitted, no parity assertion.
 */
export type CoverageState = 'live' | 'recorded' | 'sidecar_only' | 'none';

/**
 * Pinned reference implementation metadata for one (algorithm, software) cell.
 * Field names match `ReferenceImplDto` exactly (`callable`, optional `package`,
 * `version`).
 */
export interface ReferenceImpl {
  callable: string;
  package?: string;
  version: string;
}

/**
 * One Output-Level Algorithm row in the matrix. Field names match the Rust
 * `AlgorithmEntryDto` (snake_case `display_name`).
 */
export interface AlgorithmEntry {
  id: string;
  display_name: string;
  iterative: boolean;
  coverage: Record<ReferenceSoftware, CoverageState>;
  reference: Record<ReferenceSoftware, ReferenceImpl>;
}

/**
 * JSON shape returned by `GET /api/coverage-matrix`. Field names match the
 * Rust `CoverageMatrixDto` (snake_case `schema_version`, `release_version`).
 */
export interface CoverageMatrix {
  schema_version: number;
  release_version: string;
  algorithms: AlgorithmEntry[];
}

// ---------------------------------------------------------------------------
// Fetch
// ---------------------------------------------------------------------------

/**
 * Fetch the Algorithm Coverage Matrix from the agent-server.
 *
 * The `fetchImpl` parameter exists so unit tests can pass a stub `fetch`
 * without resorting to global stubs; in production callers pass the global
 * `fetch` (the default).
 *
 * Throws on non-2xx status so callers can branch on `error` in the context
 * layer.
 */
export async function fetchCoverageMatrix(
  fetchImpl: typeof fetch = fetch,
): Promise<CoverageMatrix> {
  const res = await fetchImpl('/api/coverage-matrix');
  if (!res.ok) {
    throw new Error(`coverage-matrix HTTP ${res.status}`);
  }
  return (await res.json()) as CoverageMatrix;
}

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

/**
 * Find an algorithm entry by exact-match id. Returns `undefined` on miss so
 * callers can decide whether a miss is an error or a render-no-op.
 */
export function lookup(
  matrix: CoverageMatrix,
  algorithmId: string,
): AlgorithmEntry | undefined {
  return matrix.algorithms.find((a) => a.id === algorithmId);
}

/**
 * Return the coverage state for one (algorithm, software) cell, or
 * `undefined` if the algorithm id isn't in the matrix.
 *
 * Note: when the algorithm id is found but the software key is missing from
 * the entry's coverage map (which shouldn't happen for matrices produced by
 * the Rust side), this returns `undefined` rather than synthesizing `none`,
 * letting callers detect the malformed-payload case.
 */
export function coverageOf(
  matrix: CoverageMatrix,
  algorithmId: string,
  software: ReferenceSoftware,
): CoverageState | undefined {
  const entry = lookup(matrix, algorithmId);
  return entry?.coverage[software];
}

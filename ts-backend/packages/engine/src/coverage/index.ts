// coverage/ — Coverage_Matrix loader, single source of truth (Phase 6, task 13.1).
// Transcribed from crates/stats-code/src/coverage_matrix/mod.rs.
//
// Parses the embedded matrix.toml (MATRIX_TOML) into an immutable CoverageMatrix,
// enforces structural invariants (Requirement 5.1/5.4/5.6): unique algorithm ids,
// and every (algorithm, software) cell present in both `coverage` and `reference`
// for all four softwares {R, SAS, Python, SPSS}. The release_version placeholder
// is replaced with the engine VERSION (mirrors the Rust build.rs injection).

import { parse as parseToml } from 'smol-toml';
import { MATRIX_TOML } from './matrix-data.js';
import { VERSION } from '../version.js';
import type { AlgorithmId } from '../stats/index.js';

export type ReferenceSoftware = 'R' | 'SAS' | 'Python' | 'SPSS';
export type CoverageState = 'live' | 'recorded' | 'sidecar_only' | 'none';

export const REQUIRED_SOFTWARE: readonly ReferenceSoftware[] = ['R', 'SAS', 'Python', 'SPSS'];
const COVERAGE_STATES: readonly CoverageState[] = ['live', 'recorded', 'sidecar_only', 'none'];

export interface ReferenceImpl {
  /** Function name (R / Python). */
  fn?: string;
  /** PROC / procedure name (SAS / SPSS). */
  proc?: string;
  /** Host package / library. */
  pkg?: string;
  /** Pinned version recorded with the entry. */
  version: string;
}

export interface AlgorithmEntry {
  id: string;
  display_name: string;
  iterative: boolean;
  /**
   * Whether the algorithm can actually be RUN from the shipped UI/HTTP
   * surface (`/api/sessions/:sid/run` dispatch + web configurator), as
   * opposed to being engine-level verified only (G2). Parity coverage above
   * speaks to numerical validation; this speaks to reachability.
   */
  ui_runnable: boolean;
  coverage: Record<ReferenceSoftware, CoverageState>;
  reference: Record<ReferenceSoftware, ReferenceImpl>;
}

export interface CoverageMatrix {
  schema_version: number;
  release_version: string;
  algorithms: AlgorithmEntry[];
}

export class CoverageParseError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'CoverageParseError';
  }
}

const PLACEHOLDER_VERSION = '0.0.0-build-injected';

function asRecord(v: unknown, ctx: string): Record<string, unknown> {
  if (typeof v !== 'object' || v === null || Array.isArray(v)) {
    throw new CoverageParseError(`${ctx} must be a table`);
  }
  return v as Record<string, unknown>;
}

function parseReferenceImpl(v: unknown, entry: string, sw: string): ReferenceImpl {
  const obj = asRecord(v, `algorithm ${entry} reference.${sw}`);
  if (typeof obj['version'] !== 'string') {
    throw new CoverageParseError(`algorithm ${entry} reference.${sw} missing string field "version"`);
  }
  const impl: ReferenceImpl = { version: obj['version'] };
  if (typeof obj['fn'] === 'string') impl.fn = obj['fn'];
  if (typeof obj['proc'] === 'string') impl.proc = obj['proc'];
  if (typeof obj['pkg'] === 'string') impl.pkg = obj['pkg'];
  return impl;
}

/**
 * Parse a coverage matrix from TOML text, enforcing the structural invariants.
 * The release_version placeholder is replaced with `releaseVersion`.
 */
export function parseCoverageMatrix(tomlText: string, releaseVersion: string): CoverageMatrix {
  let root: Record<string, unknown>;
  try {
    root = asRecord(parseToml(tomlText), 'matrix.toml root');
  } catch (err) {
    if (err instanceof CoverageParseError) throw err;
    throw new CoverageParseError(`matrix.toml is not valid TOML: ${(err as Error).message}`);
  }

  if (root['schema_version'] !== 1) {
    throw new CoverageParseError(`unsupported schema_version (expected 1)`);
  }

  const rawAlgorithms = root['algorithm'];
  if (!Array.isArray(rawAlgorithms)) {
    throw new CoverageParseError('matrix.toml must declare an [[algorithm]] array');
  }

  const seenIds = new Set<string>();
  const algorithms: AlgorithmEntry[] = [];

  for (let i = 0; i < rawAlgorithms.length; i += 1) {
    const obj = asRecord(rawAlgorithms[i], `algorithm[${i}]`);
    const id = obj['id'];
    if (typeof id !== 'string' || id.length === 0) {
      throw new CoverageParseError(`algorithm[${i}] is missing required field "id"`);
    }
    if (seenIds.has(id)) {
      throw new CoverageParseError(`duplicate algorithm id "${id}"`);
    }
    seenIds.add(id);

    if (typeof obj['display_name'] !== 'string') {
      throw new CoverageParseError(`algorithm "${id}" missing string field "display_name"`);
    }
    if (typeof obj['iterative'] !== 'boolean') {
      throw new CoverageParseError(`algorithm "${id}" missing boolean field "iterative"`);
    }
    // Optional so hand-written fixtures stay valid; absence means "not
    // reachable from the UI/HTTP surface" — the honest default (G2).
    const uiRunnable = obj['ui_runnable'];
    if (uiRunnable !== undefined && typeof uiRunnable !== 'boolean') {
      throw new CoverageParseError(`algorithm "${id}" field "ui_runnable" must be a boolean`);
    }

    const coverageRaw = asRecord(obj['coverage'], `algorithm "${id}" coverage`);
    const referenceRaw = asRecord(obj['reference'], `algorithm "${id}" reference`);

    const coverage = {} as Record<ReferenceSoftware, CoverageState>;
    const reference = {} as Record<ReferenceSoftware, ReferenceImpl>;

    for (const sw of REQUIRED_SOFTWARE) {
      const state = coverageRaw[sw];
      if (typeof state !== 'string' || !COVERAGE_STATES.includes(state as CoverageState)) {
        throw new CoverageParseError(
          `algorithm "${id}" coverage.${sw} has unknown value ${JSON.stringify(state)}`,
        );
      }
      coverage[sw] = state as CoverageState;

      if (!(sw in referenceRaw)) {
        throw new CoverageParseError(`algorithm "${id}" missing reference cell for ${sw}`);
      }
      reference[sw] = parseReferenceImpl(referenceRaw[sw], id, sw);
    }

    algorithms.push({
      id,
      display_name: obj['display_name'],
      iterative: obj['iterative'],
      ui_runnable: uiRunnable === true,
      coverage,
      reference,
    });
  }

  const declaredVersion =
    typeof root['release_version'] === 'string' ? (root['release_version'] as string) : PLACEHOLDER_VERSION;
  const release_version = declaredVersion === PLACEHOLDER_VERSION ? releaseVersion : declaredVersion;

  return { schema_version: 1, release_version, algorithms };
}

let cached: CoverageMatrix | null = null;

/** Return the process-wide immutable coverage matrix (parsed once). */
export function getLoadedMatrix(): CoverageMatrix {
  if (cached === null) {
    cached = parseCoverageMatrix(MATRIX_TOML, VERSION);
  }
  return cached;
}

/** Case-sensitive exact-match lookup by algorithm id. */
export function lookup(matrix: CoverageMatrix, id: string): AlgorithmEntry | undefined {
  return matrix.algorithms.find((e) => e.id === id);
}

/** Read the CoverageState for one (algorithm, software) cell. */
export function coverageState(
  matrix: CoverageMatrix,
  id: string,
  software: ReferenceSoftware,
): CoverageState | undefined {
  return lookup(matrix, id)?.coverage[software];
}

// ── Consistency check (Requirement 5.2, 5.5): every `live` cell must be
//    parity-validatable, i.e. backed by an entry in the test surface. ──

export interface TestSurface {
  /** "id\u0000software" keys present as live test cases. */
  liveCases: ReadonlySet<string>;
  /** "id\u0000software" keys with a recorded Known-Values Table. */
  recordedTables: ReadonlySet<string>;
  /** "id\u0000software" keys with a sidecar template. */
  templates: ReadonlySet<string>;
}

export type ConsistencyErrorKind =
  | 'missing_live_case'
  | 'missing_known_values'
  | 'missing_template'
  | 'unexpected_template'
  | 'unexpected_live_case'
  | 'unexpected_known_values';

export interface ConsistencyError {
  kind: ConsistencyErrorKind;
  algorithmId: string;
  software: ReferenceSoftware;
}

export function cellKey(id: string, software: ReferenceSoftware): string {
  return `${id}\u0000${software}`;
}

/**
 * Check the matrix against a test surface. Returns one error per offending
 * cell in declared order. An empty result means consistent (Requirement 5.2).
 */
export function checkConsistency(matrix: CoverageMatrix, surface: TestSurface): ConsistencyError[] {
  const errors: ConsistencyError[] = [];
  for (const entry of matrix.algorithms) {
    for (const software of REQUIRED_SOFTWARE) {
      const state = entry.coverage[software];
      const key = cellKey(entry.id, software);
      switch (state) {
        case 'live':
          if (!surface.liveCases.has(key)) {
            errors.push({ kind: 'missing_live_case', algorithmId: entry.id, software });
          }
          break;
        case 'recorded':
          if (!surface.recordedTables.has(key)) {
            errors.push({ kind: 'missing_known_values', algorithmId: entry.id, software });
          }
          break;
        case 'sidecar_only':
          if (!surface.templates.has(key)) {
            errors.push({ kind: 'missing_template', algorithmId: entry.id, software });
          }
          break;
        case 'none':
          if (surface.templates.has(key)) {
            errors.push({ kind: 'unexpected_template', algorithmId: entry.id, software });
          }
          if (surface.liveCases.has(key)) {
            errors.push({ kind: 'unexpected_live_case', algorithmId: entry.id, software });
          }
          if (surface.recordedTables.has(key)) {
            errors.push({ kind: 'unexpected_known_values', algorithmId: entry.id, software });
          }
          break;
      }
    }
  }
  return errors;
}

export type { AlgorithmId };

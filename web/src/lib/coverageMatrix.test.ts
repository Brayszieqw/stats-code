/**
 * Unit tests for the Algorithm Coverage Matrix client.
 *
 * Covers:
 *   - `CoverageState` union exhaustiveness (compile-time + runtime).
 *   - `lookup` hit / miss.
 *   - `coverageOf` hit / miss (unknown algorithm id).
 *   - `fetchCoverageMatrix` with a mocked `fetch` that returns a sample matrix.
 *   - `fetchCoverageMatrix` rejects on non-2xx status.
 *
 * Validates: Requirements 6.2
 */

import { describe, it, expect, vi } from 'vitest';

import {
  fetchCoverageMatrix,
  lookup,
  coverageOf,
  type CoverageMatrix,
  type CoverageState,
  type ReferenceSoftware,
} from './coverageMatrix';

// ---------------------------------------------------------------------------
// Sample matrix — mirrors the example in `crates/api/src/sidecar.rs` tests
// ---------------------------------------------------------------------------

function sampleMatrix(): CoverageMatrix {
  return {
    schema_version: 1,
    release_version: '0.5.0',
    algorithms: [
      {
        id: 'tableone',
        display_name: 'Table One',
        iterative: false,
        coverage: {
          R: 'live',
          SAS: 'recorded',
          Python: 'live',
          SPSS: 'recorded',
        },
        reference: {
          R: {
            callable: 'tableone::CreateTableOne',
            package: 'tableone',
            version: '0.13.2',
          },
          SAS: {
            callable: 'PROC FREQ;PROC MEANS',
            version: '9.4M8',
          },
          Python: {
            callable: 'scipy.stats.ttest_ind',
            package: 'scipy',
            version: '1.13.0',
          },
          SPSS: {
            callable: 'FREQUENCIES;DESCRIPTIVES',
            version: '29.0.1',
          },
        },
      },
      {
        id: 'logistic',
        display_name: 'Logistic Regression',
        iterative: true,
        coverage: {
          R: 'live',
          SAS: 'recorded',
          Python: 'live',
          SPSS: 'none',
        },
        reference: {
          R: {
            callable: 'stats::glm',
            package: 'stats',
            version: '4.4.1',
          },
          SAS: {
            callable: 'PROC LOGISTIC',
            version: '9.4M8',
          },
          Python: {
            callable: 'statsmodels.api.Logit',
            package: 'statsmodels',
            version: '0.14.2',
          },
          SPSS: {
            callable: 'LOGISTIC REGRESSION',
            version: '29.0.1',
          },
        },
      },
    ],
  };
}

// ---------------------------------------------------------------------------
// CoverageState exhaustiveness
// ---------------------------------------------------------------------------

describe('CoverageState', () => {
  it('matches all four wire tokens exactly', () => {
    // Compile-time: assigning each token to the union must type-check.
    const live: CoverageState = 'live';
    const recorded: CoverageState = 'recorded';
    const sidecarOnly: CoverageState = 'sidecar_only';
    const none: CoverageState = 'none';

    // Runtime sanity check that the array of all variants stays in lock-step.
    const all: CoverageState[] = [live, recorded, sidecarOnly, none];
    expect(all).toEqual(['live', 'recorded', 'sidecar_only', 'none']);
  });

  it('is exhaustively switched (compile-time guard)', () => {
    const stateOf = (s: CoverageState): string => {
      switch (s) {
        case 'live':
          return 'L';
        case 'recorded':
          return 'R';
        case 'sidecar_only':
          return 'S';
        case 'none':
          return 'N';
      }
    };

    expect(stateOf('live')).toBe('L');
    expect(stateOf('recorded')).toBe('R');
    expect(stateOf('sidecar_only')).toBe('S');
    expect(stateOf('none')).toBe('N');
  });
});

// ---------------------------------------------------------------------------
// lookup
// ---------------------------------------------------------------------------

describe('lookup', () => {
  it('returns the entry on exact-match hit', () => {
    const m = sampleMatrix();
    const entry = lookup(m, 'tableone');
    expect(entry).toBeDefined();
    expect(entry?.id).toBe('tableone');
    expect(entry?.display_name).toBe('Table One');
    expect(entry?.iterative).toBe(false);
  });

  it('returns undefined on miss', () => {
    const m = sampleMatrix();
    expect(lookup(m, 'does-not-exist')).toBeUndefined();
  });

  it('is case-sensitive (matches the Rust --filter contract)', () => {
    const m = sampleMatrix();
    expect(lookup(m, 'TableOne')).toBeUndefined();
    expect(lookup(m, 'TABLEONE')).toBeUndefined();
  });
});

// ---------------------------------------------------------------------------
// coverageOf
// ---------------------------------------------------------------------------

describe('coverageOf', () => {
  it('returns the recorded state for a hit', () => {
    const m = sampleMatrix();
    expect(coverageOf(m, 'tableone', 'R')).toBe('live');
    expect(coverageOf(m, 'tableone', 'SAS')).toBe('recorded');
    expect(coverageOf(m, 'tableone', 'Python')).toBe('live');
    expect(coverageOf(m, 'tableone', 'SPSS')).toBe('recorded');

    expect(coverageOf(m, 'logistic', 'SPSS')).toBe('none');
  });

  it('returns undefined for an unknown algorithm id', () => {
    const m = sampleMatrix();
    const sw: ReferenceSoftware = 'R';
    expect(coverageOf(m, 'unknown', sw)).toBeUndefined();
  });
});

// ---------------------------------------------------------------------------
// fetchCoverageMatrix
// ---------------------------------------------------------------------------

describe('fetchCoverageMatrix', () => {
  it('fetches /api/coverage-matrix and decodes the JSON body', async () => {
    const sample = sampleMatrix();
    const fetchImpl = vi.fn(async (input: RequestInfo | URL) => {
      const url = typeof input === 'string' ? input : input.toString();
      expect(url).toBe('/api/coverage-matrix');
      return new Response(JSON.stringify(sample), {
        status: 200,
        headers: { 'Content-Type': 'application/json' },
      });
    });

    const matrix = await fetchCoverageMatrix(fetchImpl as unknown as typeof fetch);

    expect(fetchImpl).toHaveBeenCalledTimes(1);
    expect(matrix).toEqual(sample);
    expect(matrix.schema_version).toBe(1);
    expect(matrix.release_version).toBe('0.5.0');
    expect(matrix.algorithms).toHaveLength(2);
  });

  it('throws on non-2xx status', async () => {
    const fetchImpl = vi.fn(
      async () =>
        new Response('boom', {
          status: 500,
          headers: { 'Content-Type': 'text/plain' },
        }),
    );

    await expect(
      fetchCoverageMatrix(fetchImpl as unknown as typeof fetch),
    ).rejects.toThrow(/coverage-matrix HTTP 500/);
  });
});

// tests/property/coverage-consistency.property.test.ts — Property 6 (task 13.10).
//
// Property 6 (Coverage consistency): the consistency checker reports an error
// for a cell IFF the test surface disagrees with the declared CoverageState —
// a `live` cell needs a live case, `recorded` needs a known-values table,
// `sidecar_only` needs a template, and `none` must have none of them. A surface
// that exactly satisfies the matrix yields zero errors.
//
// Validates: Requirements 5.2, 5.5

import { describe, it, expect } from 'vitest';
import fc from 'fast-check';
import { coverage } from '@stats-code/engine';

const { getLoadedMatrix, REQUIRED_SOFTWARE, checkConsistency, cellKey } = coverage;

const matrix = getLoadedMatrix();

/** Build the test surface that exactly satisfies the matrix. */
function satisfyingSurface() {
  const liveCases = new Set<string>();
  const recordedTables = new Set<string>();
  const templates = new Set<string>();
  for (const entry of matrix.algorithms) {
    for (const sw of REQUIRED_SOFTWARE) {
      const key = cellKey(entry.id, sw);
      switch (entry.coverage[sw]) {
        case 'live':
          liveCases.add(key);
          break;
        case 'recorded':
          recordedTables.add(key);
          break;
        case 'sidecar_only':
          templates.add(key);
          break;
        case 'none':
          break;
      }
    }
  }
  return { liveCases, recordedTables, templates };
}

describe('Property 6: coverage consistency (Requirements 5.2, 5.5)', () => {
  it('the exactly-satisfying surface yields zero errors', () => {
    expect(checkConsistency(matrix, satisfyingSurface())).toEqual([]);
  });

  it('dropping any single required backing introduces exactly one error', () => {
    const cellArb = fc
      .constantFrom(...matrix.algorithms.flatMap((a) => REQUIRED_SOFTWARE.map((sw) => ({ id: a.id, sw }))))
      .filter(({ id, sw }) => matrix.algorithms.find((a) => a.id === id)!.coverage[sw] !== 'none');

    fc.assert(
      fc.property(cellArb, ({ id, sw }) => {
        const surface = satisfyingSurface();
        const key = cellKey(id, sw);
        // Remove this cell's backing from whichever set holds it.
        surface.liveCases.delete(key);
        surface.recordedTables.delete(key);
        surface.templates.delete(key);
        const errors = checkConsistency(matrix, surface);
        // Exactly the dropped cell should now be inconsistent.
        expect(errors.length).toBe(1);
        expect(errors[0]!.algorithmId).toBe(id);
        expect(errors[0]!.software).toBe(sw);
      }),
      { numRuns: 200 },
    );
  });

  it('adding a template to a `none` cell flags an unexpected_template', () => {
    const noneCells = matrix.algorithms.flatMap((a) =>
      REQUIRED_SOFTWARE.filter((sw) => a.coverage[sw] === 'none').map((sw) => ({ id: a.id, sw })),
    );
    if (noneCells.length === 0) return;
    fc.assert(
      fc.property(fc.constantFrom(...noneCells), ({ id, sw }) => {
        const surface = satisfyingSurface();
        surface.templates.add(cellKey(id, sw));
        const errors = checkConsistency(matrix, surface);
        expect(errors.some((e) => e.kind === 'unexpected_template' && e.algorithmId === id && e.software === sw)).toBe(
          true,
        );
      }),
      { numRuns: 100 },
    );
  });
});

// tests/property/coverage-variant.property.test.ts — Property 2 (task 13.9).
//
// Property 2 (Coverage-driven variant selection): for ALL (algorithm, software)
// cells, the Sidecar_Generator's variant is fully determined by the cell's
// CoverageState — a `none` cell yields an Uncovered (copy-disabled) placeholder,
// and any other state yields a rendered snippet referencing the dataset.
//
// Validates: Requirements 3.4, 3.5

import { describe, it, expect } from 'vitest';
import fc from 'fast-check';
import { sidecar, coverage } from '@stats-code/engine';

const { generateSnippet } = sidecar;
const { getLoadedMatrix, REQUIRED_SOFTWARE } = coverage;

const matrix = getLoadedMatrix();
const ALGORITHM_IDS = matrix.algorithms.map((a) => a.id);
const SHA = '0'.repeat(64);
const cols: sidecar.Column[] = [
  { name: 'age', dtype: 'numeric' },
  { name: 'sex', dtype: 'categorical' },
];

describe('Property 2: coverage-driven variant selection (Requirements 3.4, 3.5)', () => {
  it('the variant matches the matrix coverage state for every generated cell', () => {
    fc.assert(
      fc.property(
        fc.constantFrom(...ALGORITHM_IDS),
        fc.constantFrom(...REQUIRED_SOFTWARE),
        (algo, sw) => {
          const entry = matrix.algorithms.find((a) => a.id === algo)!;
          const state = entry.coverage[sw];
          const snip = generateSnippet(algo, sw, {}, cols, SHA);
          if (state === 'none') {
            // Copy-disabled placeholder for an uncovered cell.
            expect(snip.kind).toBe('uncovered');
            if (snip.kind === 'uncovered') {
              expect(snip.coverageValue).toBe('none');
            }
          } else {
            expect(snip.kind).toBe('snippet');
            if (snip.kind === 'snippet') {
              // A covered cell (live/recorded/sidecar_only) renders a snippet
              // referencing the dataset; the kind is fully determined by the
              // non-`none` state.
              expect(snip.text).toContain('data.csv');
              expect(snip.text).toContain(SHA);
            }
          }
        },
      ),
      { numRuns: 300 },
    );
  });

  it('the chosen variant is stable: same cell → same kind on every call', () => {
    fc.assert(
      fc.property(
        fc.constantFrom(...ALGORITHM_IDS),
        fc.constantFrom(...REQUIRED_SOFTWARE),
        (algo, sw) => {
          const k1 = generateSnippet(algo, sw, {}, cols, SHA).kind;
          const k2 = generateSnippet(algo, sw, {}, cols, SHA).kind;
          expect(k1).toBe(k2);
        },
      ),
      { numRuns: 200 },
    );
  });
});

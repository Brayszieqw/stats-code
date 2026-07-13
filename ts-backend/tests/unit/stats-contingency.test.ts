import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { stats } from '@stats-code/engine';

const fisherGold = JSON.parse(readFileSync(
  new URL('../parity/known_values/fisher_exact_agresti.json', import.meta.url),
  'utf8',
)) as { expected: { fisher_exact_pvalue: number } };

describe('categorical contingency tests', () => {
  it('matches the Agresti Fisher exact two-sided gold value', () => {
    const result = stats.contingency.categoricalIndependenceTest([[3, 1], [1, 3]]);

    expect(result.status).toBe('computed');
    expect(result.method).toBe('fisher_exact');
    expect(result.pValue).toBeCloseTo(fisherGold.expected.fisher_exact_pvalue, 12);
    expect(result.minExpectedCount).toBe(2);
  });

  it('uses Pearson chi-square when every expected count is at least five', () => {
    const result = stats.contingency.categoricalIndependenceTest([[30, 20], [20, 30]]);

    expect(result.status).toBe('computed');
    expect(result.method).toBe('pearson_chi_square');
    expect(result.statistic).toBeCloseTo(4, 12);
    expect(result.degreesOfFreedom).toBe(1);
    expect(result.pValue).toBeCloseTo(0.045500263896, 9);
  });

  it('routes an observed zero cell in a 2x2 table to Fisher', () => {
    const result = stats.contingency.categoricalIndependenceTest([[0, 10], [5, 5]]);

    expect(result.status).toBe('computed');
    expect(result.method).toBe('fisher_exact');
    expect(result.observedZeroCells).toBe(1);
    expect(result.pValue).toBeCloseTo(0.032507739938080496, 12);
    expect(Number.isFinite(result.pValue)).toBe(true);
    expect(result.expectedCounts.flat().every(Number.isFinite)).toBe(true);
  });

  it('is invariant to swapping rows and columns', () => {
    const original = stats.contingency.categoricalIndependenceTest([[1, 9], [11, 3]]);
    const swappedRows = stats.contingency.categoricalIndependenceTest([[11, 3], [1, 9]]);
    const transposed = stats.contingency.categoricalIndependenceTest([[1, 11], [9, 3]]);

    expect(swappedRows.pValue).toBeCloseTo(original.pValue!, 14);
    expect(transposed.pValue).toBeCloseTo(original.pValue!, 14);
  });

  it('refuses a sparse non-2x2 table instead of reporting an asymptotic p-value', () => {
    const result = stats.contingency.categoricalIndependenceTest([[1, 0, 4], [0, 2, 3]]);

    expect(result).toMatchObject({
      status: 'not_computed',
      method: null,
      pValue: null,
      reason: 'sparse_non_2x2',
    });
  });

  it('reports an all-zero margin explicitly', () => {
    const result = stats.contingency.categoricalIndependenceTest([[0, 0], [3, 4]]);
    expect(result).toMatchObject({ status: 'not_computed', reason: 'zero_margin' });
  });

  it('rejects malformed counts', () => {
    expect(() => stats.contingency.categoricalIndependenceTest([[1, 2], [3, 0.5]]))
      .toThrow(/non-negative integers/);
  });
});

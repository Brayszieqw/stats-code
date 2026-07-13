// stats/contingency.ts — categorical independence tests with explicit sparse-table guards.

import { chiSquareP } from '../math/distributions.js';
import { clamp01, lnChoose } from '../math/special.js';

export type ContingencyMethod = 'pearson_chi_square' | 'fisher_exact';
export type ContingencyUnavailableReason =
  | 'empty_table'
  | 'insufficient_dimensions'
  | 'zero_margin'
  | 'sparse_non_2x2';

interface ContingencyDiagnostics {
  expectedCounts: number[][];
  minExpectedCount: number | null;
  expectedBelowFive: number;
  observedZeroCells: number;
}

export interface ComputedContingencyTest extends ContingencyDiagnostics {
  status: 'computed';
  method: ContingencyMethod;
  statistic: number | null;
  degreesOfFreedom: number | null;
  pValue: number;
}

export interface UnavailableContingencyTest extends ContingencyDiagnostics {
  status: 'not_computed';
  method: null;
  statistic: null;
  degreesOfFreedom: null;
  pValue: null;
  reason: ContingencyUnavailableReason;
}

export type ContingencyTestResult = ComputedContingencyTest | UnavailableContingencyTest;

function validateCounts(table: readonly (readonly number[])[]): void {
  const width = table[0]?.length ?? 0;
  if (table.some((row) => row.length !== width)) {
    throw new Error('Contingency table must be rectangular.');
  }
  for (const row of table) {
    for (const count of row) {
      if (!Number.isFinite(count) || !Number.isInteger(count) || count < 0) {
        throw new Error('Contingency table counts must be finite non-negative integers.');
      }
    }
  }
}

function unavailable(
  reason: ContingencyUnavailableReason,
  expectedCounts: number[][] = [],
  minExpectedCount: number | null = null,
  expectedBelowFive = 0,
  observedZeroCells = 0,
): UnavailableContingencyTest {
  return {
    status: 'not_computed',
    method: null,
    statistic: null,
    degreesOfFreedom: null,
    pValue: null,
    reason,
    expectedCounts,
    minExpectedCount,
    expectedBelowFive,
    observedZeroCells,
  };
}

function logHypergeometricProbability(a: number, rowOne: number, columnOne: number, total: number): number {
  return lnChoose(columnOne, a)
    + lnChoose(total - columnOne, rowOne - a)
    - lnChoose(total, rowOne);
}

/** R-compatible two-sided Fisher p-value: sum tables no more likely than observed. */
export function fisherExactTwoSided(table: readonly [readonly [number, number], readonly [number, number]]): number {
  validateCounts(table);
  const [[a, b], [c, d]] = table;
  const rowOne = a + b;
  const columnOne = a + c;
  const total = a + b + c + d;
  if (total === 0 || rowOne === 0 || rowOne === total || columnOne === 0 || columnOne === total) {
    throw new Error('Fisher exact test requires non-zero row and column margins.');
  }

  const minimumA = Math.max(0, rowOne - (total - columnOne));
  const maximumA = Math.min(rowOne, columnOne);
  const observedLogProbability = logHypergeometricProbability(a, rowOne, columnOne, total);
  const included: number[] = [];
  for (let candidate = minimumA; candidate <= maximumA; candidate += 1) {
    const logProbability = logHypergeometricProbability(candidate, rowOne, columnOne, total);
    if (logProbability <= observedLogProbability + 1e-12) included.push(logProbability);
  }
  const maximumLog = Math.max(...included);
  const scaledSum = included.reduce((sum, value) => sum + Math.exp(value - maximumLog), 0);
  return clamp01(Math.exp(maximumLog) * scaledSum);
}

/**
 * Compare an r×c count table. Pearson χ² is used only when every expected cell
 * is at least five. Sparse 2×2 tables use Fisher; larger sparse tables are
 * reported as not computed instead of returning a misleading asymptotic p-value.
 */
export function categoricalIndependenceTest(
  table: readonly (readonly number[])[],
): ContingencyTestResult {
  validateCounts(table);
  const rowCount = table.length;
  const columnCount = table[0]?.length ?? 0;
  if (rowCount === 0 || columnCount === 0) return unavailable('empty_table');
  if (rowCount < 2 || columnCount < 2) return unavailable('insufficient_dimensions');

  const rowTotals = table.map((row) => row.reduce((sum, count) => sum + count, 0));
  const columnTotals = Array.from({ length: columnCount }, (_, column) =>
    table.reduce((sum, row) => sum + row[column]!, 0));
  const total = rowTotals.reduce((sum, count) => sum + count, 0);
  const observedZeroCells = table.reduce(
    (sum, row) => sum + row.filter((count) => count === 0).length,
    0,
  );
  if (total === 0) return unavailable('empty_table', [], null, 0, observedZeroCells);
  if (rowTotals.some((count) => count === 0) || columnTotals.some((count) => count === 0)) {
    return unavailable('zero_margin', [], null, 0, observedZeroCells);
  }

  const expectedCounts = table.map((_, row) =>
    columnTotals.map((columnTotal) => (rowTotals[row]! * columnTotal) / total));
  const minExpectedCount = Math.min(...expectedCounts.flat());
  const expectedBelowFive = expectedCounts.flat().filter((count) => count < 5).length;
  const sparseTwoByTwo = expectedBelowFive > 0;
  const pearsonRuleFailed = minExpectedCount < 1
    || expectedBelowFive / (rowCount * columnCount) > 0.2;

  if (rowCount === 2 && columnCount === 2 && (sparseTwoByTwo || observedZeroCells > 0)) {
    const first = table[0]!;
    const second = table[1]!;
    const fisherTable: [[number, number], [number, number]] = [
      [first[0]!, first[1]!],
      [second[0]!, second[1]!],
    ];
    return {
      status: 'computed',
      method: 'fisher_exact',
      statistic: null,
      degreesOfFreedom: null,
      pValue: fisherExactTwoSided(fisherTable),
      expectedCounts,
      minExpectedCount,
      expectedBelowFive,
      observedZeroCells,
    };
  }

  if (pearsonRuleFailed) {
    return unavailable('sparse_non_2x2', expectedCounts, minExpectedCount, expectedBelowFive, observedZeroCells);
  }

  let statistic = 0;
  for (let row = 0; row < rowCount; row += 1) {
    for (let column = 0; column < columnCount; column += 1) {
      const expected = expectedCounts[row]![column]!;
      const residual = table[row]![column]! - expected;
      statistic += (residual * residual) / expected;
    }
  }
  const degreesOfFreedom = (rowCount - 1) * (columnCount - 1);
  return {
    status: 'computed',
    method: 'pearson_chi_square',
    statistic,
    degreesOfFreedom,
    pValue: chiSquareP(statistic, degreesOfFreedom),
    expectedCounts,
    minExpectedCount,
    expectedBelowFive,
    observedZeroCells,
  };
}

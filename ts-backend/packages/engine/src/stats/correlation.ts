// stats/correlation.ts — Pearson & Spearman correlation (Phase 2, task 5.3).
// Transcribed from crates/stats-code/src/stats/correlation.rs.

import { inverseNormal, tDistributionPValue } from '../math/distributions.js';
import { rankWithTies } from './rank.js';

export interface CorrelationResult {
  method: 'pearson' | 'spearman';
  n: number;
  r: number;
  tStatistic: number;
  df: number;
  pValue: number;
  ciLower: number;
  ciUpper: number;
  alpha: number;
}

/**
 * Pearson product-moment correlation coefficient.
 *
 * Intentionally returns 0 (rather than throwing) when either input has zero
 * variance: this is a public export consumed directly by the parity suite
 * (tests/parity/batch-a.parity.test.ts) as a raw numeric primitive, mirroring
 * a bare `cor()`-style building block rather than a hypothesis-test result.
 * The "reject a constant column" policy lives one layer up, in
 * `pearsonCorrelation` / `spearmanCorrelation` below, which gate on variance
 * via `assertNonConstant()` before ever calling this function.
 */
export function pearsonR(x: readonly number[], y: readonly number[]): number {
  const n = x.length;
  const meanX = x.reduce((s, v) => s + v, 0) / n;
  const meanY = y.reduce((s, v) => s + v, 0) / n;
  let cov = 0;
  let varX = 0;
  let varY = 0;
  for (let i = 0; i < n; i += 1) {
    const dx = x[i]! - meanX;
    const dy = y[i]! - meanY;
    cov += dx * dy;
    varX += dx * dx;
    varY += dy * dy;
  }
  const denom = Math.sqrt(varX * varY);
  if (denom <= 0 || !Number.isFinite(denom)) {
    return 0;
  }
  return Math.max(-1, Math.min(1, cov / denom));
}

/**
 * Reject correlation inputs whose variance cannot be computed (non-finite
 * values) or is exactly zero (a constant column) — mirrors the zero-variance
 * rejections in ttest.ts / anova.ts / diagnostic.ts. Checked on the *original*
 * series, not derived ranks, so a constant column is rejected for Spearman
 * too (the ranks of a constant column are themselves constant, so checking
 * post-rank would silently let a degenerate input through the rank step
 * before failing later, or not fail at all).
 */
function assertNonConstant(values: readonly number[], method: string, label: string): void {
  if (values.some((v) => !Number.isFinite(v))) {
    throw new Error(`${method} correlation requires ${label} to contain finite values.`);
  }
  const mean = values.reduce((s, v) => s + v, 0) / values.length;
  const sumSquares = values.reduce((s, v) => s + (v - mean) ** 2, 0);
  if (sumSquares <= 0) {
    throw new Error(`${method} correlation requires ${label} to have non-zero variance (all values are identical).`);
  }
}

function fisherZCi(r: number, n: number, alpha: number): { ciLower: number; ciUpper: number } {
  if (n <= 3 || Math.abs(r) >= 1) {
    return Math.abs(r) >= 1 ? { ciLower: r, ciUpper: r } : { ciLower: -1, ciUpper: 1 };
  }
  const zCrit = inverseNormal(1 - alpha / 2);
  const se = 1 / Math.sqrt(n - 3);
  const zr = 0.5 * Math.log((1 + r) / (1 - r));
  const lo = zr - zCrit * se;
  const hi = zr + zCrit * se;
  return { ciLower: Math.tanh(lo), ciUpper: Math.tanh(hi) };
}

function correlationFrom(method: 'pearson' | 'spearman', r: number, n: number, alpha: number): CorrelationResult {
  const df = n - 2;
  // t = r * sqrt(df / (1 - r^2))
  const denom = 1 - r * r;
  const tStatistic = denom > 0
    ? r * Math.sqrt(df / denom)
    : Math.sign(r) * Number.POSITIVE_INFINITY;
  const pValue = tDistributionPValue(tStatistic, df);
  const { ciLower, ciUpper } = fisherZCi(r, n, alpha);
  return { method, n, r, tStatistic, df, pValue, ciLower, ciUpper, alpha };
}

/** Pearson correlation test. */
export function pearsonCorrelation(x: readonly number[], y: readonly number[], alpha = 0.05): CorrelationResult {
  if (x.length !== y.length) throw new Error('Correlation requires equal-length samples.');
  if (x.length < 3) throw new Error('Correlation requires at least 3 observations.');
  assertNonConstant(x, 'Pearson', 'x');
  assertNonConstant(y, 'Pearson', 'y');
  return correlationFrom('pearson', pearsonR(x, y), x.length, alpha);
}

/** Spearman rank correlation test. */
export function spearmanCorrelation(x: readonly number[], y: readonly number[], alpha = 0.05): CorrelationResult {
  if (x.length !== y.length) throw new Error('Correlation requires equal-length samples.');
  if (x.length < 3) throw new Error('Correlation requires at least 3 observations.');
  // Check variance on the raw inputs, not the ranks: a constant column's
  // ranks are themselves constant (tied at the mean rank), so this must run
  // before rankWithTies to catch the degenerate case at the true source.
  assertNonConstant(x, 'Spearman', 'x');
  assertNonConstant(y, 'Spearman', 'y');
  const rankX = rankWithTies(x);
  const rankY = rankWithTies(y);
  return correlationFrom('spearman', pearsonR(rankX, rankY), x.length, alpha);
}

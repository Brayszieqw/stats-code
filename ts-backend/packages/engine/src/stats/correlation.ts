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

/** Pearson product-moment correlation coefficient. */
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
  return correlationFrom('pearson', pearsonR(x, y), x.length, alpha);
}

/** Spearman rank correlation test. */
export function spearmanCorrelation(x: readonly number[], y: readonly number[], alpha = 0.05): CorrelationResult {
  if (x.length !== y.length) throw new Error('Correlation requires equal-length samples.');
  if (x.length < 3) throw new Error('Correlation requires at least 3 observations.');
  const rankX = rankWithTies(x);
  const rankY = rankWithTies(y);
  return correlationFrom('spearman', pearsonR(rankX, rankY), x.length, alpha);
}

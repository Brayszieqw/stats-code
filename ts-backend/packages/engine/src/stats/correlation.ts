// stats/correlation.ts — Pearson & Spearman correlation (Phase 2, task 5.3).
// Transcribed from crates/stats-code/src/stats/correlation.rs.

import { tDistributionPValue } from '../math/distributions.js';
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

/** Inverse normal CDF (A&S 26.2.23) used for Fisher-z CI back-transform. */
function normQuantile(p: number): number {
  if (p <= 0) return Number.NEGATIVE_INFINITY;
  if (p >= 1) return Number.POSITIVE_INFINITY;
  const lower = p > 0.5 ? 1 - p : p;
  const t = Math.sqrt(-2 * Math.log(lower));
  const c0 = 2.515517;
  const c1 = 0.802853;
  const c2 = 0.010328;
  const d1 = 1.432788;
  const d2 = 0.189269;
  const d3 = 0.001308;
  const num = c0 + c1 * t + c2 * t * t;
  const den = 1 + d1 * t + d2 * t * t + d3 * t * t * t;
  const z = t - num / den;
  return p < 0.5 ? -z : z;
}

function fisherZCi(r: number, n: number, alpha: number): { ciLower: number; ciUpper: number } {
  if (n <= 3 || Math.abs(r) >= 1) {
    return Math.abs(r) >= 1 ? { ciLower: r, ciUpper: r } : { ciLower: -1, ciUpper: 1 };
  }
  const zCrit = normQuantile(1 - alpha / 2);
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
  const tStatistic = denom > 0 ? r * Math.sqrt(df / denom) : Number.POSITIVE_INFINITY;
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

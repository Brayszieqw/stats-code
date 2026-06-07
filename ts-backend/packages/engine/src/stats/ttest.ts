// stats/ttest.ts — Student's t-tests (Phase 2, task 5.3).
// Transcribed from crates/stats-code/src/stats/ttest.rs and math welch_t_statistic.
// Pure functions over numeric arrays; CSV parsing lives in the handler layer.

import { tDistributionPValue, tDistributionCriticalValue } from '../math/distributions.js';

export interface TtestResult {
  method: string;
  n: number;
  mean: number;
  sd: number;
  se: number;
  tStatistic: number;
  df: number;
  pValue: number;
  ciLower: number;
  ciUpper: number;
  alpha: number;
}

function meanAndVariance(values: readonly number[]): { mean: number; variance: number } {
  const n = values.length;
  const mean = values.reduce((s, v) => s + v, 0) / n;
  const variance = values.reduce((s, v) => s + (v - mean) ** 2, 0) / (n - 1);
  return { mean, variance };
}

/** One-sample t-test of H0: mean = mu. */
export function oneSampleTtest(values: readonly number[], mu: number, alpha = 0.05): TtestResult {
  const n = values.length;
  if (n < 2) {
    throw new Error('One-sample t-test requires at least 2 observations.');
  }
  const { mean, variance } = meanAndVariance(values);
  if (variance <= 0) {
    throw new Error('One-sample t-test: variance is zero (all values identical).');
  }
  const sd = Math.sqrt(variance);
  const se = sd / Math.sqrt(n);
  const tStatistic = (mean - mu) / se;
  const df = n - 1;
  const pValue = tDistributionPValue(tStatistic, df);
  const tCrit = tDistributionCriticalValue(alpha, df);
  return {
    method: 'One-sample t-test',
    n,
    mean,
    sd,
    se,
    tStatistic,
    df,
    pValue,
    ciLower: mean - tCrit * se,
    ciUpper: mean + tCrit * se,
    alpha,
  };
}

/** Paired t-test on after - before differences. */
export function pairedTtest(
  before: readonly number[],
  after: readonly number[],
  alpha = 0.05,
): TtestResult {
  if (before.length !== after.length) {
    throw new Error('Paired t-test requires equal-length samples.');
  }
  const diffs = after.map((a, i) => a - before[i]!);
  const n = diffs.length;
  if (n < 2) {
    throw new Error('Paired t-test requires at least 2 complete pairs.');
  }
  const { mean, variance } = meanAndVariance(diffs);
  if (variance <= 0) {
    throw new Error('Paired t-test: differences are identical (zero variance).');
  }
  const sd = Math.sqrt(variance);
  const se = sd / Math.sqrt(n);
  const tStatistic = mean / se;
  const df = n - 1;
  const pValue = tDistributionPValue(tStatistic, df);
  const tCrit = tDistributionCriticalValue(alpha, df);
  return {
    method: 'Paired t-test',
    n,
    mean,
    sd,
    se,
    tStatistic,
    df,
    pValue,
    ciLower: mean - tCrit * se,
    ciUpper: mean + tCrit * se,
    alpha,
  };
}

export interface WelchResult extends TtestResult {
  mean1: number;
  mean2: number;
  meanDiff: number;
}

/** Welch's two-sample t-test (unequal variances), matching math::welch_t_statistic. */
export function welchTtest(a: readonly number[], b: readonly number[], alpha = 0.05): WelchResult {
  const n1 = a.length;
  const n2 = b.length;
  if (n1 < 2 || n2 < 2) {
    throw new Error("Welch's t-test requires at least 2 observations per group.");
  }
  const m1 = meanAndVariance(a);
  const m2 = meanAndVariance(b);
  const se = Math.sqrt(m1.variance / n1 + m2.variance / n2);
  if (se <= 0 || !Number.isFinite(se)) {
    throw new Error("Welch's t-test: zero standard error.");
  }
  const meanDiff = m1.mean - m2.mean;
  const tStatistic = meanDiff / se;
  const numerator = (m1.variance / n1 + m2.variance / n2) ** 2;
  const denominator =
    (m1.variance / n1) ** 2 / (n1 - 1) + (m2.variance / n2) ** 2 / (n2 - 1);
  const df = denominator > 0 ? numerator / denominator : 1;
  const pValue = tDistributionPValue(tStatistic, df);
  const tCrit = tDistributionCriticalValue(alpha, df);
  return {
    method: "Welch's t-test",
    n: n1 + n2,
    mean: meanDiff,
    sd: se * Math.sqrt(n1 + n2),
    se,
    tStatistic,
    df,
    pValue,
    ciLower: meanDiff - tCrit * se,
    ciUpper: meanDiff + tCrit * se,
    alpha,
    mean1: m1.mean,
    mean2: m2.mean,
    meanDiff,
  };
}

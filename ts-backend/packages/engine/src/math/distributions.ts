// math/distributions.ts — probability distribution CDFs and p-values (task 5.1).
// Transcribed from crates/stats-code/src/math/{mod,distributions}.rs.

import {
  regularizedLowerGamma,
  regularizedBetaIncomplete,
  regularizedIncompleteBeta,
  clamp01,
  SQRT_2PI,
} from './special.js';

/** Standard normal CDF (Abramowitz & Stegun approximation, ~1e-7 precision). */
export function normalCdf(value: number): number {
  const absolute = Math.abs(value);
  const t = 1 / (1 + 0.2316419 * absolute);
  const density = Math.exp(-0.5 * absolute * absolute) / SQRT_2PI;
  const approximation =
    1 -
    density *
      t *
      (0.31938153 +
        t * (-0.356563782 + t * (1.781477937 + t * (-1.821255978 + t * 1.330274429))));
  return value >= 0 ? approximation : 1 - approximation;
}

/** Chi-square CDF using the regularized lower incomplete gamma function. */
export function chiSquareCdf(x: number, df: number): number {
  if (x <= 0 || df <= 0) return 0;
  return regularizedLowerGamma(df / 2, x / 2);
}

/** Upper-tail chi-square p-value. */
export function chiSquareP(x: number, df: number): number {
  if (x <= 0 || df <= 0) return 1;
  return clamp01(1 - chiSquareCdf(x, df));
}

/** Upper-tail F-distribution p-value via the incomplete beta relationship. */
export function fDistributionPValue(f: number, df1: number, df2: number): number {
  if (f <= 0 || !Number.isFinite(f)) return 1;
  const x = (df1 * f) / (df1 * f + df2);
  const p = regularizedBetaIncomplete(x, df1 / 2, df2 / 2);
  return clamp01(1 - p);
}

/** Two-sided Student's t p-value via the incomplete beta function. */
export function tDistributionPValue(t: number, df: number): number {
  if (df <= 0 || !Number.isFinite(t)) return 1;
  const x = df / (df + t * t);
  const p = regularizedBetaIncomplete(x, df / 2, 0.5);
  return clamp01(p);
}

/** Two-sided Student's t critical value at significance level alpha (bisection). */
export function tDistributionCriticalValue(alpha: number, df: number): number {
  if (alpha <= 0 || alpha >= 1) return Number.NaN;
  if (df <= 0) return Number.NaN;
  let lo = 0;
  let hi = 50;
  while (tDistributionPValue(hi, df) > alpha) {
    hi *= 2;
    if (hi > 1e5) return hi;
  }
  for (let i = 0; i < 120; i += 1) {
    const mid = (lo + hi) / 2;
    const p = tDistributionPValue(mid, df);
    if (p < alpha) {
      hi = mid;
    } else {
      lo = mid;
    }
    if (hi - lo < 1e-12) break;
  }
  return (lo + hi) / 2;
}

/** Inverse standard normal CDF (Acklam's rational approximation). */
export function inverseNormal(p: number): number {
  if (p <= 0) return Number.NEGATIVE_INFINITY;
  if (p >= 1) return Number.POSITIVE_INFINITY;

  const A = [
    -3.969683028665376e1, 2.209460984245205e2, -2.759285104469687e2, 1.38357751867269e2,
    -3.066479806614716e1, 2.506628277459239,
  ];
  const B = [
    -5.447609879822406e1, 1.615858368580409e2, -1.556989798598866e2, 6.680131188771972e1,
    -1.328068155288572e1,
  ];
  const C = [
    -7.784894002430293e-3, -3.223964580411365e-1, -2.400758277161838, -2.549732539343734,
    4.374664141464968, 2.938163982698783,
  ];
  const D = [7.784695709041462e-3, 3.224671290700398e-1, 2.445134137142996, 3.754408661907416];

  const plow = 0.02425;
  const phigh = 1 - plow;
  if (p < plow) {
    const q = Math.sqrt(-2 * Math.log(p));
    return (
      (((((C[0]! * q + C[1]!) * q + C[2]!) * q + C[3]!) * q + C[4]!) * q + C[5]!) /
      ((((D[0]! * q + D[1]!) * q + D[2]!) * q + D[3]!) * q + 1)
    );
  }
  if (p > phigh) {
    const q = Math.sqrt(-2 * Math.log(1 - p));
    return (
      -(((((C[0]! * q + C[1]!) * q + C[2]!) * q + C[3]!) * q + C[4]!) * q + C[5]!) /
      ((((D[0]! * q + D[1]!) * q + D[2]!) * q + D[3]!) * q + 1)
    );
  }
  const q = p - 0.5;
  const r = q * q;
  return (
    ((((((A[0]! * r + A[1]!) * r + A[2]!) * r + A[3]!) * r + A[4]!) * r + A[5]!) * q) /
    (((((B[0]! * r + B[1]!) * r + B[2]!) * r + B[3]!) * r + B[4]!) * r + 1)
  );
}

/** Two-sided upper-tail p-value for Student's t (named helper). */
export function studentTTwoSided(t: number, df: number): number {
  return tDistributionPValue(t, df);
}

export { regularizedIncompleteBeta };

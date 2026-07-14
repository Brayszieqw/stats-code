// math/distributions.ts — probability distribution CDFs and p-values (task 5.1).
// Transcribed from crates/stats-code/src/math/{mod,distributions}.rs.

import {
  regularizedLowerGamma,
  regularizedUpperGammaCf,
  regularizedBetaIncomplete,
  regularizedIncompleteBeta,
  logGamma,
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
  if (Number.isNaN(f)) return Number.NaN;
  if (f === Number.POSITIVE_INFINITY) return 0;
  if (f <= 0 || f === Number.NEGATIVE_INFINITY) return 1;
  const x = (df1 * f) / (df1 * f + df2);
  const p = regularizedBetaIncomplete(x, df1 / 2, df2 / 2);
  return clamp01(1 - p);
}

/** Two-sided Student's t p-value via the incomplete beta function. */
export function tDistributionPValue(t: number, df: number): number {
  if (df <= 0) return 1;
  if (Number.isNaN(t)) return Number.NaN;
  if (!Number.isFinite(t)) return 0;
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
  let approximation: number;
  if (p < plow) {
    const q = Math.sqrt(-2 * Math.log(p));
    approximation = (
      (((((C[0]! * q + C[1]!) * q + C[2]!) * q + C[3]!) * q + C[4]!) * q + C[5]!) /
      ((((D[0]! * q + D[1]!) * q + D[2]!) * q + D[3]!) * q + 1)
    );
  } else if (p > phigh) {
    const q = Math.sqrt(-2 * Math.log(1 - p));
    approximation = (
      -(((((C[0]! * q + C[1]!) * q + C[2]!) * q + C[3]!) * q + C[4]!) * q + C[5]!) /
      ((((D[0]! * q + D[1]!) * q + D[2]!) * q + D[3]!) * q + 1)
    );
  } else {
    const q = p - 0.5;
    const r = q * q;
    approximation = (
      ((((((A[0]! * r + A[1]!) * r + A[2]!) * r + A[3]!) * r + A[4]!) * r + A[5]!) * q) /
      (((((B[0]! * r + B[1]!) * r + B[2]!) * r + B[3]!) * r + B[4]!) * r + 1)
    );
  }

  // Acklam's approximation is accurate to about 1e-9. One Halley step,
  // using the incomplete-gamma identity for Phi(x), reaches double precision.
  const gammaArgument = (approximation * approximation) / 2;
  let cdfError: number;
  if (gammaArgument > 1.5) {
    const tail = regularizedUpperGammaCf(0.5, gammaArgument) / 2;
    cdfError = approximation < 0 ? tail - p : (1 - p) - tail;
  } else {
    const gamma = regularizedLowerGamma(0.5, gammaArgument);
    const cdf = approximation >= 0 ? 0.5 + gamma / 2 : 0.5 - gamma / 2;
    cdfError = cdf - p;
  }
  const density = Math.exp((-approximation * approximation) / 2) / SQRT_2PI;
  const correction = cdfError / density;
  return Number.isFinite(correction)
    ? approximation - correction / (1 + (approximation * correction) / 2)
    : approximation;
}

/** Two-sided upper-tail p-value for Student's t (named helper). */
export function studentTTwoSided(t: number, df: number): number {
  return tDistributionPValue(t, df);
}

/**
 * CDF of the noncentral t distribution, P(T ≤ t | df, ncp).
 *
 * Algorithm AS 243 (Lenth, 1989), as implemented in R's `pnt.c`: a
 * Poisson-weighted mixture of regularized incomplete beta terms. Used by the
 * power family (PROC POWER's noncentral-t sample size). Reduces to the central
 * t CDF when ncp ≈ 0.
 */
export function noncentralTCdf(t: number, df: number, ncp: number): number {
  if (!Number.isFinite(t) || df <= 0) return Number.NaN;
  if (Math.abs(ncp) < 1e-12) {
    const upperTwoSided = tDistributionPValue(t, df);
    return t >= 0 ? 1 - upperTwoSided / 2 : upperTwoSided / 2;
  }

  const errMax = 1e-12;
  const itrMax = 1000;
  const sqrt2dPi = Math.sqrt(2 / Math.PI);

  let negdel = false;
  let tt = t;
  let del = ncp;
  if (t < 0) {
    negdel = true;
    tt = -t;
    del = -ncp;
  }

  // x = tt^2 / (tt^2 + df); the central-normal part carries the rest.
  const x = (tt * tt) / (tt * tt + df);
  let tnc = 0;

  if (x > 0) {
    const lambda = del * del;
    let p = 0.5 * Math.exp(-0.5 * lambda);
    let q = sqrt2dPi * p * del;
    let s = 0.5 - p;
    const a = 0.5;
    const b = 0.5 * df;
    const rxb = Math.pow(1 - x, b);
    const albeta = logGamma(a) + logGamma(b) - logGamma(a + b);
    let xodd = regularizedIncompleteBeta(a, b, x);
    let godd = 2 * rxb * Math.exp(a * Math.log(x) - albeta);
    let xeven = 1 - rxb;
    let geven = b * x * rxb;
    tnc = p * xodd + q * xeven;

    let aa = a;
    for (let it = 1; it <= itrMax; it += 1) {
      aa += 1;
      xodd -= godd;
      xeven -= geven;
      godd *= (x * (aa + b - 1)) / aa;
      geven *= (x * (aa + b - 0.5)) / (aa + 0.5);
      p *= lambda / (2 * it);
      q *= lambda / (2 * it + 1);
      s -= p;
      tnc += p * xodd + q * xeven;
      const errbd = 2 * s * (xodd - godd);
      if (Math.abs(errbd) < errMax) break;
    }
  }

  tnc += normalCdf(-del);
  const result = negdel ? 1 - tnc : tnc;
  return clamp01(result);
}

export { regularizedIncompleteBeta };

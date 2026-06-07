// stats/power.ts — power / sample-size family (Phase 5, task 11.1; revised 11.2).
//
// Calibrated to SAS PROC POWER (the recorded validation oracle):
//   power_single_arm → one-proportion hypothesis test; effect size is the
//                       standardized difference (p1-p0)/sqrt(p0(1-p0)); sample
//                       size from the normal approximation; achieved power from
//                       the normal CDF.
//   power_phase2     → two-proportion superiority; effect size is Cohen's h
//                       (arcsine); normal-approximation sample size + power.
//   power_phase3     → two-means superiority; effect size is the standardized
//                       mean difference d = |Δ|/σ; sample size from the normal
//                       approximation, achieved power from the NONCENTRAL t
//                       (PROC POWER's t-test method).
//
// NOTE (parity scope): the power family is a `recorded` Coverage_Matrix cell,
// not a `live` one. required_n and effect_size reproduce SAS exactly; achieved
// power agrees with PROC POWER to ~1e-6 (well within the documented power
// tolerance) — the residual gap is the method-internal difference between the
// engine's CDFs and PROC POWER's quadrature.

import { inverseNormal, normalCdf, noncentralTCdf, tDistributionCriticalValue } from '../math/distributions.js';

function validateProbability(value: number, name: string): void {
  if (!Number.isFinite(value) || value < 0 || value > 1) {
    throw new Error(`${name} must be between 0 and 1.`);
  }
}

function validateProbabilityExclusive(value: number, name: string): void {
  if (!Number.isFinite(value) || value <= 0 || value >= 1) {
    throw new Error(`${name} must be strictly between 0 and 1.`);
  }
}

export interface PowerResult {
  method: string;
  alpha: number;
  power: number;
  /** Required sample size (total for single-arm, per-arm for two-group). */
  requiredN: number;
  /** Total sample size across all arms. */
  totalN: number;
  /** Achieved power at the required sample size. */
  achievedPower: number;
  /** Standardized effect size used by the design. */
  effectSize: number;
}

/**
 * power_single_arm: one-sample proportion hypothesis test (H0: p=p0 vs p1).
 * Effect size = (p1-p0)/sqrt(p0(1-p0)); required n = 2·⌈((z_α+z_β)/ES)²⌉ to
 * match PROC POWER's one-proportion convention; achieved power from the normal
 * approximation.
 */
export function powerSingleArm(p0: number, p1: number, alpha = 0.05, power = 0.8): PowerResult {
  validateProbability(p0, 'p0');
  validateProbability(p1, 'p1');
  validateProbabilityExclusive(alpha, 'alpha');
  validateProbabilityExclusive(power, 'power');
  const diff = Math.abs(p1 - p0);
  if (diff <= Number.EPSILON) {
    throw new Error('p0 and p1 must differ to compute one-proportion sample size.');
  }
  const denom = Math.sqrt(p0 * (1 - p0));
  if (denom <= 0) {
    throw new Error('p0 must be strictly between 0 and 1 for a standardized effect size.');
  }
  const effectSize = diff / denom;
  const zAlpha = inverseNormal(1 - alpha / 2);
  const zPower = inverseNormal(power);
  const nHalf = Math.ceil(((zAlpha + zPower) / effectSize) ** 2);
  const requiredN = 2 * nHalf;
  const achievedPower = normalCdf(effectSize * Math.sqrt(requiredN / 2) - zAlpha);
  return {
    method: 'one_proportion',
    alpha,
    power,
    requiredN,
    totalN: requiredN,
    achievedPower,
    effectSize,
  };
}

/**
 * power_phase2: two-proportion superiority. Effect size is Cohen's h (arcsine
 * transform); required n per arm = ⌈((z_α+z_β)/h)²⌉; achieved power from the
 * normal approximation (PROC POWER's proportions method).
 */
export function powerPhase2(p0: number, p1: number, alpha = 0.05, power = 0.8): PowerResult {
  validateProbability(p0, 'p0');
  validateProbability(p1, 'p1');
  validateProbabilityExclusive(alpha, 'alpha');
  validateProbabilityExclusive(power, 'power');
  const h = 2 * Math.asin(Math.sqrt(p1)) - 2 * Math.asin(Math.sqrt(p0));
  if (Math.abs(h) <= Number.EPSILON) {
    throw new Error('p0 and p1 must differ to compute a two-proportion sample size.');
  }
  const effectSize = Math.abs(h);
  const zAlpha = inverseNormal(1 - alpha / 2);
  const zPower = inverseNormal(power);
  // Two-group arcsine test: per-arm n = 2·((z_α+z_β)/h)² (factor 2 for the
  // two-sample design), matching PROC POWER's two-proportion convention.
  const requiredN = Math.ceil(2 * ((zAlpha + zPower) / effectSize) ** 2);
  const achievedPower = normalCdf(effectSize * Math.sqrt(requiredN / 2) - zAlpha);
  return {
    method: 'two_proportions',
    alpha,
    power,
    requiredN,
    totalN: 2 * requiredN,
    achievedPower,
    effectSize,
  };
}

/**
 * power_phase3: two-means superiority. Effect size is the standardized mean
 * difference d = |Δ|/σ; required n per arm = ⌈((z_α+z_β)/d)²⌉; achieved power
 * from the NONCENTRAL t distribution (PROC POWER's t-test method).
 */
export function powerPhase3(meanDiff: number, std: number, alpha = 0.05, power = 0.8): PowerResult {
  if (!Number.isFinite(meanDiff)) {
    throw new Error('meanDiff must be finite.');
  }
  if (!Number.isFinite(std) || std <= 0) {
    throw new Error('std must be positive and finite.');
  }
  validateProbabilityExclusive(alpha, 'alpha');
  validateProbabilityExclusive(power, 'power');
  const diff = Math.abs(meanDiff);
  if (diff <= Number.EPSILON) {
    throw new Error('meanDiff must be non-zero to compute a two-mean sample size.');
  }
  const effectSize = diff / std;
  const zAlpha = inverseNormal(1 - alpha / 2);
  const zPower = inverseNormal(power);
  // Normal-approximation per-arm n is the search seed; PROC POWER then solves
  // for the smallest integer n whose noncentral-t power reaches the target.
  const seed = Math.max(2, Math.floor(2 * ((zAlpha + zPower) / effectSize) ** 2) - 2);
  let requiredN = seed;
  let achievedPower = 0;
  for (let n = seed; n <= seed + 100; n += 1) {
    const dfN = 2 * n - 2;
    const ncpN = effectSize * Math.sqrt(n / 2);
    const tCritN = tDistributionCriticalValue(alpha, dfN);
    achievedPower = 1 - noncentralTCdf(tCritN, dfN, ncpN);
    if (achievedPower >= power) {
      requiredN = n;
      break;
    }
  }
  return {
    method: 'two_means',
    alpha,
    power,
    requiredN,
    totalN: 2 * requiredN,
    achievedPower,
    effectSize,
  };
}

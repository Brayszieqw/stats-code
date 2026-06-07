// stats/power.ts — power / sample-size family (Phase 5, task 11.1).
// Transcribed from crates/stats-code/src/power.rs. Uses the inverse normal CDF
// for the z critical values; all formulas use the normal approximation.
//
// Mapping to the Output-Level Algorithm ids:
//   power_single_arm → one-proportion precision (Wald) sample size
//   power_phase2     → two-proportion superiority sample size
//   power_phase3     → two-means superiority sample size

import { inverseNormal } from '../math/distributions.js';

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

function validateAllocation(value: number): void {
  if (!Number.isFinite(value) || value <= 0) {
    throw new Error('allocationRatio must be positive and finite.');
  }
}

export interface OneProportionResult {
  method: 'one_proportion_precision';
  alpha: number;
  totalN: number;
  margin: number;
}

/**
 * power_single_arm: one-sample proportion precision sample size (Wald normal
 * approximation). n = z²·p·(1-p) / margin².
 */
export function powerSingleArm(proportion: number, margin: number, alpha = 0.05): OneProportionResult {
  validateProbability(proportion, 'proportion');
  validateProbabilityExclusive(alpha, 'alpha');
  if (!Number.isFinite(margin) || margin <= 0) {
    throw new Error('margin must be positive and finite.');
  }
  const zAlpha = inverseNormal(1 - alpha / 2);
  const n = Math.ceil((zAlpha * zAlpha * proportion * (1 - proportion)) / (margin * margin));
  return { method: 'one_proportion_precision', alpha, totalN: n, margin };
}

export interface TwoGroupResult {
  method: string;
  alpha: number;
  power: number;
  allocationRatio: number;
  totalN: number;
  group1N: number;
  group2N: number;
  effectSize: number;
}

/**
 * power_phase2: two-proportion superiority sample size (normal approximation,
 * two-sided). allocationRatio = n2/n1.
 */
export function powerPhase2(
  p1: number,
  p2: number,
  alpha = 0.05,
  power = 0.8,
  allocationRatio = 1,
): TwoGroupResult {
  validateProbability(p1, 'p1');
  validateProbability(p2, 'p2');
  validateProbabilityExclusive(alpha, 'alpha');
  validateProbabilityExclusive(power, 'power');
  validateAllocation(allocationRatio);
  const diff = Math.abs(p1 - p2);
  if (diff <= Number.EPSILON) {
    throw new Error('p1 and p2 must differ to compute two-proportion sample size.');
  }
  const zAlpha = inverseNormal(1 - alpha / 2);
  const zPower = inverseNormal(power);
  const a = allocationRatio;
  const pooled = (p1 + a * p2) / (1 + a);
  const varNull = pooled * (1 - pooled) * (1 + 1 / a);
  const varAlt = p1 * (1 - p1) + (p2 * (1 - p2)) / a;
  const n1 = Math.ceil((zAlpha * Math.sqrt(varNull) + zPower * Math.sqrt(varAlt)) ** 2 / (diff * diff));
  const n2 = Math.ceil(n1 * a);
  return {
    method: 'two_independent_proportions',
    alpha,
    power,
    allocationRatio: a,
    totalN: n1 + n2,
    group1N: n1,
    group2N: n2,
    effectSize: diff,
  };
}

/**
 * power_phase3: two-means superiority sample size (normal approximation, common
 * SD). allocationRatio = n2/n1; effectSize is the standardized difference.
 */
export function powerPhase3(
  mean1: number,
  mean2: number,
  sd: number,
  alpha = 0.05,
  power = 0.8,
  allocationRatio = 1,
): TwoGroupResult {
  if (!Number.isFinite(mean1) || !Number.isFinite(mean2)) {
    throw new Error('mean1 and mean2 must be finite.');
  }
  if (!Number.isFinite(sd) || sd <= 0) {
    throw new Error('sd must be positive and finite.');
  }
  validateProbabilityExclusive(alpha, 'alpha');
  validateProbabilityExclusive(power, 'power');
  validateAllocation(allocationRatio);
  const diff = Math.abs(mean1 - mean2);
  if (diff <= Number.EPSILON) {
    throw new Error('mean1 and mean2 must differ to compute two-mean sample size.');
  }
  const zAlpha = inverseNormal(1 - alpha / 2);
  const zPower = inverseNormal(power);
  const a = allocationRatio;
  const n1 = Math.ceil(((zAlpha + zPower) ** 2 * sd * sd * (1 + 1 / a)) / (diff * diff));
  const n2 = Math.ceil(n1 * a);
  return {
    method: 'two_independent_means',
    alpha,
    power,
    allocationRatio: a,
    totalN: n1 + n2,
    group1N: n1,
    group2N: n2,
    effectSize: diff / sd,
  };
}

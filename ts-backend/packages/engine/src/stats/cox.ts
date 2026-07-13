// stats/cox.ts — Cox proportional hazards via Newton-Raphson with Efron tie
// handling (Phase 4, task 9.1). Transcribed exactly from
// crates/stats-code/src/cox.rs (fit_cox_newton + cox_partial_stats).
//
// Efron approximation matches lifelines and R survival's ties="efron". The
// risk-score sums are computed on a shifted scale (subtract max_eta) for
// numerical stability, then the max_eta is added back into the log partial
// likelihood term.

import { chiSquareP } from '../math/distributions.js';
import {
  dot,
  invertMatrix,
  invertMatrixWithRidge,
  matrixMultiply,
  matrixVectorMul,
  transpose,
  type Matrix,
} from '../math/linalg.js';

const MAX_ITERATIONS = 50;
const TOLERANCE = 1e-8;

export interface CoxObservation {
  time: number;
  event: boolean;
  x: number[];
  weight: number;
}

export interface CoxPartialStats {
  logPartialLikelihood: number;
  gradient: number[];
  information: Matrix;
}

export interface CoxFit {
  beta: number[];
  standardErrors: number[];
  iterations: number;
  converged: boolean;
  logPartialLikelihood: number;
}

export interface CoxCoefficient {
  index: number;
  beta: number;
  hazardRatio: number;
  stdError: number;
  z: number;
  pValue: number;
  ciLower: number;
  ciUpper: number;
}

export interface CoxResult {
  coefficients: CoxCoefficient[];
  logPartialLikelihood: number;
  iterations: number;
  converged: boolean;
  tiedEventTimes: number;
}

export type CoxPhUnavailableReason = 'no_events' | 'singular_information' | 'invalid_fit';

export interface CoxPhCovariateTest {
  index: number;
  statistic: number | null;
  pValue: number | null;
  violated: boolean;
}

export type CoxPhTestResult = {
  status: 'computed';
  method: 'time_interaction_score';
  timeTransform: 'centered_log1p';
  alpha: number;
  statistic: number;
  degreesOfFreedom: number;
  pValue: number;
  violated: boolean;
  covariates: CoxPhCovariateTest[];
} | {
  status: 'not_computed';
  method: 'time_interaction_score';
  timeTransform: 'centered_log1p';
  alpha: number;
  statistic: null;
  degreesOfFreedom: null;
  pValue: null;
  violated: false;
  covariates: CoxPhCovariateTest[];
  reason: CoxPhUnavailableReason;
};

const Z_95 = 1.959963984540054;

function validateCoxObservations(observations: readonly CoxObservation[]): number {
  const p = observations.length > 0 ? observations[0]!.x.length : 0;
  if (p === 0) throw new Error('Empty Cox design matrix.');
  for (const observation of observations) {
    if (observation.x.length !== p || observation.x.some((value) => !Number.isFinite(value))) {
      throw new Error('Cox covariates must form a finite rectangular matrix.');
    }
    if (!Number.isFinite(observation.time) || observation.time < 0) {
      throw new Error('Cox survival times must be finite non-negative numbers.');
    }
    if (!Number.isFinite(observation.weight) || observation.weight <= 0) {
      throw new Error('Cox observation weights must be finite positive numbers.');
    }
  }
  if (!observations.some((observation) => observation.event)) {
    throw new Error('Cox regression requires at least one observed event; all records are censored.');
  }
  return p;
}

/** Standard normal two-sided p-value (matches 2*(1 - Φ(|z|))). */
function normalCdfAbramowitz(x: number): number {
  const a = Math.abs(x);
  const t = 1 / (1 + 0.2316419 * a);
  const density = Math.exp(-0.5 * a * a) / 2.5066282746310002;
  const approx =
    1 -
    density *
      t *
      (0.31938153 + t * (-0.356563782 + t * (1.781477937 + t * (-1.821255978 + t * 1.330274429))));
  return x >= 0 ? approx : 1 - approx;
}

/** Compute (logPL, gradient, information) with the Efron tie approximation. */
export function coxPartialStats(observations: readonly CoxObservation[], beta: readonly number[]): CoxPartialStats {
  const p = beta.length;

  const eventEntries = observations
    .map((obs, index) => ({ index, time: obs.time, event: obs.event }))
    .filter((e) => e.event)
    .sort((a, b) => a.time - b.time);

  let logPartialLikelihood = 0;
  const gradient = new Array<number>(p).fill(0);
  const information: Matrix = Array.from({ length: p }, () => new Array(p).fill(0));
  const linearPredictors = observations.map((obs) => dot(obs.x, beta));

  let eventPosition = 0;
  while (eventPosition < eventEntries.length) {
    const time = eventEntries[eventPosition]!.time;
    const eventIndices: number[] = [];
    while (eventPosition < eventEntries.length && eventEntries[eventPosition]!.time === time) {
      eventIndices.push(eventEntries[eventPosition]!.index);
      eventPosition += 1;
    }

    const d = eventIndices.reduce((s, idx) => s + observations[idx]!.weight, 0);
    const dCount = eventIndices.length;

    let s0 = 0;
    const s1 = new Array<number>(p).fill(0);
    const s2: Matrix = Array.from({ length: p }, () => new Array(p).fill(0));
    let se0 = 0;
    const se1 = new Array<number>(p).fill(0);
    const se2: Matrix = Array.from({ length: p }, () => new Array(p).fill(0));

    const riskSetIndices: number[] = [];
    for (let i = 0; i < observations.length; i += 1) {
      if (observations[i]!.time >= time) riskSetIndices.push(i);
    }
    let maxEta = Number.NEGATIVE_INFINITY;
    for (const idx of riskSetIndices) {
      if (linearPredictors[idx]! > maxEta) maxEta = linearPredictors[idx]!;
    }
    if (!Number.isFinite(maxEta)) {
      throw new Error('Invalid Cox risk set encountered during fitting.');
    }

    for (const idx of riskSetIndices) {
      const obs = observations[idx]!;
      const riskScore = obs.weight * Math.exp(linearPredictors[idx]! - maxEta);
      s0 += riskScore;
      for (let j = 0; j < p; j += 1) {
        s1[j]! += riskScore * obs.x[j]!;
        for (let k = 0; k < p; k += 1) {
          s2[j]![k]! += riskScore * obs.x[j]! * obs.x[k]!;
        }
      }
    }
    for (const idx of eventIndices) {
      const obs = observations[idx]!;
      const riskScore = obs.weight * Math.exp(linearPredictors[idx]! - maxEta);
      se0 += riskScore;
      for (let j = 0; j < p; j += 1) {
        se1[j]! += riskScore * obs.x[j]!;
        for (let k = 0; k < p; k += 1) {
          se2[j]![k]! += riskScore * obs.x[j]! * obs.x[k]!;
        }
      }
    }
    if (s0 <= 0 || !Number.isFinite(s0)) {
      throw new Error('Invalid Cox risk set encountered during fitting.');
    }

    const eventSum = new Array<number>(p).fill(0);
    for (const idx of eventIndices) {
      const eventWeight = observations[idx]!.weight;
      logPartialLikelihood += eventWeight * linearPredictors[idx]!;
      for (let j = 0; j < p; j += 1) {
        eventSum[j]! += eventWeight * observations[idx]!.x[j]!;
      }
    }

    const dSteps = Math.max(eventIndices.length, 1);
    for (let step = 0; step < dSteps; step += 1) {
      const fraction = step / dCount;
      const denom = s0 - fraction * se0;
      if (denom <= 0 || !Number.isFinite(denom)) {
        throw new Error('Invalid Cox risk set encountered during fitting.');
      }
      logPartialLikelihood -= (d / dCount) * (maxEta + Math.log(denom));
      for (let j = 0; j < p; j += 1) {
        const numJ = s1[j]! - fraction * se1[j]!;
        gradient[j]! -= ((d / dCount) * numJ) / denom;
        for (let k = 0; k < p; k += 1) {
          const numJk = s2[j]![k]! - fraction * se2[j]![k]!;
          information[j]![k]! +=
            (d / dCount) * (numJk / denom - (numJ * (s1[k]! - fraction * se1[k]!)) / (denom * denom));
        }
      }
    }
    for (let j = 0; j < p; j += 1) {
      gradient[j]! += eventSum[j]!;
    }
  }

  return { logPartialLikelihood, gradient, information };
}

/** Count distinct tied event times (groups with >1 event). */
export function countTiedEventTimes(observations: readonly CoxObservation[]): number {
  const eventTimes = observations
    .filter((o) => o.event)
    .map((o) => o.time)
    .sort((a, b) => a - b);
  let tied = 0;
  let index = 0;
  while (index < eventTimes.length) {
    const time = eventTimes[index]!;
    let groupSize = 1;
    index += 1;
    while (index < eventTimes.length && eventTimes[index]! === time) {
      groupSize += 1;
      index += 1;
    }
    if (groupSize > 1) tied += 1;
  }
  return tied;
}

/** Fit Cox via Newton-Raphson (max 50 iters, tol 1e-8). */
export function fitCoxNewton(observations: readonly CoxObservation[]): CoxFit {
  const p = validateCoxObservations(observations);
  const beta = new Array<number>(p).fill(0);
  let converged = false;
  let iterations = 0;

  for (let iter = 0; iter < MAX_ITERATIONS; iter += 1) {
    const { gradient, information } = coxPartialStats(observations, beta);
    const informationInverse = invertMatrixWithRidge(information);
    const step = matrixVectorMul(informationInverse, gradient);
    let maxStep = 0;
    for (let index = 0; index < p; index += 1) {
      maxStep = Math.max(maxStep, Math.abs(step[index]!));
      beta[index]! += step[index]!;
    }
    iterations = iter + 1;
    if (maxStep < TOLERANCE) {
      converged = true;
      break;
    }
  }

  const final = coxPartialStats(observations, beta);
  const covariance = invertMatrixWithRidge(final.information);
  const standardErrors = Array.from({ length: p }, (_, index) =>
    Math.sqrt(Math.max(covariance[index]![index]!, 0)),
  );

  return {
    beta,
    standardErrors,
    iterations,
    converged,
    logPartialLikelihood: final.logPartialLikelihood,
  };
}

/**
 * Global proportional-hazards score diagnostic for covariate×time interactions.
 * The time basis is centered log1p(event time); Efron tie handling matches the
 * fitted Cox partial likelihood. This is intentionally not labelled cox.zph.
 */
export function coxProportionalHazardsTest(
  observations: readonly CoxObservation[],
  beta: readonly number[],
  alpha = 0.05,
): CoxPhTestResult {
  const p = validateCoxObservations(observations);
  const empty = (reason: CoxPhUnavailableReason): CoxPhTestResult => ({
    status: 'not_computed',
    method: 'time_interaction_score',
    timeTransform: 'centered_log1p',
    alpha,
    statistic: null,
    degreesOfFreedom: null,
    pValue: null,
    violated: false,
    covariates: Array.from({ length: p }, (_, index) => ({ index, statistic: null, pValue: null, violated: false })),
    reason,
  });
  if (beta.length !== p || beta.some((value) => !Number.isFinite(value)) || alpha <= 0 || alpha >= 1) {
    return empty('invalid_fit');
  }

  const eventObservations = observations.filter((observation) => observation.event);
  if (eventObservations.length === 0) return empty('no_events');
  const totalEventWeight = eventObservations.reduce((sum, observation) => sum + observation.weight, 0);
  const center = eventObservations.reduce(
    (sum, observation) => sum + observation.weight * Math.log1p(observation.time),
    0,
  ) / totalEventWeight;
  const linearPredictors = observations.map((observation) => dot(observation.x, beta));
  const eventTimes = [...new Set(eventObservations.map((observation) => observation.time))].sort((a, b) => a - b);
  const score = new Array<number>(p).fill(0);
  const informationBase: Matrix = Array.from({ length: p }, () => new Array(p).fill(0));
  const informationCross: Matrix = Array.from({ length: p }, () => new Array(p).fill(0));
  const informationTime: Matrix = Array.from({ length: p }, () => new Array(p).fill(0));

  for (const time of eventTimes) {
    const eventIndices = observations
      .map((observation, index) => observation.event && observation.time === time ? index : -1)
      .filter((index) => index >= 0);
    const riskIndices = observations
      .map((observation, index) => observation.time >= time ? index : -1)
      .filter((index) => index >= 0);
    const maxEta = Math.max(...riskIndices.map((index) => linearPredictors[index]!));
    if (!Number.isFinite(maxEta)) return empty('invalid_fit');

    let s0 = 0;
    let eventS0 = 0;
    const s1 = new Array<number>(p).fill(0);
    const eventS1 = new Array<number>(p).fill(0);
    const s2: Matrix = Array.from({ length: p }, () => new Array(p).fill(0));
    const eventS2: Matrix = Array.from({ length: p }, () => new Array(p).fill(0));
    const eventSum = new Array<number>(p).fill(0);
    for (const index of riskIndices) {
      const observation = observations[index]!;
      const riskScore = observation.weight * Math.exp(linearPredictors[index]! - maxEta);
      s0 += riskScore;
      for (let row = 0; row < p; row += 1) {
        s1[row]! += riskScore * observation.x[row]!;
        for (let column = 0; column < p; column += 1) {
          s2[row]![column]! += riskScore * observation.x[row]! * observation.x[column]!;
        }
      }
    }
    for (const index of eventIndices) {
      const observation = observations[index]!;
      const riskScore = observation.weight * Math.exp(linearPredictors[index]! - maxEta);
      eventS0 += riskScore;
      for (let row = 0; row < p; row += 1) {
        eventS1[row]! += riskScore * observation.x[row]!;
        eventSum[row]! += observation.weight * observation.x[row]!;
        for (let column = 0; column < p; column += 1) {
          eventS2[row]![column]! += riskScore * observation.x[row]! * observation.x[column]!;
        }
      }
    }

    const totalEventWeightAtTime = eventIndices.reduce((sum, index) => sum + observations[index]!.weight, 0);
    const expectedSum = new Array<number>(p).fill(0);
    const timeBasis = Math.log1p(time) - center;
    for (let step = 0; step < eventIndices.length; step += 1) {
      const fraction = step / eventIndices.length;
      const denominator = s0 - fraction * eventS0;
      if (denominator <= 0 || !Number.isFinite(denominator)) return empty('invalid_fit');
      const stepWeight = totalEventWeightAtTime / eventIndices.length;
      const mean = s1.map((value, index) => (value - fraction * eventS1[index]!) / denominator);
      for (let row = 0; row < p; row += 1) {
        expectedSum[row]! += stepWeight * mean[row]!;
        for (let column = 0; column < p; column += 1) {
          const secondMoment = (s2[row]![column]! - fraction * eventS2[row]![column]!) / denominator;
          const covariance = secondMoment - mean[row]! * mean[column]!;
          const information = stepWeight * covariance;
          informationBase[row]![column]! += information;
          informationCross[row]![column]! += timeBasis * information;
          informationTime[row]![column]! += timeBasis * timeBasis * information;
        }
      }
    }
    for (let index = 0; index < p; index += 1) {
      score[index]! += timeBasis * (eventSum[index]! - expectedSum[index]!);
    }
  }

  try {
    const inverseBase = invertMatrix(informationBase);
    const nuisanceAdjustment = matrixMultiply(
      transpose(informationCross),
      matrixMultiply(inverseBase, informationCross),
    );
    const adjustedInformation = informationTime.map((row, rowIndex) =>
      row.map((value, columnIndex) => value - nuisanceAdjustment[rowIndex]![columnIndex]!));
    const inverseAdjusted = invertMatrix(adjustedInformation);
    const projected = matrixVectorMul(inverseAdjusted, score);
    const statistic = Math.max(0, dot(score, projected));
    if (!Number.isFinite(statistic)) return empty('singular_information');
    const pValue = chiSquareP(statistic, p);
    const covariates = score.map((value, index) => {
      const variance = adjustedInformation[index]![index]!;
      if (!(variance > 1e-12) || !Number.isFinite(variance)) {
        return { index, statistic: null, pValue: null, violated: false };
      }
      const covariateStatistic = Math.max(0, value * value / variance);
      const covariatePValue = chiSquareP(covariateStatistic, 1);
      return { index, statistic: covariateStatistic, pValue: covariatePValue, violated: covariatePValue < alpha };
    });
    return {
      status: 'computed',
      method: 'time_interaction_score',
      timeTransform: 'centered_log1p',
      alpha,
      statistic,
      degreesOfFreedom: p,
      pValue,
      violated: pValue < alpha,
      covariates,
    };
  } catch {
    return empty('singular_information');
  }
}

/** Full Cox regression: fit, hazard ratios, Wald tests, CIs. */
export function coxRegression(observations: readonly CoxObservation[]): CoxResult {
  const fit = fitCoxNewton(observations);
  const coefficients: CoxCoefficient[] = fit.beta.map((beta, index) => {
    const stdError = fit.standardErrors[index]!;
    const z = stdError > 0 ? beta / stdError : Number.POSITIVE_INFINITY;
    const pValue = 2 * (1 - normalCdfAbramowitz(Math.abs(z)));
    return {
      index,
      beta,
      hazardRatio: Math.exp(beta),
      stdError,
      z,
      pValue,
      ciLower: Math.exp(beta - Z_95 * stdError),
      ciUpper: Math.exp(beta + Z_95 * stdError),
    };
  });
  return {
    coefficients,
    logPartialLikelihood: fit.logPartialLikelihood,
    iterations: fit.iterations,
    converged: fit.converged,
    tiedEventTimes: countTiedEventTimes(observations),
  };
}

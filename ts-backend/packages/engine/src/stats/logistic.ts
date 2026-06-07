// stats/logistic.ts — logistic regression via Newton-Raphson / IRLS (task 9.1).
// Transcribed exactly from crates/stats-code/src/logistic.rs::fit (the IRLS
// loop, max_iterations=50, tolerance=1e-8, probability clamp [1e-9, 1-1e-9],
// ridge-regularized Fisher inverse) and the math diagnostics helpers.

import { sigmoid } from '../math/special.js';
import { dot, invertMatrixWithRidge, matrixVectorMul, type Matrix } from '../math/linalg.js';
import { normalCdf } from '../math/distributions.js';

const MAX_ITERATIONS = 50;
const TOLERANCE = 1e-8;
const PROB_CLAMP_LO = 1e-9;
const PROB_CLAMP_HI = 1 - 1e-9;

function clampProb(p: number): number {
  return Math.min(PROB_CLAMP_HI, Math.max(PROB_CLAMP_LO, p));
}

export interface LogisticFit {
  beta: number[];
  iterations: number;
  converged: boolean;
}

/** Weighted Fisher information XᵀWX with W = w·p·(1-p). */
function fisherInformationWeighted(x: Matrix, probabilities: readonly number[], weights: readonly number[]): Matrix {
  const p = x[0]!.length;
  const info: Matrix = Array.from({ length: p }, () => new Array(p).fill(0));
  for (let i = 0; i < x.length; i += 1) {
    const prob = probabilities[i]!;
    const w = Math.max(weights[i]! * prob * (1 - prob), 1e-9);
    const row = x[i]!;
    for (let j = 0; j < p; j += 1) {
      for (let k = 0; k < p; k += 1) {
        info[j]![k]! += row[j]! * w * row[k]!;
      }
    }
  }
  return info;
}

/** Fit logistic regression; returns the converged beta and iteration metadata. */
export function fitLogistic(x: Matrix, y: readonly number[], weights?: readonly number[]): LogisticFit {
  const n = x.length;
  const p = n > 0 ? x[0]!.length : 0;
  if (n === 0 || p === 0) {
    throw new Error('Empty design matrix.');
  }
  const w = weights ?? new Array(n).fill(1);
  if (w.length !== n || w.some((v) => !Number.isFinite(v) || v <= 0)) {
    throw new Error('Logistic weights must be positive, finite, and aligned to rows.');
  }

  const beta = new Array<number>(p).fill(0);
  let converged = false;
  let iterations = 0;

  for (let iter = 0; iter < MAX_ITERATIONS; iter += 1) {
    const probabilities = x.map((row) => clampProb(sigmoid(dot(row, beta))));
    const fisher: Matrix = Array.from({ length: p }, () => new Array(p).fill(0));
    const gradient = new Array<number>(p).fill(0);

    for (let i = 0; i < n; i += 1) {
      const prob = probabilities[i]!;
      const obsWeight = w[i]!;
      const weight = Math.max(obsWeight * prob * (1 - prob), 1e-9);
      const residual = obsWeight * (y[i]! - prob);
      const row = x[i]!;
      for (let j = 0; j < p; j += 1) {
        gradient[j]! += row[j]! * residual;
        for (let k = 0; k < p; k += 1) {
          fisher[j]![k]! += row[j]! * weight * row[k]!;
        }
      }
    }

    const fisherInverse = invertMatrixWithRidge(fisher);
    const step = matrixVectorMul(fisherInverse, gradient);
    let maxStep = 0;
    for (let idx = 0; idx < p; idx += 1) {
      maxStep = Math.max(maxStep, Math.abs(step[idx]!));
      beta[idx]! += step[idx]!;
    }
    iterations = iter + 1;
    if (maxStep < TOLERANCE) {
      converged = true;
      break;
    }
  }

  return { beta, iterations, converged };
}

export interface LogisticCoefficient {
  index: number;
  beta: number;
  stdError: number;
  z: number;
  pValue: number;
  oddsRatio: number;
  ciLower: number;
  ciUpper: number;
}

export interface LogisticResult {
  coefficients: LogisticCoefficient[];
  logLikelihood: number;
  iterations: number;
  converged: boolean;
}

const Z_95 = 1.959963984540054;

/** Full logistic regression: fit, standard errors, Wald tests, odds ratios. */
export function logisticRegression(x: Matrix, y: readonly number[], weights?: readonly number[]): LogisticResult {
  const n = x.length;
  const p = x[0]!.length;
  const w = weights ?? new Array(n).fill(1);
  const fit = fitLogistic(x, y, w);

  const fittedProbabilities = x.map((row) => clampProb(sigmoid(dot(row, fit.beta))));
  const fisher = fisherInformationWeighted(x, fittedProbabilities, w);
  const covariance = invertMatrixWithRidge(fisher);
  const coefficients: LogisticCoefficient[] = [];
  for (let j = 0; j < p; j += 1) {
    const stdError = Math.sqrt(Math.max(covariance[j]![j]!, 0));
    const z = stdError > 0 ? fit.beta[j]! / stdError : Number.POSITIVE_INFINITY;
    const pValue = 2 * (1 - normalCdf(Math.abs(z)));
    coefficients.push({
      index: j,
      beta: fit.beta[j]!,
      stdError,
      z,
      pValue,
      oddsRatio: Math.exp(fit.beta[j]!),
      ciLower: Math.exp(fit.beta[j]! - Z_95 * stdError),
      ciUpper: Math.exp(fit.beta[j]! + Z_95 * stdError),
    });
  }

  let logLikelihood = 0;
  for (let i = 0; i < n; i += 1) {
    const prob = fittedProbabilities[i]!;
    logLikelihood += w[i]! * (y[i]! * Math.log(prob) + (1 - y[i]!) * Math.log(1 - prob));
  }

  return { coefficients, logLikelihood, iterations: fit.iterations, converged: fit.converged };
}

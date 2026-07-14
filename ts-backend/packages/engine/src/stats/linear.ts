// stats/linear.ts — ordinary least squares regression (Phase 3, task 7.1).
// Transcribed from crates/stats-code/src/linear.rs (normal-equations OLS).
//
// Estimates β = (XᵀX)⁻¹Xᵀy; SE from residual mean square and diag((XᵀX)⁻¹);
// per-term t tests; model R², adjusted R², and the overall F statistic.
// The CSV/encoding pipeline lives in the handler layer; this is the math core.

import { invertMatrix, transpose, matrixMultiply, matrixVectorMul } from '../math/linalg.js';
import { tDistributionPValue, fDistributionPValue } from '../math/distributions.js';

export interface LinearCoefficient {
  index: number;
  estimate: number;
  stdError: number;
  tStatistic: number;
  pValue: number;
  ciLower: number;
  ciUpper: number;
  degenerate: boolean;
}

export interface LinearResult {
  n: number;
  p: number;
  coefficients: LinearCoefficient[];
  rSquared: number;
  adjRSquared: number;
  residualStdError: number;
  fStatistic: number;
  fPValue: number;
  dfModel: number;
  dfResidual: number;
  degenerate: boolean;
}

/**
 * Solve OLS for design matrix X (n×p, including the intercept column) and
 * response y (length n). Returns coefficients with SE/t/p and model fit.
 */
export function ols(x: readonly (readonly number[])[], y: readonly number[], alpha = 0.05): LinearResult {
  const n = x.length;
  if (n === 0) throw new Error('OLS requires at least one observation.');
  const p = x[0]!.length;
  if (n <= p) throw new Error('OLS requires more observations than parameters.');
  if (y.length !== n) throw new Error('OLS: X and y length mismatch.');

  const xm = x.map((row) => row.slice());
  const xt = transpose(xm);
  const xtx = matrixMultiply(xt, xm);
  const xtxInv = invertMatrix(xtx);
  const xty = matrixVectorMul(xt, y);
  const beta = matrixVectorMul(xtxInv, xty);

  // Residuals and sums of squares.
  const yhat = matrixVectorMul(xm, beta);
  const meanY = y.reduce((s, v) => s + v, 0) / n;
  let ssRes = 0;
  let ssTot = 0;
  for (let i = 0; i < n; i += 1) {
    ssRes += (y[i]! - yhat[i]!) ** 2;
    ssTot += (y[i]! - meanY) ** 2;
  }
  const dfResidual = n - p;
  const dfModel = p - 1;
  const sigma2 = ssRes / dfResidual;
  const residualStdError = Math.sqrt(sigma2);

  const coefficients: LinearCoefficient[] = [];
  for (let j = 0; j < p; j += 1) {
    const variance = sigma2 * xtxInv[j]![j]!;
    const stdError = Math.sqrt(Math.max(variance, 0));
    if (!Number.isFinite(stdError)) {
      throw new Error(`OLS produced a non-finite standard error for coefficient ${j}.`);
    }
    const estimate = beta[j]!;
    const degenerate = stdError === 0;
    const tStatistic = degenerate
      ? estimate === 0
        ? 0
        : Math.sign(estimate) * Number.POSITIVE_INFINITY
      : estimate / stdError;
    const pValue = tDistributionPValue(tStatistic, dfResidual);
    // 95%-style CI uses the t critical value at the requested alpha.
    const tCrit = criticalT(alpha, dfResidual);
    coefficients.push({
      index: j,
      estimate: beta[j]!,
      stdError,
      tStatistic,
      pValue,
      ciLower: beta[j]! - tCrit * stdError,
      ciUpper: beta[j]! + tCrit * stdError,
      degenerate,
    });
  }

  const rSquared = ssTot > 0 ? 1 - ssRes / ssTot : 0;
  const adjRSquared = 1 - (1 - rSquared) * ((n - 1) / dfResidual);
  const msModel = (ssTot - ssRes) / dfModel;
  const msRes = sigma2;
  const fStatistic = msRes > 0
    ? msModel / msRes
    : msModel > 0
      ? Number.POSITIVE_INFINITY
      : 0;
  const fPValue = fDistributionPValue(fStatistic, dfModel, dfResidual);
  const degenerate = coefficients.some((coefficient) => coefficient.degenerate) || !Number.isFinite(fStatistic);

  return {
    n,
    p,
    coefficients,
    rSquared,
    adjRSquared,
    residualStdError,
    fStatistic,
    fPValue,
    dfModel,
    dfResidual,
    degenerate,
  };
}

/** Two-sided t critical value via bisection on the t p-value. */
function criticalT(alpha: number, df: number): number {
  if (alpha <= 0 || alpha >= 1 || df <= 0) return Number.NaN;
  let lo = 0;
  let hi = 50;
  while (tDistributionPValue(hi, df) > alpha) {
    hi *= 2;
    if (hi > 1e5) return hi;
  }
  for (let i = 0; i < 120; i += 1) {
    const mid = (lo + hi) / 2;
    if (tDistributionPValue(mid, df) < alpha) hi = mid;
    else lo = mid;
    if (hi - lo < 1e-12) break;
  }
  return (lo + hi) / 2;
}

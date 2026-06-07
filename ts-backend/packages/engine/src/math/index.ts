// math/ — pure linear algebra + special functions (Phase 2, task 5.1).
// No native addons: keeps the SEA single-file build valid and deterministic.

export const MATH_MODULE_READY = true as const;

export {
  sigmoid,
  logGamma,
  lnChoose,
  regularizedLowerGamma,
  regularizedUpperGammaCf,
  regularizedIncompleteBeta,
  regularizedBetaIncomplete,
  clamp01,
} from './special.js';

export {
  normalCdf,
  chiSquareCdf,
  chiSquareP,
  fDistributionPValue,
  tDistributionPValue,
  tDistributionCriticalValue,
  inverseNormal,
  studentTTwoSided,
} from './distributions.js';

export {
  type Matrix,
  dot,
  matrixVectorMul,
  matrixMultiply,
  transpose,
  invertMatrix,
  invertMatrixWithRidge,
  matrixDeterminant,
  matrixTrace,
  cholesky,
  qr,
  jacobiEigh,
} from './linalg.js';

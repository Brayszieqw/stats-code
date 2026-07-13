// stats/model-diagnostics.ts — model-design diagnostics shared by regression families.

import { invertMatrix, jacobiEigh, matrixMultiply, transpose, type Matrix } from '../math/linalg.js';

export interface PredictorDesignDiagnostic {
  observationCount: number;
  predictorCount: number;
  rank: number;
  rankDeficient: boolean;
  constantTerms: string[];
  conditionIndex: number;
  vif: Record<string, number | null>;
  maxVif: number | null;
}

/**
 * Diagnose the predictor-only design (do not include an intercept column).
 * Predictors are centered and scaled before rank, condition-index, and VIF
 * calculations so the diagnostics are not driven by measurement units.
 */
export function diagnosePredictorDesign(
  x: readonly (readonly number[])[],
  terms?: readonly string[],
): PredictorDesignDiagnostic {
  if (x.length === 0) throw new Error('Predictor diagnostics require at least one observation.');
  const predictorCount = x[0]!.length;
  if (x.some((row) => row.length !== predictorCount || row.some((value) => !Number.isFinite(value)))) {
    throw new Error('Predictor diagnostics require a rectangular, finite design matrix.');
  }
  if (terms !== undefined && terms.length !== predictorCount) {
    throw new Error('Predictor diagnostic term names must align with design columns.');
  }
  const names = terms?.slice() ?? Array.from({ length: predictorCount }, (_, index) => `x${index + 1}`);
  if (predictorCount === 0) {
    return {
      observationCount: x.length,
      predictorCount: 0,
      rank: 0,
      rankDeficient: false,
      constantTerms: [],
      conditionIndex: 1,
      vif: {},
      maxVif: null,
    };
  }

  const standardized: Matrix = Array.from({ length: x.length }, () => new Array(predictorCount).fill(0));
  const constantTerms: string[] = [];
  for (let column = 0; column < predictorCount; column += 1) {
    const values = x.map((row) => row[column]!);
    const mean = values.reduce((sum, value) => sum + value, 0) / values.length;
    const centered = values.map((value) => value - mean);
    const sumSquares = centered.reduce((sum, value) => sum + value * value, 0);
    const scale = values.reduce((max, value) => Math.max(max, Math.abs(value)), 1);
    const constantTolerance = Number.EPSILON * values.length * scale * scale * 100;
    if (sumSquares <= constantTolerance) {
      constantTerms.push(names[column]!);
      continue;
    }
    const norm = Math.sqrt(sumSquares);
    for (let row = 0; row < x.length; row += 1) standardized[row]![column] = centered[row]! / norm;
  }

  const correlation = matrixMultiply(transpose(standardized), standardized);
  const eigenvalues = jacobiEigh(correlation).eigenvalues;
  const maxEigenvalue = Math.max(eigenvalues[0] ?? 0, 0);
  const rankTolerance = Math.max(x.length, predictorCount) * Number.EPSILON * Math.max(maxEigenvalue, 1) * 100;
  const rank = eigenvalues.filter((value) => value > rankTolerance).length;
  const rankDeficient = rank < predictorCount;
  const minEigenvalue = rankDeficient ? 0 : Math.max(eigenvalues[eigenvalues.length - 1] ?? 0, 0);
  const conditionIndex = minEigenvalue > 0 ? Math.sqrt(maxEigenvalue / minEigenvalue) : Number.POSITIVE_INFINITY;

  const vif: Record<string, number | null> = Object.fromEntries(names.map((name) => [name, null]));
  let maxVif: number | null = null;
  if (!rankDeficient) {
    const inverse = invertMatrix(correlation);
    names.forEach((name, index) => {
      const value = Math.max(inverse[index]![index]!, 1);
      vif[name] = value;
      maxVif = maxVif === null ? value : Math.max(maxVif, value);
    });
  }

  return {
    observationCount: x.length,
    predictorCount,
    rank,
    rankDeficient,
    constantTerms,
    conditionIndex,
    vif,
    maxVif,
  };
}

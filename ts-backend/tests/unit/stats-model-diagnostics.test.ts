import { describe, expect, it } from 'vitest';
import { stats } from '@stats-code/engine';

describe('shared model-design diagnostics', () => {
  it('reports rank, condition index, and VIF on a full-rank design', () => {
    const diagnostic = stats.modelDiagnostics.diagnosePredictorDesign(
      [[1, 2], [2, 1], [3, 4], [4, 2], [5, 5], [6, 3]],
      ['age', 'bmi'],
    );
    expect(diagnostic.rank).toBe(2);
    expect(diagnostic.rankDeficient).toBe(false);
    expect(diagnostic.conditionIndex).toBeGreaterThanOrEqual(1);
    expect(diagnostic.vif.age).toBeGreaterThanOrEqual(1);
    expect(diagnostic.vif.bmi).toBeGreaterThanOrEqual(1);
  });

  it('identifies exact collinearity and constant predictors', () => {
    const duplicate = stats.modelDiagnostics.diagnosePredictorDesign(
      [[1, 2], [2, 4], [3, 6], [4, 8]],
      ['x', 'twice_x'],
    );
    expect(duplicate).toMatchObject({ rank: 1, predictorCount: 2, rankDeficient: true });
    expect(duplicate.conditionIndex).toBe(Number.POSITIVE_INFINITY);
    expect(duplicate.maxVif).toBeNull();

    const constant = stats.modelDiagnostics.diagnosePredictorDesign(
      [[1, 7], [2, 7], [3, 7], [4, 7]],
      ['x', 'constant'],
    );
    expect(constant.rankDeficient).toBe(true);
    expect(constant.constantTerms).toEqual(['constant']);
  });

  it('flags a near-collinear design numerically without declaring rank loss', () => {
    const diagnostic = stats.modelDiagnostics.diagnosePredictorDesign(
      Array.from({ length: 40 }, (_, index) => {
        const x = index + 1;
        return [x, x + (index % 2 === 0 ? 0.01 : -0.01)];
      }),
      ['x', 'almost_x'],
    );
    expect(diagnostic.rankDeficient).toBe(false);
    expect(diagnostic.conditionIndex).toBeGreaterThan(30);
    expect(diagnostic.maxVif).toBeGreaterThan(10);
  });
});

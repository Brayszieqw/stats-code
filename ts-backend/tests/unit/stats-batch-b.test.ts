import { describe, it, expect } from 'vitest';
import { stats } from '@stats-code/engine';

const { linear, epi, survival, diagnostic, standardization } = stats;
const close = (a: number, b: number, tol: number) => Math.abs(a - b) < tol;

describe('linear regression (OLS)', () => {
  it('recovers exact slope/intercept for a perfectly linear relation', () => {
    // y = 2x + 1; design rows [1, x].
    const x = [
      [1, 1],
      [1, 2],
      [1, 3],
      [1, 4],
      [1, 5],
    ];
    const y = [3, 5, 7, 9, 11];
    const r = linear.ols(x, y);
    expect(close(r.coefficients[0]!.estimate, 1, 1e-9)).toBe(true); // intercept
    expect(close(r.coefficients[1]!.estimate, 2, 1e-9)).toBe(true); // slope
    expect(close(r.rSquared, 1, 1e-9)).toBe(true);
  });

  it('matches a known simple-regression fit (R lm)', () => {
    // x=[1,2,3,4,5], y=[2,4,5,4,5]; R lm: intercept 2.2, slope 0.6, R^2=0.6
    const x = [
      [1, 1],
      [1, 2],
      [1, 3],
      [1, 4],
      [1, 5],
    ];
    const y = [2, 4, 5, 4, 5];
    const r = linear.ols(x, y);
    expect(close(r.coefficients[0]!.estimate, 2.2, 1e-9)).toBe(true);
    expect(close(r.coefficients[1]!.estimate, 0.6, 1e-9)).toBe(true);
    expect(close(r.rSquared, 0.6, 1e-9)).toBe(true);
  });

  it('marks zero-residual OLS statistics and preserves their p-value direction', () => {
    const r = linear.ols([
      [1, -1],
      [1, 0],
      [1, 1],
    ], [-1, 0, 1]);
    expect(r.coefficients[0]).toMatchObject({ estimate: 0, tStatistic: 0, pValue: 1, degenerate: true });
    expect(r.coefficients[1]).toMatchObject({ estimate: 1, pValue: 0, degenerate: true });
    expect(r.fStatistic).toBe(Number.POSITIVE_INFINITY);
    expect(r.fPValue).toBe(0);
    expect(r.degenerate).toBe(true);
  });
});

describe('OR / RR', () => {
  it('computes OR and RR for a 2x2 table', () => {
    // a=20,b=10,c=10,d=20 → OR = (20*20)/(10*10) = 4
    const r = epi.orRr(20, 10, 10, 20);
    expect(close(r.oddsRatio, 4, 1e-12)).toBe(true);
    // risk_exposed = 20/30, risk_unexposed = 10/30 → RR = 2
    expect(close(r.riskRatio, 2, 1e-12)).toBe(true);
    expect(r.orCiLower).toBeLessThan(r.oddsRatio);
    expect(r.orCiUpper).toBeGreaterThan(r.oddsRatio);
  });

  it('uses a visible 0.5 correction for zero cells and never returns NaN/Infinity', () => {
    const r = epi.orRr(0, 10, 5, 10);
    expect(r.continuityCorrected).toBe(true);
    expect(Object.values(r).filter((value): value is number => typeof value === 'number').every(Number.isFinite)).toBe(true);
  });

  it('rejects an empty 2x2 margin', () => {
    expect(() => epi.orRr(0, 0, 5, 10)).toThrow(/non-empty exposed and unexposed groups/);
  });
});

describe('attributable risk', () => {
  it('computes risk difference and attributable fraction', () => {
    const r = epi.attributableRisk(20, 10, 10, 20);
    expect(close(r.riskExposed, 20 / 30, 1e-12)).toBe(true);
    expect(close(r.riskUnexposed, 10 / 30, 1e-12)).toBe(true);
    expect(close(r.riskDifference, 10 / 30, 1e-12)).toBe(true);
    // RR=2 → AR% = (2-1)/2 = 50%
    expect(close(r.attributableRiskPercent, 50, 1e-9)).toBe(true);
  });

  it('keeps attributable-risk outputs finite when a cell is zero', () => {
    const r = epi.attributableRisk(0, 10, 5, 10);
    expect(r.continuityCorrected).toBe(true);
    expect(Object.values(r).filter((value): value is number => typeof value === 'number').every(Number.isFinite)).toBe(true);
  });
});

describe('Kaplan-Meier', () => {
  it('drops survival at each event time and holds steady at censoring', () => {
    // times: 1(event),2(event),3(censored),4(event); n=4
    const r = survival.kaplanMeier([1, 2, 3, 4], [true, true, false, true]);
    // S(1) = 1 - 1/4 = 0.75
    expect(close(r.points[0]!.survival, 0.75, 1e-12)).toBe(true);
    // S(2) = 0.75 * (1 - 1/3) = 0.5
    expect(close(r.points[1]!.survival, 0.5, 1e-12)).toBe(true);
    // median survival is the first time S ≤ 0.5
    expect(r.medianSurvival).toBe(2);
  });

  it('keeps the terminal all-event standard error finite', () => {
    const r = survival.kaplanMeier([1], [true]);
    expect(r.points[0]).toMatchObject({ survival: 0, stdError: 0 });
    expect(Number.isFinite(r.points[0]!.stdError)).toBe(true);
  });
});

describe('log-rank', () => {
  it('matches the lifelines two-group gold value', () => {
    const result = survival.logRankTest(
      [1, 2, 3, 4, 5, 2, 3, 4, 5, 6],
      [true, true, false, true, false, false, true, false, false, true],
      ['A', 'A', 'A', 'A', 'A', 'B', 'B', 'B', 'B', 'B'],
    );
    expect(result.status).toBe('computed');
    expect(result.statistic).toBeCloseTo(1.5333798671220824, 12);
    expect(result.pValue).toBeCloseTo(0.21560589487391288, 9);
    expect(result.degreesOfFreedom).toBe(1);
    expect(result.groups).toEqual(expect.arrayContaining([
      expect.objectContaining({ group: 'A', n: 5, events: 3, censored: 2 }),
      expect.objectContaining({ group: 'B', n: 5, events: 2, censored: 3 }),
    ]));
  });

  it('uses the full covariance for a three-group comparison', () => {
    const result = survival.logRankTest(
      [1, 2, 3, 4, 5, 2, 3, 4, 5, 6, 1, 3, 4, 6, 7],
      [true, true, false, true, false, false, true, false, false, true, true, false, true, false, false],
      ['A', 'A', 'A', 'A', 'A', 'B', 'B', 'B', 'B', 'B', 'C', 'C', 'C', 'C', 'C'],
    );
    expect(result.status).toBe('computed');
    expect(result.statistic).toBeCloseTo(1.3869230534821408, 12);
    expect(result.pValue).toBeCloseTo(0.4998428516102297, 9);
    expect(result.degreesOfFreedom).toBe(2);
  });

  it('reports all-censored data instead of fabricating a p-value', () => {
    const result = survival.logRankTest([1, 2, 3, 4], [false, false, false, false], ['A', 'A', 'B', 'B']);
    expect(result).toMatchObject({ status: 'not_computed', reason: 'no_events', pValue: null });
  });
});

describe('life table', () => {
  it('computes conditional and cumulative survival over intervals', () => {
    // events at t=0.5,1.5; censored at 2.5; breaks [0,1,2,3]
    const intervals = survival.lifeTable(
      [0.5, 1.5, 2.5, 0.7],
      [true, true, false, true],
      [0, 1, 2, 3],
    );
    expect(intervals).toHaveLength(3);
    // interval [0,1): 2 events, 0 censored, at risk 4 → q = 2/4 = 0.5
    expect(close(intervals[0]!.conditionalProbDeath, 0.5, 1e-12)).toBe(true);
    expect(close(intervals[0]!.cumulativeSurvival, 0.5, 1e-12)).toBe(true);
  });
});

describe('diagnostic ROC / AUC', () => {
  it('perfect separation → AUC = 1', () => {
    const scores = [0.1, 0.2, 0.8, 0.9];
    const labels = [false, false, true, true];
    expect(close(diagnostic.rocCurve(scores, labels).auc, 1, 1e-12)).toBe(true);
    expect(close(diagnostic.aucFromRanks(scores, labels), 1, 1e-12)).toBe(true);
  });

  it('reversed scores → AUC = 0', () => {
    const scores = [0.9, 0.8, 0.2, 0.1];
    const labels = [false, false, true, true];
    expect(close(diagnostic.aucFromRanks(scores, labels), 0, 1e-12)).toBe(true);
  });

  it('trapezoidal AUC agrees with the rank-based AUC on ties', () => {
    const scores = [0.5, 0.5, 0.5, 0.9, 0.1];
    const labels = [true, false, true, true, false];
    const a1 = diagnostic.rocCurve(scores, labels).auc;
    const a2 = diagnostic.aucFromRanks(scores, labels);
    expect(close(a1, a2, 1e-9)).toBe(true);
  });
});

describe('standardization', () => {
  it('direct standardization applies study rates to the standard population', () => {
    // 2 strata. study events [10, 20], pop [100, 100]; standard pop [50, 150].
    // stratum rates 0.1, 0.2 → expected = 0.1*50 + 0.2*150 = 35 over 200 = 0.175
    const r = standardization.directStandardization([10, 20], [100, 100], [50, 150]);
    expect(close(r.crudeRate, 30 / 200, 1e-12)).toBe(true);
    expect(close(r.standardizedRate, 0.175, 1e-12)).toBe(true);
  });

  it('indirect standardization computes SMR', () => {
    // observed 30; study pop [100,100]; standard rates [0.1, 0.1] → expected 20
    const r = standardization.indirectStandardization(30, [100, 100], [0.1, 0.1]);
    expect(close(r.expected, 20, 1e-12)).toBe(true);
    expect(close(r.smr, 1.5, 1e-12)).toBe(true);
  });
});

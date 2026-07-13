import { describe, it, expect } from 'vitest';
import { stats } from '@stats-code/engine';

const { ttest, anova, correlation, nonparametric, tableone } = stats;
const close = (a: number, b: number, tol: number) => Math.abs(a - b) < tol;

describe('paired t-test (known R outputs)', () => {
  it('before/after diffs [2,2,1,2,2]: mean_diff 1.8, t=9.0, df=4', () => {
    const before = [10, 12, 14, 16, 18];
    const after = [12, 14, 15, 18, 20];
    const r = ttest.pairedTtest(before, after, 0.05);
    expect(close(r.mean, 1.8, 1e-10)).toBe(true);
    expect(close(r.sd, 0.447213595, 1e-5)).toBe(true);
    expect(close(r.se, 0.2, 1e-10)).toBe(true);
    expect(close(r.tStatistic, 9.0, 1e-10)).toBe(true);
    expect(r.df).toBe(4);
    expect(r.pValue).toBeLessThan(0.01);
  });

  it('R fixture: before/after → t=5.9367, df=5, p=0.001933', () => {
    const before = [100, 105, 110, 115, 120, 125];
    const after = [102, 108, 112, 118, 122, 130];
    const r = ttest.pairedTtest(before, after, 0.05);
    expect(close(r.mean, 2.833333, 1e-3)).toBe(true);
    expect(close(r.tStatistic, 5.9367, 1e-2)).toBe(true);
    expect(close(r.pValue, 0.001933, 1e-3)).toBe(true);
    expect(close(r.ciLower, 1.6065, 1e-2)).toBe(true);
    expect(close(r.ciUpper, 4.0602, 1e-2)).toBe(true);
  });
});

describe('one-sample t-test (known R outputs)', () => {
  it('[10,12,14,16,18], mu=0 → mean 14, t≈9.899, df=4', () => {
    const r = ttest.oneSampleTtest([10, 12, 14, 16, 18], 0, 0.05);
    expect(close(r.mean, 14, 1e-10)).toBe(true);
    expect(close(r.sd, 3.16227766, 1e-5)).toBe(true);
    expect(close(r.tStatistic, 9.899495, 1e-2)).toBe(true);
    expect(r.df).toBe(4);
  });

  it('mu equal to mean → t=0, p≈1', () => {
    const r = ttest.oneSampleTtest([10, 12, 14, 16, 18], 14, 0.05);
    expect(close(r.tStatistic, 0, 1e-10)).toBe(true);
    expect(close(r.pValue, 1.0, 1e-8)).toBe(true);
  });

  it('rejects fewer than 2 observations and zero variance', () => {
    expect(() => ttest.oneSampleTtest([1], 0)).toThrow();
    expect(() => ttest.oneSampleTtest([5, 5, 5], 0)).toThrow();
  });
});

describe("Welch's t-test (known R output)", () => {
  it('a=[1..5], b=[3..7] → t=-2.0, df≈8', () => {
    const r = ttest.welchTtest([1, 2, 3, 4, 5], [3, 4, 5, 6, 7], 0.05);
    expect(close(r.tStatistic, -2.0, 1e-2)).toBe(true);
    expect(close(r.df, 8.0, 0.5)).toBe(true);
    expect(close(r.pValue, 0.0806, 2e-2)).toBe(true);
  });
});

describe('one-way ANOVA', () => {
  it('three clearly-separated groups yield a small p-value', () => {
    const r = anova.oneWayAnova([
      [1, 2, 3],
      [4, 5, 6],
      [7, 8, 9],
    ]);
    expect(r.k).toBe(3);
    expect(r.dfBetween).toBe(2);
    expect(r.dfWithin).toBe(6);
    // SS_between for these groups = 3*((2-5)^2+(5-5)^2+(8-5)^2) = 54; SS_within=2 per group *3=6
    expect(close(r.ssBetween, 54, 1e-9)).toBe(true);
    expect(close(r.ssWithin, 6, 1e-9)).toBe(true);
    expect(close(r.fStatistic, 27, 1e-9)).toBe(true);
    expect(r.pValue).toBeLessThan(0.01);
  });

  it('identical groups → F≈0, p≈1', () => {
    const r = anova.oneWayAnova([
      [5, 6, 7],
      [5, 6, 7],
      [5, 6, 7],
    ]);
    expect(close(r.ssBetween, 0, 1e-9)).toBe(true);
    expect(close(r.fStatistic, 0, 1e-9)).toBe(true);
    expect(close(r.pValue, 1.0, 1e-9)).toBe(true);
  });
});

describe('correlation', () => {
  it('perfect positive Pearson r=1', () => {
    const r = correlation.pearsonCorrelation([1, 2, 3, 4, 5], [2, 4, 6, 8, 10]);
    expect(close(r.r, 1.0, 1e-12)).toBe(true);
  });

  it('perfect negative Pearson r=-1', () => {
    const r = correlation.pearsonCorrelation([1, 2, 3, 4, 5], [10, 8, 6, 4, 2]);
    expect(close(r.r, -1.0, 1e-12)).toBe(true);
  });

  it('Spearman handles a monotone non-linear relation as r=1', () => {
    const r = correlation.spearmanCorrelation([1, 2, 3, 4, 5], [1, 4, 9, 16, 25]);
    expect(close(r.r, 1.0, 1e-12)).toBe(true);
  });

  it('a known moderate Pearson correlation has a sensible t and p', () => {
    const r = correlation.pearsonCorrelation([1, 2, 3, 4, 5, 6], [2, 1, 4, 3, 6, 5]);
    expect(r.r).toBeGreaterThan(0);
    expect(r.r).toBeLessThan(1);
    expect(r.pValue).toBeGreaterThan(0);
    expect(r.pValue).toBeLessThan(1);
  });
});

describe('nonparametric', () => {
  it('Mann-Whitney separates two disjoint groups', () => {
    const r = nonparametric.mannWhitneyU([1, 2, 3], [10, 11, 12]);
    expect(r.u).toBe(0);
    expect(r.pValue).toBeLessThan(0.1);
  });

  it('Kruskal-Wallis on three separated groups yields small p', () => {
    const r = nonparametric.kruskalWallis([
      [1, 2, 3],
      [4, 5, 6],
      [7, 8, 9],
    ]);
    expect(r.df).toBe(2);
    expect(r.pValue).toBeLessThan(0.05);
  });
});

describe('tableone descriptive summaries', () => {
  it('continuous summary computes mean/sd/median/IQR and counts missing', () => {
    const s = tableone.summarizeContinuous('age', [10, 20, 30, 40, null]);
    expect(s.n).toBe(4);
    expect(s.missing).toBe(1);
    expect(close(s.mean, 25, 1e-12)).toBe(true);
    expect(close(s.median, 25, 1e-12)).toBe(true);
    expect(s.min).toBe(10);
    expect(s.max).toBe(40);
  });

  it('categorical summary counts levels and percentages', () => {
    const s = tableone.summarizeCategorical('sex', ['M', 'F', 'M', 'M', null, '']);
    expect(s.n).toBe(4);
    expect(s.missing).toBe(2);
    const m = s.levels.find((l) => l.level === 'M')!;
    expect(m.count).toBe(3);
    expect(close(m.percent, 75, 1e-9)).toBe(true);
  });

  it('quantileSorted interpolates', () => {
    expect(tableone.quantileSorted([1, 2, 3, 4], 0.5)).toBeCloseTo(2.5, 12);
  });

  it('computes absolute continuous and categorical standardized differences', () => {
    const first = tableone.summarizeContinuous('age', [10, 20]);
    const second = tableone.summarizeContinuous('age', [30, 40]);
    expect(tableone.standardizedMeanDifference(first, second)).toBeCloseTo(Math.sqrt(8), 12);

    expect(tableone.standardizedProportionDifference(1, 1, 1, 2)).toBeCloseTo(Math.sqrt(2), 12);
    expect(tableone.standardizedProportionDifference(0, 0, 1, 2)).toBeNull();
  });
});

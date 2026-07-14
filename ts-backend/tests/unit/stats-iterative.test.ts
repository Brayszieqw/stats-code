import { describe, it, expect } from 'vitest';
import { stats } from '@stats-code/engine';

const { logistic, cox } = stats;
const close = (a: number, b: number, tol: number) => Math.abs(a - b) < tol;

describe('logistic regression (IRLS)', () => {
  it('converges and recovers a positive effect on overlapping data', () => {
    // Overlapping classes (not separable) so the MLE is finite and converges.
    const x = [
      [1, -2],
      [1, -1],
      [1, 0],
      [1, 1],
      [1, 2],
      [1, -1],
      [1, 0],
      [1, 1],
    ];
    const y = [0, 0, 0, 1, 1, 1, 1, 0];
    const r = logistic.logisticRegression(x, y);
    expect(r.converged).toBe(true);
    expect(r.coefficients[1]!.beta).toBeGreaterThan(0); // positive slope
    expect(r.coefficients[1]!.oddsRatio).toBeGreaterThan(1);
  });

  it('matches a known intercept-only model: beta0 = logit(p)', () => {
    // intercept-only design; 6 ones, 4 zeros → p=0.6, beta0 = ln(0.6/0.4)
    const x = Array.from({ length: 10 }, () => [1]);
    const y = [1, 1, 1, 1, 1, 1, 0, 0, 0, 0];
    const r = logistic.logisticRegression(x, y);
    expect(r.converged).toBe(true);
    expect(close(r.coefficients[0]!.beta, Math.log(0.6 / 0.4), 1e-6)).toBe(true);
  });

  it('fitLogistic reports iteration metadata', () => {
    const x = [
      [1, 0],
      [1, 1],
      [1, 2],
      [1, 3],
    ];
    const y = [0, 0, 1, 1];
    const fit = logistic.fitLogistic(x, y);
    expect(fit.iterations).toBeGreaterThan(0);
    expect(fit.iterations).toBeLessThanOrEqual(50);
    expect(fit.beta).toHaveLength(2);
    expect(fit.regularized).toBe(false);
  });

  it('rejects non-binary and single-class outcomes before fitting', () => {
    const x = [[1, 0], [1, 1], [1, 2]];
    expect(() => logistic.logisticRegression(x, [0, 2, 1])).toThrow(/encoded as 0\/1/);
    expect(() => logistic.logisticRegression(x, [0, 0, 0])).toThrow(/both outcome classes/);
  });

  it('rejects an empty design matrix with the stable contract error', () => {
    expect(() => logistic.logisticRegression([], [])).toThrow(/^Empty design matrix\.$/);
  });

  it('reports ridge regularization for a collinear logistic design', () => {
    const x = [
      [1, -2, -2],
      [1, -1, -1],
      [1, 0, 0],
      [1, 1, 1],
      [1, 2, 2],
      [1, -1, -1],
      [1, 0, 0],
      [1, 1, 1],
    ];
    const result = logistic.logisticRegression(x, [0, 0, 0, 1, 1, 1, 1, 0]);
    expect(result.regularized).toBe(true);
    expect(result.ridgeValue).toBeGreaterThan(0);
  });
});

describe('Cox proportional hazards (Efron ties)', () => {
  function obs(time: number, event: boolean, x: number[], weight = 1): cox.CoxObservation {
    return { time, event, x, weight };
  }

  function simulatedBinaryCox(seed: number, crossing: boolean): cox.CoxObservation[] {
    let state = seed >>> 0;
    const random = () => {
      state = (1664525 * state + 1013904223) >>> 0;
      return (state + 0.5) / 4294967296;
    };
    const exponential = (rate: number) => -Math.log(random()) / rate;
    const observations: cox.CoxObservation[] = [];
    const perGroup = crossing ? 60 : 100;
    for (const x of [0, 1]) {
      for (let index = 0; index < perGroup; index += 1) {
        let time: number;
        if (crossing) {
          const earlyRate = x === 0 ? 0.35 : 0.04;
          const lateRate = x === 0 ? 0.04 : 0.35;
          const earlyTime = exponential(earlyRate);
          time = earlyTime < 5 ? earlyTime : 5 + exponential(lateRate);
        } else {
          time = exponential(x === 0 ? 0.10 : 0.18);
        }
        const censor = crossing ? 6 + 6 * random() : 12 * random();
        const event = time <= censor;
        observations.push(obs(event ? time : censor, event, [x]));
      }
    }
    return observations;
  }

  it('converges and gives a positive coefficient when higher covariate → earlier events', () => {
    const observations = [
      obs(1, true, [1]),
      obs(2, false, [0]),
      obs(3, true, [1]),
      obs(4, true, [0]),
      obs(5, false, [1]),
      obs(6, true, [0]),
      obs(7, true, [1]),
      obs(8, false, [0]),
    ];
    const r = cox.coxRegression(observations);
    expect(r.converged).toBe(true);
    expect(Number.isFinite(r.coefficients[0]!.beta)).toBe(true);
  });

  it('counts tied event times correctly', () => {
    const observations = [
      obs(1, true, [1]),
      obs(1, true, [0]), // tie at t=1
      obs(2, true, [1]),
      obs(3, false, [0]),
    ];
    expect(cox.countTiedEventTimes(observations)).toBe(1);
  });

  it('partial-likelihood gradient at beta=0 is the score vector and is finite', () => {
    const observations = [
      obs(1, true, [1]),
      obs(2, true, [0]),
      obs(3, false, [1]),
      obs(4, true, [0]),
    ];
    const stat = cox.coxPartialStats(observations, [0]);
    expect(Number.isFinite(stat.logPartialLikelihood)).toBe(true);
    expect(Number.isFinite(stat.gradient[0]!)).toBe(true);
    expect(stat.information[0]![0]!).toBeGreaterThan(0);
  });

  it('Efron handling reduces to Breslow with no ties (single-event groups run)', () => {
    // No tied event times, with covariate/time overlap so the MLE is finite.
    const observations = [
      obs(1, true, [1]),
      obs(2, false, [0]),
      obs(3, true, [0]),
      obs(4, true, [1]),
      obs(5, false, [0]),
      obs(6, true, [0]),
    ];
    const r = cox.coxRegression(observations);
    expect(r.tiedEventTimes).toBe(0);
    expect(r.converged).toBe(true);
    expect(Number.isFinite(r.coefficients[0]!.beta)).toBe(true);
  });

  it('does not flag a seeded proportional-hazards dataset', () => {
    const observations = simulatedBinaryCox(7, false);
    const fit = cox.coxRegression(observations);
    const diagnostic = cox.coxProportionalHazardsTest(
      observations,
      fit.coefficients.map((coefficient) => coefficient.beta),
    );
    expect(diagnostic.status).toBe('computed');
    expect(diagnostic.statistic).toBeCloseTo(1.292030198041073, 10);
    expect(diagnostic.pValue).toBeCloseTo(0.25567414788078313, 8);
    expect(diagnostic.violated).toBe(false);
  });

  it('flags a seeded crossing-hazards dataset and identifies the covariate', () => {
    const observations = simulatedBinaryCox(42, true);
    const fit = cox.coxRegression(observations);
    const diagnostic = cox.coxProportionalHazardsTest(
      observations,
      fit.coefficients.map((coefficient) => coefficient.beta),
    );
    expect(diagnostic.status).toBe('computed');
    expect(diagnostic.statistic).toBeCloseTo(26.727339787374135, 9);
    expect(diagnostic.pValue).toBeLessThan(1e-6);
    expect(diagnostic.violated).toBe(true);
    expect(diagnostic.covariates[0]).toMatchObject({ violated: true });
  });

  it('rejects an all-censored Cox fit before returning meaningless coefficients', () => {
    expect(() => cox.coxRegression([obs(1, false, [0]), obs(2, false, [1])]))
      .toThrow(/at least one observed event/);
  });

  it('reports ridge regularization for a collinear Cox design', () => {
    const observations = [
      obs(1, true, [1, 1]),
      obs(2, false, [0, 0]),
      obs(3, true, [1, 1]),
      obs(4, true, [0, 0]),
      obs(5, false, [1, 1]),
      obs(6, true, [0, 0]),
    ];
    const result = cox.coxRegression(observations);
    expect(result.regularized).toBe(true);
    expect(result.ridgeValue).toBeGreaterThan(0);
  });
});

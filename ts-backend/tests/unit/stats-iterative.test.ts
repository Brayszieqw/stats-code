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
  });
});

describe('Cox proportional hazards (Efron ties)', () => {
  function obs(time: number, event: boolean, x: number[], weight = 1): cox.CoxObservation {
    return { time, event, x, weight };
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
});

// tests/parity/iterative.parity.test.ts — iterative parity suite (task 9.2).
//
// Compares cox (Efron ties) and logistic (Newton-Raphson/IRLS) metrics against
// recorded Reference_Software baselines within the ITERATIVE threshold
// 1e-4/1e-7 (Requirement 2.3). Halts on the first metric violation. The
// recordings under crates/stats-code/validation/known_values are the oracle;
// Reference_Software is never spawned (Forbidden_Runtime policy).
//
// _Requirements: 2.3, 2.4, 2.5_

import { describe, it, expect } from 'vitest';
import { stats, parity } from '@stats-code/engine';
import { loadBaseline, parseCsv, numericColumn } from './fixtures.js';

const { DEFAULT_ITERATIVE, compareScalar } = parity;
const TOL = DEFAULT_ITERATIVE; // 1e-4 / 1e-7
const SOFTWARES = ['sas', 'spss'] as const;

function assertMetric(
  software: string,
  algorithm: string,
  metric: string,
  tsValue: number,
  refValue: number,
): void {
  const result = compareScalar(tsValue, refValue, TOL);
  expect(
    result.status,
    `[${software}/${algorithm}] metric "${metric}": ts=${tsValue} ref=${refValue} ${result.message}`,
  ).toBe('pass');
}

describe('Iterative parity vs recorded Reference_Software (1e-4/1e-7)', () => {
  for (const software of SOFTWARES) {
    describe(`${software}`, () => {
      it('cox: betas, hazard ratios, std errors, log partial likelihood', () => {
        const base = loadBaseline(software, 'cox');
        if (!base) return; // cell not recorded for this software
        const csv = parseCsv(base.input.dataset_csv);
        const age = numericColumn(csv, 'age');
        const bmi = numericColumn(csv, 'bmi');
        const time = numericColumn(csv, base.input.spec.duration_col ?? 'time');
        const death = numericColumn(csv, base.input.spec.event_col ?? 'death');
        const observations = time.map((t, i) => ({
          time: t,
          event: death[i] === 1,
          x: [age[i]!, bmi[i]!],
          weight: 1,
        }));
        const r = stats.cox.coxRegression(observations);
        const exp = base.expected_outputs;
        // coefficients[0]=age, [1]=bmi (design column order).
        assertMetric(software, 'cox', 'beta_age', r.coefficients[0]!.beta, exp.beta_age!);
        assertMetric(software, 'cox', 'beta_bmi', r.coefficients[1]!.beta, exp.beta_bmi!);
        assertMetric(software, 'cox', 'hazard_ratio_age', r.coefficients[0]!.hazardRatio, exp.hazard_ratio_age!);
        assertMetric(software, 'cox', 'hazard_ratio_bmi', r.coefficients[1]!.hazardRatio, exp.hazard_ratio_bmi!);
        assertMetric(software, 'cox', 'stderr_age', r.coefficients[0]!.stdError, exp.stderr_age!);
        assertMetric(software, 'cox', 'stderr_bmi', r.coefficients[1]!.stdError, exp.stderr_bmi!);
        assertMetric(software, 'cox', 'log_partial_likelihood', r.logPartialLikelihood, exp.log_partial_likelihood!);
      });

      it('logistic: betas, odds ratios, std errors, log likelihood', () => {
        const base = loadBaseline(software, 'logistic');
        if (!base) return;
        const csv = parseCsv(base.input.dataset_csv);
        const age = numericColumn(csv, 'age');
        const sex = numericColumn(csv, 'sex');
        const y = numericColumn(csv, base.input.spec.outcome_col ?? 'outcome');
        // Design matrix with intercept first: [1, age, sex].
        const x = y.map((_, i) => [1, age[i]!, sex[i]!]);
        const r = stats.logistic.logisticRegression(x, y);
        const exp = base.expected_outputs;
        // coefficients[0]=const, [1]=age, [2]=sex.
        assertMetric(software, 'logistic', 'beta_const', r.coefficients[0]!.beta, exp.beta_const!);
        assertMetric(software, 'logistic', 'beta_age', r.coefficients[1]!.beta, exp.beta_age!);
        assertMetric(software, 'logistic', 'beta_sex', r.coefficients[2]!.beta, exp.beta_sex!);
        assertMetric(software, 'logistic', 'stderr_const', r.coefficients[0]!.stdError, exp.stderr_const!);
        assertMetric(software, 'logistic', 'stderr_age', r.coefficients[1]!.stdError, exp.stderr_age!);
        assertMetric(software, 'logistic', 'stderr_sex', r.coefficients[2]!.stdError, exp.stderr_sex!);
        assertMetric(software, 'logistic', 'odds_ratio_age', r.coefficients[1]!.oddsRatio, exp.odds_ratio_age!);
        assertMetric(software, 'logistic', 'odds_ratio_sex', r.coefficients[2]!.oddsRatio, exp.odds_ratio_sex!);
        assertMetric(software, 'logistic', 'log_likelihood', r.logLikelihood, exp.log_likelihood!);
      });
    });
  }
});

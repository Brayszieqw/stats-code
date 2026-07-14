// tests/parity/batch-a.parity.test.ts — Batch A parity suite (task 5.5).
//
// Compares each (algorithm, software, case, metric) for batch A
// (tableone, ttest, anova, nonparametric, correlation) against recorded
// Reference_Software baselines within the non-iterative threshold 1e-6/1e-9.
// The suite halts on the first metric violation (Requirement 2.4/2.5) and fails
// if a required baseline recording is unavailable (Requirement 2.6).
//
// Reference_Software is NOT spawned here (the Forbidden_Runtime policy blocks
// R/SAS/Python/SPSS and they are not installed on CI); the recorded baselines
// under crates/stats-code/validation/known_values are the authoritative oracle.
//
// _Requirements: 2.2, 2.4, 2.5, 2.6_

import { describe, it, expect } from 'vitest';
import { stats, parity } from '@stats-code/engine';
import { loadBaseline, parseCsv, groupedColumn, numericColumn } from './fixtures.js';

const { DEFAULT_NON_ITERATIVE, compareScalar } = parity;
const TOL = DEFAULT_NON_ITERATIVE; // 1e-6 / 1e-9

// Reference_Software with recorded batch A baselines. R/Python recordings are
// not present in the repo; SAS + SPSS are. A `live` cell requires a recording,
// so we assert availability per (software, algorithm) below.
const SOFTWARES = ['sas', 'spss'] as const;

/** Assert a single metric is within the non-iterative parity threshold. */
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

/** Pooled (Student) two-sample t-statistic, matching SAS PROC TTEST pooled. */
function pooledTwoSampleT(a: number[], b: number[]): number {
  const mean = (xs: number[]) => xs.reduce((s, v) => s + v, 0) / xs.length;
  const ma = mean(a);
  const mb = mean(b);
  const va = a.reduce((s, v) => s + (v - ma) ** 2, 0) / (a.length - 1);
  const vb = b.reduce((s, v) => s + (v - mb) ** 2, 0) / (b.length - 1);
  const sp2 = ((a.length - 1) * va + (b.length - 1) * vb) / (a.length + b.length - 2);
  const se = Math.sqrt(sp2 * (1 / a.length + 1 / b.length));
  return (ma - mb) / se;
}

describe('Batch A parity vs recorded Reference_Software (1e-6/1e-9)', () => {
  for (const software of SOFTWARES) {
    describe(`${software}`, () => {
      it('tableone: group means/sds/ns and pooled t-statistic', () => {
        const base = loadBaseline(software, 'tableone');
        // Requirement 2.6: a required baseline must exist to validate a `live` cell.
        expect(base, `missing required ${software}/tableone baseline`).not.toBeNull();
        if (!base) return;
        const csv = parseCsv(base.input.dataset_csv);
        const groups = groupedColumn(csv, base.input.spec.value_col!, base.input.spec.group_col!);
        const a = groups.get('A')!;
        const b = groups.get('B')!;
        const sa = stats.tableone.summarizeContinuous('value', a);
        const sb = stats.tableone.summarizeContinuous('value', b);
        const exp = base.expected_outputs;

        assertMetric(software, 'tableone', 'n_group_A', sa.n, exp.n_group_A!);
        assertMetric(software, 'tableone', 'n_group_B', sb.n, exp.n_group_B!);
        assertMetric(software, 'tableone', 'mean_group_A', sa.mean, exp.mean_group_A!);
        assertMetric(software, 'tableone', 'mean_group_B', sb.mean, exp.mean_group_B!);
        assertMetric(software, 'tableone', 'sd_group_A', sa.sd, exp.sd_group_A!);
        assertMetric(software, 'tableone', 'sd_group_B', sb.sd, exp.sd_group_B!);
        if (exp.ttest_statistic !== undefined) {
          assertMetric(software, 'tableone', 'ttest_statistic', pooledTwoSampleT(a, b), exp.ttest_statistic);
        }
      });

      it('ttest: Welch mean difference, statistic and p-value', () => {
        const base = loadBaseline(software, 'ttest');
        expect(base, `missing required ${software}/ttest baseline`).not.toBeNull();
        if (!base) return;
        const csv = parseCsv(base.input.dataset_csv);
        const groups = groupedColumn(csv, base.input.spec.value_col!, base.input.spec.group_col!);
        const result = stats.ttest.welchTtest(groups.get('A')!, groups.get('B')!);
        const exp = base.expected_outputs;

        assertMetric(software, 'ttest', 'mean_diff', result.meanDiff, exp.mean_diff!);
        assertMetric(software, 'ttest', 'ttest_welch_statistic', result.tStatistic, exp.ttest_welch_statistic!);
        assertMetric(software, 'ttest', 'ttest_welch_p_value', result.pValue, exp.ttest_welch_p_value!);
      });

      it('anova: ss_between/ss_within/df/f', () => {
        const base = loadBaseline(software, 'anova');
        expect(base, `missing required ${software}/anova baseline`).not.toBeNull();
        if (!base) return;
        const csv = parseCsv(base.input.dataset_csv);
        const groups = groupedColumn(csv, base.input.spec.value_col!, base.input.spec.group_col!);
        const r = stats.anova.oneWayAnova([...groups.values()]);
        const exp = base.expected_outputs;
        assertMetric(software, 'anova', 'df_between', r.dfBetween, exp.df_between!);
        assertMetric(software, 'anova', 'df_within', r.dfWithin, exp.df_within!);
        assertMetric(software, 'anova', 'ss_between', r.ssBetween, exp.ss_between!);
        assertMetric(software, 'anova', 'ss_within', r.ssWithin, exp.ss_within!);
        assertMetric(software, 'anova', 'f_statistic', r.fStatistic, exp.f_statistic!);
      });

      it('correlation: pearson_r and spearman_rho', () => {
        const base = loadBaseline(software, 'correlation');
        expect(base, `missing required ${software}/correlation baseline`).not.toBeNull();
        if (!base) return;
        const csv = parseCsv(base.input.dataset_csv);
        const x = numericColumn(csv, base.input.spec.x ?? 'x');
        const y = numericColumn(csv, base.input.spec.y ?? 'y');
        const exp = base.expected_outputs;
        if (exp.pearson_r !== undefined) {
          assertMetric(software, 'correlation', 'pearson_r', stats.correlation.pearsonR(x, y), exp.pearson_r);
        }
        if (exp.spearman_rho !== undefined) {
          const rho = stats.correlation.spearmanCorrelation(x, y).r;
          assertMetric(software, 'correlation', 'spearman_rho', rho, exp.spearman_rho);
        }
      });

      it('nonparametric: mann-whitney U and group sizes', () => {
        const base = loadBaseline(software, 'nonparametric');
        expect(base, `missing required ${software}/nonparametric baseline`).not.toBeNull();
        if (!base) return;
        const csv = parseCsv(base.input.dataset_csv);
        const groups = groupedColumn(csv, base.input.spec.value_col!, base.input.spec.group_col!);
        const a = groups.get('A')!;
        const b = groups.get('B')!;
        const r = stats.nonparametric.mannWhitneyU(a, b);
        const exp = base.expected_outputs;
        if (exp.n_group_A !== undefined) assertMetric(software, 'nonparametric', 'n_group_A', r.n1, exp.n_group_A);
        if (exp.n_group_B !== undefined) assertMetric(software, 'nonparametric', 'n_group_B', r.n2, exp.n_group_B);
        // U is reported as min(U1,U2); SAS may report the complementary statistic.
        if (exp.mannwhitney_u !== undefined) {
          const n1n2 = r.n1 * r.n2;
          const passesEither =
            compareScalar(r.u, exp.mannwhitney_u, TOL).status === 'pass' ||
            compareScalar(n1n2 - r.u, exp.mannwhitney_u, TOL).status === 'pass';
          expect(
            passesEither,
            `[${software}/nonparametric] mannwhitney_u: ts=${r.u} (or ${n1n2 - r.u}) ref=${exp.mannwhitney_u}`,
          ).toBe(true);
        }
      });
    });
  }
});

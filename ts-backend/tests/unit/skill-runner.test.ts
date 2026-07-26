// tests/unit/skill-runner.test.ts — in-process SkillRunner (task 5.9).
//
// In-process invocation of linear/logistic/cox/kaplan_meier against a fixture
// dataset returns a SkillResult with analysis metadata; missing arg and every
// hand-thrown input rejection (engine gates, column extraction) → invalid_args
// (422 at the route, D15); runtime defects → execution_failed (≤ 2048 chars).
//
// _Requirements: 5.2, 5.3, 5.5, 5.6_

import { describe, it, expect } from 'vitest';
import { createHash } from 'node:crypto';
import { stats } from '@stats-code/engine';
import {
  SkillRegistry,
  SkillRunner,
  SkillRunErrorException,
  type SkillContext,
  type DatasetSummary,
} from '@stats-code/server';

function ctxFor(csv: string, columns: DatasetSummary['columns']): SkillContext {
  const bytes = new TextEncoder().encode(csv);
  const summary: DatasetSummary = {
    dataset_id: 'ds-1',
    file_name: 'data.csv',
    size_bytes: bytes.byteLength,
    encoding: 'Utf8',
    row_count: csv.trim().split('\n').length - 1,
    columns,
    uploaded_at: '2026-01-01T00:00:00Z',
    sha256: createHash('sha256').update(bytes).digest('hex'),
  };
  return { datasetBytes: bytes, datasetSummary: summary };
}

const num = (name: string) => ({ name, inferred_type: 'Numeric' as const, missing_count: 0 });
const cat = (name: string) => ({ name, inferred_type: 'Categorical' as const, missing_count: 0 });

describe('SkillRunner in-process execution (Requirements 5.2, 5.3, 5.5, 5.6)', () => {
  const reg = SkillRegistry.withDefaults();
  const runner = new SkillRunner(reg);

  it('runs table one summaries and attaches analysis metadata', async () => {
    const ctx = ctxFor('age,bmi,arm,sex\n60,21,A,M\n64,,A,M\n70,25,B,F\n72,29,B,F\n', [num('age'), num('bmi'), cat('arm'), cat('sex')]);
    const result = await runner.run(reg.get('tableone')!, {
      group: 'arm',
      continuous: ['age', 'bmi'],
      categorical: ['arm', 'sex'],
      dataset_id: 'ds-1',
    }, ctx);
    const payload = result.payload as {
      groups: Array<{
        label: string;
        continuous: Array<{ variable: string; n: number; missing: number }>;
      }>;
      standardized_differences: {
        comparison: { first: string; second: string };
        continuous: Array<{ variable: string; smd: number | null }>;
      };
      categorical_tests: Array<{
        variable: string;
        status: string;
        method: string | null;
        observed_zero_cells: number;
      }>;
    };
    expect(payload.groups.map((group) => group.label)).toEqual(['A', 'B']);
    expect(payload.groups[0]!.continuous).toHaveLength(2);
    expect(payload.groups[0]!.continuous.find((summary) => summary.variable === 'bmi')).toMatchObject({ n: 1, missing: 1 });
    expect(payload.groups[1]!.continuous.find((summary) => summary.variable === 'bmi')).toMatchObject({ n: 2, missing: 0 });
    expect(payload.standardized_differences.comparison).toEqual({ first: 'A', second: 'B' });
    expect(payload.standardized_differences.continuous.find((entry) => entry.variable === 'age')?.smd)
      .toBeCloseTo(9 / Math.sqrt(5), 12);
    expect(payload.standardized_differences.continuous.find((entry) => entry.variable === 'bmi')?.smd)
      .toBeCloseTo(3, 12);
    expect(payload.categorical_tests.find((entry) => entry.variable === 'arm')).toMatchObject({
      status: 'not_applicable',
      method: null,
    });
    expect(payload.categorical_tests.find((entry) => entry.variable === 'sex')).toMatchObject({
      status: 'computed',
      method: 'fisher_exact',
      observed_zero_cells: 2,
    });
    expect(result.analysis?.algorithm_id).toBe('tableone');
    expect(result.analysis?.result_contract).toMatchObject({
      estimates: [],
      counts: { input_n: 4, complete_case_n: 3, missing_n: 1 },
      exclusions: [{ n: 1 }],
      analysis_availability: { unadjusted: 'not_applicable', adjusted: 'not_applicable' },
    });
  });

  it('runs Welch t-test and attaches analysis metadata', async () => {
    const ctx = ctxFor('score,arm\n10,A\n11,A\n12,A\n20,B\n21,B\n22,B\n', [num('score'), cat('arm')]);
    const result = await runner.run(reg.get('ttest')!, {
      group: 'arm',
      testVar: 'score',
      dataset_id: 'ds-1',
    }, ctx);
    const payload = result.payload as { p_value: number; groups: unknown[] };
    expect(payload.groups).toHaveLength(2);
    expect(payload.p_value).toBeGreaterThanOrEqual(0);
    expect(result.analysis?.algorithm_id).toBe('ttest');
  });

  it('runs one-way ANOVA', async () => {
    const ctx = ctxFor(
      'score,arm,unused\n10,A,\n11,A,\n12,A,\n20,B,\n21,B,\n22,B,\n30,C,\n31,C,\n32,C,\n',
      [num('score'), cat('arm'), num('unused')],
    );
    const result = await runner.run(reg.get('anova')!, {
      group: 'arm',
      testVar: 'score',
      dataset_id: 'ds-1',
    }, ctx);
    const payload = result.payload as { f_statistic: number; p_value: number; groups: string[] };
    expect(payload.groups).toEqual(['A', 'B', 'C']);
    expect(payload.f_statistic).toBeGreaterThan(0);
    expect(payload.p_value).toBeGreaterThanOrEqual(0);
    expect(result.analysis?.algorithm_id).toBe('anova');
    expect(result.analysis?.result_contract?.counts).toMatchObject({
      input_n: 9,
      complete_case_n: 9,
      missing_n: 0,
    });
  });

  it('runs Pearson correlation', async () => {
    const ctx = ctxFor(
      'x,y,unused\n1,2,\n2,4,\n3,6,\n4,8,\n5,10,\n',
      [num('x'), num('y'), num('unused')],
    );
    const result = await runner.run(reg.get('correlation')!, {
      x: 'x',
      y: 'y',
      dataset_id: 'ds-1',
    }, ctx);
    const payload = result.payload as { r: number; p_value: number; method: string };
    expect(payload.method).toBe('pearson');
    expect(payload.r).toBeCloseTo(1, 5);
    expect(payload.p_value).toBeGreaterThanOrEqual(0);
    expect(result.analysis?.algorithm_id).toBe('correlation');
    expect(result.analysis?.result_contract?.estimates?.[0]?.effect_unit).toBe('Correlation coefficient');
    expect(result.analysis?.result_contract?.counts).toMatchObject({
      input_n: 5,
      complete_case_n: 5,
      missing_n: 0,
    });
  });

  it('rejects Pearson correlation on a constant column instead of returning r=0/p=1', async () => {
    const ctx = ctxFor(
      'x,y\n1,7\n2,7\n3,7\n4,7\n5,7\n',
      [num('x'), num('y')],
    );
    await expect(runner.run(reg.get('correlation')!, {
      x: 'x',
      y: 'y',
      dataset_id: 'ds-1',
    }, ctx)).rejects.toMatchObject({
      detail: {
        kind: 'invalid_args',
        message: expect.stringMatching(/non-zero variance/i),
      },
    });
  });

  it('runs linear regression and attaches analysis metadata', async () => {
    const ctx = ctxFor('y,x\n1,1\n2,2\n3,3.1\n4,3.9\n5,5\n', [num('y'), num('x')]);
    const result = await runner.run(reg.get('model_linear')!, {
      outcome: 'y',
      predictors: ['x'],
      dataset_id: 'ds-1',
    }, ctx);
    expect(result.schema_version).toBe('1.0');
    const payload = result.payload as {
      r_squared: number;
      coefficients: Array<{ term?: string; index?: number }>;
      model_diagnostics: { collinearity: { status: string; rank: number; max_vif: number } };
    };
    expect(payload.r_squared).toBeGreaterThan(0.9);
    expect(payload.coefficients.map((c) => c.term)).toEqual(['(Intercept)', 'x']);
    expect(payload.model_diagnostics.collinearity).toMatchObject({ status: 'passed', rank: 1, max_vif: 1 });
    expect(result.analysis).not.toBeNull();
    expect(result.analysis?.algorithm_id).toBe('linear');
    expect(result.analysis?.dataset_id).toBe('ds-1');
    expect(result.analysis?.dataset_sha256).toBe(ctx.datasetSummary.sha256);
    expect(result.analysis?.result_contract).toMatchObject({
      schema_version: '1.0',
      method: { algorithm_id: 'linear', method_version: '1.0' },
      counts: {
        input_n: 5,
        complete_case_n: 5,
        missing_n: 0,
        event_n: null,
        person_time: null,
      },
      analysis_availability: { unadjusted: 'available', adjusted: 'not_computed' },
      convergence: { status: 'not_applicable' },
      provenance: { engine_name: '@stats-code/engine' },
    });
    expect((result.analysis?.result_contract as { estimates: unknown[] }).estimates).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ term: 'x', effect_unit: 'Beta', adjustment: 'unadjusted' }),
      ]),
    );
  });

  it('surfaces degenerate exact-fit coefficients in the linear API contract', async () => {
    const ctx = ctxFor('y,x\n-1,-1\n0,0\n1,1\n', [num('y'), num('x')]);
    const result = await runner.run(reg.get('model_linear')!, {
      outcome: 'y',
      predictors: ['x'],
      dataset_id: 'ds-1',
    }, ctx);
    const payload = result.payload as {
      degenerate: boolean;
      coefficients: Array<{ term: string; degenerate: boolean }>;
      model_diagnostics: { numerical_degeneracy: { status: string; coefficient_indexes: number[] } };
    };
    expect(payload.degenerate).toBe(true);
    expect(payload.coefficients.find((coefficient) => coefficient.term === 'x')?.degenerate).toBe(true);
    expect(payload.model_diagnostics.numerical_degeneracy).toMatchObject({
      status: 'warning',
      coefficient_indexes: [0, 1],
    });
    expect((result.analysis?.result_contract as { assumption_diagnostics: Array<{ code: string; status: string }> })
      .assumption_diagnostics).toEqual(expect.arrayContaining([
        expect.objectContaining({ code: 'model-numerical-degeneracy', status: 'warning' }),
      ]));
  });

  it('runs logistic regression', async () => {
    const ctx = ctxFor('y,x\n0,1\n0,2\n1,5\n1,6\n0,2\n1,7\n', [num('y'), num('x')]);
    const result = await runner.run(reg.get('model_logistic')!, {
      outcome: 'y',
      predictors: ['x'],
      dataset_id: 'ds-1',
    }, ctx);
    const payload = result.payload as {
      odds_ratios: number[];
      regularized: boolean;
      ridge_value: number;
      model_diagnostics: {
        convergence: { status: string };
        sparse_data: { status: string; information_per_predictor: number };
        separation_screen: { status: string };
        numerical_degeneracy: { status: string };
        regularization: { status: string; ridge_value: number };
      };
    };
    expect(Array.isArray(payload.odds_ratios)).toBe(true);
    expect(payload.regularized).toBe(false);
    expect(payload.ridge_value).toBe(0);
    expect(payload.model_diagnostics).toMatchObject({
      convergence: { status: 'failed' },
      sparse_data: { status: 'warning', information_per_predictor: 3 },
      separation_screen: { status: 'warning' },
      numerical_degeneracy: { status: 'passed' },
      regularization: { status: 'passed', ridge_value: 0 },
    });
    expect(result.risk_signals).toEqual(expect.arrayContaining(['ModelConvergenceFailed', 'SparseData']));
    expect(result.analysis?.algorithm_id).toBe('logistic');
    expect(result.analysis?.result_contract).toMatchObject({
      counts: { input_n: 6, complete_case_n: 6, event_n: 3 },
      convergence: { status: 'failed' },
    });
    expect((result.analysis?.result_contract as { estimates: Array<{ effect_unit: string }> }).estimates)
      .toEqual(expect.arrayContaining([expect.objectContaining({ effect_unit: 'OR' })]));
    expect((result.analysis?.result_contract as { assumption_diagnostics: Array<{ code: string; status: string }> })
      .assumption_diagnostics).toEqual(expect.arrayContaining([
        expect.objectContaining({ code: 'model-convergence', status: 'failed' }),
        expect.objectContaining({ code: 'model-sparse-data', status: 'warning' }),
        expect.objectContaining({ code: 'model-collinearity', status: 'passed' }),
        expect.objectContaining({ code: 'logistic-separation-screen', status: 'warning' }),
      ]));
  });

  it('runs cox regression', async () => {
    const ctx = ctxFor('t,e,x\n5,1,1\n6,0,2\n7,1,1\n8,1,3\n3,1,2\n9,0,1\n', [num('t'), num('e'), num('x')]);
    const result = await runner.run(reg.get('model_cox')!, {
      time: 't',
      event: 'e',
      predictors: ['x'],
      dataset_id: 'ds-1',
    }, ctx);
    const payload = result.payload as {
      hazard_ratios: number[];
      regularized: boolean;
      ridge_value: number;
      ph_test: { status: string; p_value: number; violated: boolean; recommendation: string };
      model_diagnostics: {
        sparse_data: { status: string; information_per_predictor: number };
        regularization: { status: string; ridge_value: number };
      };
    };
    expect(Array.isArray(payload.hazard_ratios)).toBe(true);
    expect(payload.regularized).toBe(false);
    expect(payload.ridge_value).toBe(0);
    expect(payload.ph_test.status).toBe('computed');
    expect(payload.ph_test.p_value).toBeGreaterThan(0.05);
    expect(payload.ph_test.violated).toBe(false);
    expect(payload.ph_test.recommendation).toContain('仍需结合图形');
    expect(payload.model_diagnostics.sparse_data).toMatchObject({ status: 'warning', information_per_predictor: 4 });
    expect(payload.model_diagnostics.regularization).toMatchObject({ status: 'passed', ridge_value: 0 });
    expect(result.risk_signals).toContain('SparseData');
    expect(result.analysis?.algorithm_id).toBe('cox');
    expect(result.analysis?.result_contract).toMatchObject({
      counts: { input_n: 6, complete_case_n: 6, event_n: 4, person_time: 38 },
      convergence: { status: 'converged' },
    });
    expect((result.analysis?.result_contract as { assumption_diagnostics: Array<{ code: string }> })
      .assumption_diagnostics).toEqual(
        expect.arrayContaining([
          expect.objectContaining({ code: 'model-convergence', status: 'passed' }),
          expect.objectContaining({ code: 'model-sparse-data', status: 'warning' }),
          expect.objectContaining({ code: 'model-collinearity', status: 'passed' }),
          expect.objectContaining({ code: 'cox-ph', status: 'passed' }),
        ]),
      );
  });

  it('rejects a rank-deficient regression design before fitting', async () => {
    const ctx = ctxFor('y,x,twice_x\n1,1,2\n2,2,4\n3,3,6\n4,4,8\n5,5,10\n', [num('y'), num('x'), num('twice_x')]);
    await expect(runner.run(reg.get('model_linear')!, {
      outcome: 'y',
      predictors: ['x', 'twice_x'],
      dataset_id: 'ds-1',
    }, ctx)).rejects.toMatchObject({
      detail: { kind: 'invalid_args', message: expect.stringMatching(/rank deficient/i) },
    });
  });

  it('rejects a non-binary Cox event indicator', async () => {
    const ctx = ctxFor('t,e,x\n1,0,1\n2,2,2\n3,1,3\n4,0,4\n', [num('t'), num('e'), num('x')]);
    await expect(runner.run(reg.get('model_cox')!, {
      time: 't',
      event: 'e',
      predictors: ['x'],
      dataset_id: 'ds-1',
    }, ctx)).rejects.toMatchObject({
      detail: { kind: 'invalid_args', message: expect.stringMatching(/encoded as 0\/1/) },
    });
  });

  it('runs kaplan-meier survival', async () => {
    const ctx = ctxFor(
      't,e,arm\n1,1,A\n2,1,A\n3,0,A\n4,1,A\n5,0,A\n2,0,B\n3,1,B\n4,0,B\n5,0,B\n6,1,B\n',
      [num('t'), num('e'), cat('arm')],
    );
    const result = await runner.run(reg.get('survival_km')!, {
      time: 't',
      event: 'e',
      group: 'arm',
      dataset_id: 'ds-1',
    }, ctx);
    const payload = result.payload as {
      survival_table: unknown[];
      groups: string[];
      steps: Array<{ group: string; std_error: number }>;
      group_summaries: Array<{ group: string; n: number; event_n: number; censored_n: number }>;
      log_rank: { status: string; statistic: number; p_value: number; degrees_of_freedom: number };
    };
    expect(Array.isArray(payload.survival_table)).toBe(true);
    expect(payload.groups).toEqual(['A', 'B']);
    expect(payload.group_summaries).toEqual([
      { group: 'A', n: 5, event_n: 3, censored_n: 2, median_survival: 4 },
      { group: 'B', n: 5, event_n: 2, censored_n: 3, median_survival: 6 },
    ]);
    expect(payload.log_rank).toMatchObject({ status: 'computed', degrees_of_freedom: 1 });
    expect(payload.log_rank.statistic).toBeCloseTo(1.5333798671220824, 12);
    expect(payload.log_rank.p_value).toBeCloseTo(0.21560589487391288, 9);
    expect(payload.steps.every((step) => Number.isFinite(step.std_error))).toBe(true);
    expect(result.analysis?.algorithm_id).toBe('kaplan_meier');
    expect(result.analysis?.result_contract).toMatchObject({
      counts: { input_n: 10, complete_case_n: 10, event_n: 5, person_time: 35 },
      convergence: { status: 'not_applicable' },
    });
  });

  it('rejects with invalid_args when a required argument is missing', async () => {
    const ctx = ctxFor('y,x\n1,1\n2,2\n', [num('y'), num('x')]);
    await expect(
      runner.run(reg.get('model_linear')!, { outcome: 'y', dataset_id: 'ds-1' }, ctx),
    ).rejects.toMatchObject({ detail: { kind: 'invalid_args', missing: ['predictors'] } });
  });

  it('maps a thrown engine input error to invalid_args with a bounded message (D15)', async () => {
    const ctx = ctxFor('y,x\n1,1\n2,2\n', [num('y'), num('x')]);
    try {
      await runner.run(reg.get('model_linear')!, {
        outcome: 'missing_col',
        predictors: ['x'],
        dataset_id: 'ds-1',
      }, ctx);
      expect.unreachable('should have thrown');
    } catch (err) {
      expect(err).toBeInstanceOf(SkillRunErrorException);
      const detail = (err as SkillRunErrorException).detail;
      expect(detail.kind).toBe('invalid_args');
      if (detail.kind === 'invalid_args') {
        expect(detail.message.length).toBeLessThanOrEqual(2048);
        expect(detail.message).toContain('column not found');
      }
    }
  });

  it('keeps runtime defects (non-plain Error subclasses) as execution_failed (D15)', async () => {
    const throwingRunner = new SkillRunner(reg);
    const ctx = ctxFor('y,x\n1,1\n2,2\n', [num('y'), num('x')]);
    // Force a TypeError inside execution by sabotaging the context shape.
    const broken = { ...ctx, datasetBytes: undefined } as unknown as SkillContext;
    await expect(throwingRunner.run(reg.get('model_linear')!, {
      outcome: 'y',
      predictors: ['x'],
      dataset_id: 'ds-1',
    }, broken)).rejects.toMatchObject({
      detail: { kind: 'execution_failed' },
    });
  });

  describe('degenerate analysis inputs reject as invalid_args, not 500 (D15)', () => {
    it('ttest with a single group', async () => {
      const ctx = ctxFor('score,arm\n10,A\n11,A\n12,A\n13,A\n', [num('score'), cat('arm')]);
      await expect(runner.run(reg.get('ttest')!, {
        group: 'arm', testVar: 'score', dataset_id: 'ds-1',
      }, ctx)).rejects.toMatchObject({
        detail: { kind: 'invalid_args', message: expect.stringMatching(/./) },
      });
    });

    it('ttest with a singleton group', async () => {
      const ctx = ctxFor('score,arm\n10,A\n20,B\n21,B\n22,B\n', [num('score'), cat('arm')]);
      await expect(runner.run(reg.get('ttest')!, {
        group: 'arm', testVar: 'score', dataset_id: 'ds-1',
      }, ctx)).rejects.toMatchObject({
        detail: { kind: 'invalid_args', message: expect.stringMatching(/./) },
      });
    });

    it('correlation with an unknown method', async () => {
      const ctx = ctxFor('x,y\n1,2\n2,4\n3,5\n4,8\n5,9\n', [num('x'), num('y')]);
      await expect(runner.run(reg.get('correlation')!, {
        x: 'x', y: 'y', method: 'bogus', dataset_id: 'ds-1',
      }, ctx)).rejects.toMatchObject({
        detail: { kind: 'invalid_args', message: expect.stringMatching(/./) },
      });
    });

    it('anova with non-numeric text in the outcome column', async () => {
      const ctx = ctxFor(
        'score,arm\nabc,A\n11,A\n20,B\n21,B\n30,C\n31,C\n',
        [num('score'), cat('arm')],
      );
      await expect(runner.run(reg.get('anova')!, {
        group: 'arm', testVar: 'score', dataset_id: 'ds-1',
      }, ctx)).rejects.toMatchObject({
        detail: { kind: 'invalid_args', message: expect.stringMatching(/./) },
      });
    });
  });

  it('runs native inspect skill without an algorithm mapping', async () => {
    const ctx = ctxFor('a,b\n1,2\n', [num('a'), num('b')]);
    const result = await runner.run(reg.get('inspect')!, { dataset_id: 'ds-1' }, ctx);
    const payload = result.payload as { row_count: number };
    expect(payload.row_count).toBe(1);
    expect(result.analysis).toBeNull();
  });

  it('exposes the bounded-search convergence state for native power results', async () => {
    const ctx = ctxFor('a\n1\n', [num('a')]);
    const result = await runner.run(reg.get('power')!, {
      test_type: 'means',
      effect_size: 0.5,
      alpha: 0.05,
      power: 0.8,
    }, ctx);
    expect(result.payload).toMatchObject({
      required_n: 64,
      converged: true,
    });
    expect(result.analysis).toBeNull();
  });

  describe('table one continuous group-difference tests (payload.continuous_tests)', () => {
    it('two groups → welch_t, matching a direct welchTtest call', async () => {
      const ctx = ctxFor('score,arm\n10,A\n11,A\n12,A\n20,B\n21,B\n22,B\n', [num('score'), cat('arm')]);
      const result = await runner.run(reg.get('tableone')!, {
        group: 'arm',
        continuous: ['score'],
        categorical: [],
        dataset_id: 'ds-1',
      }, ctx);
      const payload = result.payload as {
        continuous_tests: Array<{
          variable: string;
          status: string;
          method: string | null;
          statistic: number | null;
          degrees_of_freedom: number | null;
          degrees_of_freedom_denominator: number | null;
          p_value: number | null;
          groups: string[];
          group_ns: number[];
          degenerate: boolean;
        }>;
      };
      const entry = payload.continuous_tests.find((e) => e.variable === 'score')!;
      expect(entry.groups).toEqual(['A', 'B']);
      const direct = stats.ttest.welchTtest([10, 11, 12], [20, 21, 22], 0.05);
      expect(entry.status).toBe('computed');
      expect(entry.method).toBe('welch_t');
      expect(entry.statistic).toBeCloseTo(direct.tStatistic, 12);
      expect(entry.p_value).toBeCloseTo(direct.pValue, 12);
      expect(entry.degrees_of_freedom).toBeCloseTo(direct.df, 12);
      expect(entry.degrees_of_freedom_denominator).toBeNull();
      expect(entry.group_ns).toEqual([3, 3]);
      expect(entry.degenerate).toBe(false);
    });

    it('three groups → one_way_anova, matching a direct oneWayAnova call', async () => {
      const ctx = ctxFor(
        'score,arm\n10,A\n11,A\n12,A\n20,B\n21,B\n22,B\n30,C\n31,C\n32,C\n',
        [num('score'), cat('arm')],
      );
      const result = await runner.run(reg.get('tableone')!, {
        group: 'arm',
        continuous: ['score'],
        categorical: [],
        dataset_id: 'ds-1',
      }, ctx);
      const payload = result.payload as {
        continuous_tests: Array<{
          variable: string;
          status: string;
          method: string | null;
          statistic: number | null;
          degrees_of_freedom: number | null;
          degrees_of_freedom_denominator: number | null;
          p_value: number | null;
          group_ns: number[];
          groups: string[];
        }>;
      };
      const entry = payload.continuous_tests.find((e) => e.variable === 'score')!;
      expect(entry.groups).toEqual(['A', 'B', 'C']);
      const direct = stats.anova.oneWayAnova([[10, 11, 12], [20, 21, 22], [30, 31, 32]]);
      expect(entry.status).toBe('computed');
      expect(entry.method).toBe('one_way_anova');
      expect(entry.statistic).toBeCloseTo(direct.fStatistic, 12);
      expect(entry.p_value).toBeCloseTo(direct.pValue, 12);
      expect(entry.degrees_of_freedom).toBe(direct.dfBetween);
      expect(entry.degrees_of_freedom_denominator).toBe(direct.dfWithin);
      expect(entry.group_ns).toEqual([3, 3, 3]);
    });

    it('anova skips rows with a blank test value instead of scoring them as 0', async () => {
      // `Number('')` is 0, so a blank cell used to enter the analysis as a
      // literal zero — while resultCounts() reported that same row as excluded.
      // The statistics and the audit metadata describing them must agree.
      const ctx = ctxFor(
        'score,arm\n10,A\n11,A\n12,A\n,A\n20,B\n21,B\n22,B\n',
        [num('score'), cat('arm')],
      );
      const result = await runner.run(reg.get('anova')!, {
        group: 'arm',
        testVar: 'score',
        dataset_id: 'ds-1',
      }, ctx);
      const payload = result.payload as { n_total: number; group_ns: Record<string, number> };
      const direct = stats.anova.oneWayAnova([[10, 11, 12], [20, 21, 22]]);

      // 6 rows analysed, not 7: the blank row is excluded rather than scored as
      // zero. `complete_case_n` (attached on the research-workflow path) counts
      // by the same rule, so the two now describe the same rows.
      expect(payload.n_total).toBe(6);
      expect(payload.group_ns).toEqual({ A: 3, B: 3 });
      expect((payload as unknown as { f_statistic: number }).f_statistic)
        .toBeCloseTo(direct.fStatistic, 12);
    });

    it('anova rejects non-numeric text rather than silently dropping the row', async () => {
      // A non-empty, non-numeric cell counts as "complete" for the audit trail,
      // so dropping it quietly would put the reported N and the analysed N back
      // out of step. Surface it as an input error instead.
      const ctx = ctxFor(
        'score,arm\n10,A\n11,A\nabc,A\n20,B\n21,B\n22,B\n',
        [num('score'), cat('arm')],
      );
      await expect(runner.run(reg.get('anova')!, {
        group: 'arm',
        testVar: 'score',
        dataset_id: 'ds-1',
      }, ctx)).rejects.toThrow(SkillRunErrorException);
    });

    it('correlation rejects an unsupported method instead of substituting pearson', async () => {
      const ctx = ctxFor('x,y\n1,2\n2,4\n3,5\n4,9\n', [num('x'), num('y')]);
      await expect(runner.run(reg.get('correlation')!, {
        x: 'x',
        y: 'y',
        method: 'kendall',
        dataset_id: 'ds-1',
      }, ctx)).rejects.toThrow(SkillRunErrorException);
    });

    it('no stratification → all continuous variables not_computed, no throw', async () => {
      const ctx = ctxFor('age,bmi\n60,21\n64,25\n70,25\n72,29\n', [num('age'), num('bmi')]);
      const result = await runner.run(reg.get('tableone')!, {
        continuous: ['age', 'bmi'],
        categorical: [],
        dataset_id: 'ds-1',
      }, ctx);
      const payload = result.payload as {
        continuous_tests: Array<{ variable: string; status: string; reason: string | null }>;
      };
      expect(payload.continuous_tests).toHaveLength(2);
      for (const entry of payload.continuous_tests) {
        expect(entry.status).toBe('not_computed');
        expect(entry.reason).toBeTruthy();
      }
    });

    it('strata variable placed in the continuous list → not_applicable', async () => {
      const ctx = ctxFor('score,arm\n10,A\n11,A\n20,B\n21,B\n', [num('score'), cat('arm')]);
      const result = await runner.run(reg.get('tableone')!, {
        group: 'arm',
        continuous: ['arm', 'score'],
        categorical: [],
        dataset_id: 'ds-1',
      }, ctx);
      const payload = result.payload as {
        continuous_tests: Array<{ variable: string; status: string; method: string | null }>;
      };
      expect(payload.continuous_tests.find((e) => e.variable === 'arm')).toMatchObject({
        status: 'not_applicable',
        method: null,
      });
      expect(payload.continuous_tests.find((e) => e.variable === 'score')?.status).toBe('computed');
    });

    it('a group with a single non-missing value → not_computed (not NaN)', async () => {
      const ctx = ctxFor('val,arm\n10,A\n20,B\n21,B\n22,B\n', [num('val'), cat('arm')]);
      const result = await runner.run(reg.get('tableone')!, {
        group: 'arm',
        continuous: ['val'],
        categorical: [],
        dataset_id: 'ds-1',
      }, ctx);
      const payload = result.payload as {
        continuous_tests: Array<{
          variable: string;
          status: string;
          statistic: number | null;
          p_value: number | null;
          group_ns: number[];
        }>;
      };
      const entry = payload.continuous_tests.find((e) => e.variable === 'val')!;
      expect(entry.status).toBe('not_computed');
      expect(entry.statistic).toBeNull();
      expect(entry.p_value).toBeNull();
      expect(entry.group_ns).toEqual([1, 3]);
    });

    it('zero-variance groups (2-group Welch) → degenerate, no NaN leaked to payload', async () => {
      const ctx = ctxFor('val,arm\n5,A\n5,A\n7,B\n7,B\n', [num('val'), cat('arm')]);
      const result = await runner.run(reg.get('tableone')!, {
        group: 'arm',
        continuous: ['val'],
        categorical: [],
        dataset_id: 'ds-1',
      }, ctx);
      const payload = result.payload as {
        continuous_tests: Array<{
          variable: string;
          status: string;
          statistic: number | null;
          p_value: number | null;
          degenerate: boolean;
        }>;
      };
      const entry = payload.continuous_tests.find((e) => e.variable === 'val')!;
      expect(entry.status).toBe('not_computed');
      expect(entry.degenerate).toBe(true);
      expect(entry.statistic).toBeNull();
      expect(entry.p_value).toBeNull();
    });

    it('zero within-group variance across 3 groups (ANOVA non-finite F) → degenerate, no Infinity/NaN leaked', async () => {
      const ctx = ctxFor('val,arm\n5,A\n5,A\n7,B\n7,B\n9,C\n9,C\n', [num('val'), cat('arm')]);
      const result = await runner.run(reg.get('tableone')!, {
        group: 'arm',
        continuous: ['val'],
        categorical: [],
        dataset_id: 'ds-1',
      }, ctx);
      const payload = result.payload as {
        continuous_tests: Array<{
          variable: string;
          status: string;
          statistic: number | null;
          p_value: number | null;
          degenerate: boolean;
        }>;
      };
      const entry = payload.continuous_tests.find((e) => e.variable === 'val')!;
      // Engine returns fStatistic = Infinity (msWithin = 0, msBetween > 0, per
      // direct probe of oneWayAnova); the guard must catch the non-finite
      // statistic and blank it rather than leak Infinity into the payload.
      expect(entry.status).toBe('not_computed');
      expect(entry.degenerate).toBe(true);
      expect(entry.statistic).toBeNull();
      expect(entry.p_value).toBeNull();
    });

    it('all values identical across all 3 groups (msWithin=0 AND msBetween=0 → F=0 finite) → still not_computed, degenerate', async () => {
      const ctx = ctxFor('val,arm\n5,A\n5,A\n5,A\n5,B\n5,B\n5,B\n5,C\n5,C\n5,C\n', [num('val'), cat('arm')]);
      const result = await runner.run(reg.get('tableone')!, {
        group: 'arm',
        continuous: ['val'],
        categorical: [],
        dataset_id: 'ds-1',
      }, ctx);
      const payload = result.payload as {
        continuous_tests: Array<{
          variable: string;
          status: string;
          statistic: number | null;
          p_value: number | null;
          degenerate: boolean;
        }>;
      };
      const entry = payload.continuous_tests.find((e) => e.variable === 'val')!;
      // Engine returns fStatistic = 0 here (msWithin = 0 AND msBetween = 0, so
      // the `msBetween > 0` branch is false too), which is a *finite* number —
      // the isFinite guard alone would miss this. `result.degenerate` (true,
      // because msWithin === 0) is what must catch it.
      expect(entry.status).toBe('not_computed');
      expect(entry.degenerate).toBe(true);
      expect(entry.statistic).toBeNull();
      expect(entry.p_value).toBeNull();
    });

    it('within-group variance zero but between-group means differ (msWithin=0, msBetween>0 → F=Infinity) → not_computed, degenerate', async () => {
      const ctx = ctxFor('val,arm\n1,A\n1,A\n1,A\n2,B\n2,B\n2,B\n3,C\n3,C\n3,C\n', [num('val'), cat('arm')]);
      const result = await runner.run(reg.get('tableone')!, {
        group: 'arm',
        continuous: ['val'],
        categorical: [],
        dataset_id: 'ds-1',
      }, ctx);
      const payload = result.payload as {
        continuous_tests: Array<{
          variable: string;
          status: string;
          statistic: number | null;
          p_value: number | null;
          degenerate: boolean;
        }>;
      };
      const entry = payload.continuous_tests.find((e) => e.variable === 'val')!;
      expect(entry.status).toBe('not_computed');
      expect(entry.degenerate).toBe(true);
      expect(entry.statistic).toBeNull();
      expect(entry.p_value).toBeNull();
    });

    it('group_ns stays consistent with the displayed per-group summary n, including missing exclusion', async () => {
      const ctx = ctxFor(
        'age,bmi,arm\n60,21,A\n64,,A\n62,22,A\n70,25,B\n72,29,B\n80,31,C\n82,33,C\n84,35,C\n',
        [num('age'), num('bmi'), cat('arm')],
      );
      const result = await runner.run(reg.get('tableone')!, {
        group: 'arm',
        continuous: ['age', 'bmi'],
        categorical: [],
        dataset_id: 'ds-1',
      }, ctx);
      const payload = result.payload as {
        groups: Array<{ label: string; continuous: Array<{ variable: string; n: number; missing: number }> }>;
        continuous_tests: Array<{ variable: string; groups: string[]; group_ns: number[] }>;
      };
      expect(payload.groups.map((g) => g.label)).toEqual(['A', 'B', 'C']);
      const bmiGroupA = payload.groups[0]!.continuous.find((c) => c.variable === 'bmi')!;
      expect(bmiGroupA).toMatchObject({ n: 2, missing: 1 });

      expect(payload.continuous_tests.length).toBeGreaterThan(0);
      for (const test of payload.continuous_tests) {
        for (const [groupIndex, groupLabel] of test.groups.entries()) {
          const groupSummary = payload.groups.find((g) => g.label === groupLabel)!;
          const summary = groupSummary.continuous.find((c) => c.variable === test.variable)!;
          expect(test.group_ns[groupIndex]).toBe(summary.n);
        }
      }
    });
  });
});

describe('regression dummy encoding for categorical predictors', () => {
  const reg = SkillRegistry.withDefaults();
  const runner = new SkillRunner(reg);

  // 12 行、smoke 有 3 个水平（current/former/never），age 为连续变量。
  const csv = [
    'y,age,smoke',
    '10,40,never',
    '12,45,never',
    '14,50,never',
    '16,55,never',
    '21,42,former',
    '23,47,former',
    '25,52,former',
    '27,57,former',
    '31,41,current',
    '33,46,current',
    '35,51,current',
    '37,56,current',
  ].join('\n') + '\n';
  const columns = [num('y'), num('age'), cat('smoke')];

  it('runs linear regression with a categorical predictor instead of throwing', async () => {
    const ctx = ctxFor(csv, columns);
    const result = await runner.run(reg.get('model_linear')!, {
      outcome: 'y',
      predictors: ['age', 'smoke'],
      dataset_id: 'ds-1',
    }, ctx);
    const payload = result.payload as {
      coefficients: Array<{ term: string; reference: string | null; beta: number }>;
    };
    // 3 个水平 → drop-first 后 2 个哑变量；加上截距与 age 共 4 项。
    expect(payload.coefficients).toHaveLength(4);
    expect(payload.coefficients.map((c) => c.term)).toEqual([
      '(Intercept)', 'age', 'smoke=former', 'smoke=never',
    ]);
  });

  it('names the reference level on every dummy term and leaves numeric terms null', async () => {
    const ctx = ctxFor(csv, columns);
    const result = await runner.run(reg.get('model_linear')!, {
      outcome: 'y',
      predictors: ['age', 'smoke'],
      dataset_id: 'ds-1',
    }, ctx);
    const payload = result.payload as {
      coefficients: Array<{ term: string; reference: string | null }>;
    };
    const byTerm = new Map(payload.coefficients.map((c) => [c.term, c.reference]));
    // 参考水平是排序后的第一个：current < former < never
    expect(byTerm.get('smoke=former')).toBe('current');
    expect(byTerm.get('smoke=never')).toBe('current');
    expect(byTerm.get('age')).toBeNull();
    expect(byTerm.get('(Intercept)')).toBeNull();
  });

  it('recovers the known level effects from a constructed design', async () => {
    // 构造数据里 former 比 current 低约 10、never 比 current 低约 20，
    // 哑变量编码正确时系数应当复现这个差距（容差留给 age 的共变）。
    const ctx = ctxFor(csv, columns);
    const result = await runner.run(reg.get('model_linear')!, {
      outcome: 'y',
      predictors: ['age', 'smoke'],
      dataset_id: 'ds-1',
    }, ctx);
    const payload = result.payload as {
      coefficients: Array<{ term: string; estimate: number }>;
    };
    const beta = (term: string) => payload.coefficients.find((c) => c.term === term)!.estimate;
    expect(beta('smoke=former')).toBeLessThan(-5);
    expect(beta('smoke=never')).toBeLessThan(beta('smoke=former'));
  });

  it('keeps a purely numeric predictor list byte-identical to the old behaviour', async () => {
    const ctx = ctxFor(csv, columns);
    const result = await runner.run(reg.get('model_linear')!, {
      outcome: 'y',
      predictors: ['age'],
      dataset_id: 'ds-1',
    }, ctx);
    const payload = result.payload as {
      coefficients: Array<{ term: string; reference: string | null }>;
    };
    expect(payload.coefficients.map((c) => c.term)).toEqual(['(Intercept)', 'age']);
    expect(payload.coefficients.every((c) => c.reference === null)).toBe(true);
  });

  it('rejects a categorical predictor with a single level rather than silently dropping it', async () => {
    const flat = 'y,age,arm\n10,40,A\n12,45,A\n14,50,A\n16,55,A\n18,60,A\n';
    const ctx = ctxFor(flat, [num('y'), num('age'), cat('arm')]);
    await expect(runner.run(reg.get('model_linear')!, {
      outcome: 'y',
      predictors: ['age', 'arm'],
      dataset_id: 'ds-1',
    }, ctx)).rejects.toMatchObject({
      detail: { kind: 'invalid_args', message: expect.stringMatching(/no variation/i) },
    });
  });

  it('rejects a missing value in a categorical predictor instead of inventing a level', async () => {
    const holed = 'y,smoke\n10,never\n12,\n14,former\n16,current\n18,never\n';
    const ctx = ctxFor(holed, [num('y'), cat('smoke')]);
    await expect(runner.run(reg.get('model_linear')!, {
      outcome: 'y',
      predictors: ['smoke'],
      dataset_id: 'ds-1',
    }, ctx)).rejects.toMatchObject({
      detail: {
        kind: 'invalid_args',
        message: expect.stringMatching(/missing value in column smoke/i),
      },
    });
  });

  it('encodes categorical predictors for logistic regression too', async () => {
    const binary = [
      'event,smoke',
      '0,never', '0,never', '0,never', '1,never',
      '0,former', '1,former', '1,former', '0,former',
      '1,current', '1,current', '1,current', '0,current',
    ].join('\n') + '\n';
    const ctx = ctxFor(binary, [num('event'), cat('smoke')]);
    const result = await runner.run(reg.get('model_logistic')!, {
      outcome: 'event',
      predictors: ['smoke'],
      dataset_id: 'ds-1',
    }, ctx);
    const payload = result.payload as {
      coefficients: Array<{ term: string; reference: string | null }>;
    };
    expect(payload.coefficients.map((c) => c.term)).toEqual([
      '(Intercept)', 'smoke=former', 'smoke=never',
    ]);
    expect(payload.coefficients[1]!.reference).toBe('current');
  });

  it('encodes categorical predictors for cox regression without an intercept term', async () => {
    const surv = [
      'time,dead,smoke',
      '5,1,never', '9,0,never', '12,1,never', '15,0,never',
      '4,1,former', '7,1,former', '11,0,former', '14,1,former',
      '2,1,current', '3,1,current', '6,1,current', '8,1,current',
    ].join('\n') + '\n';
    const ctx = ctxFor(surv, [num('time'), num('dead'), cat('smoke')]);
    const result = await runner.run(reg.get('model_cox')!, {
      time: 'time',
      event: 'dead',
      predictors: ['smoke'],
      dataset_id: 'ds-1',
    }, ctx);
    const payload = result.payload as {
      coefficients: Array<{ term: string; reference: string | null }>;
    };
    // Cox 无截距：只有 2 个哑变量项
    expect(payload.coefficients.map((c) => c.term)).toEqual(['smoke=former', 'smoke=never']);
    expect(payload.coefficients.every((c) => c.reference === 'current')).toBe(true);
  });
});

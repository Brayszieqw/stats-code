// tests/unit/skill-runner.test.ts — in-process SkillRunner (task 5.9).
//
// In-process invocation of linear/logistic/cox/kaplan_meier against a fixture
// dataset returns a SkillResult with analysis metadata; missing arg →
// invalid_args; thrown engine error → execution_failed (≤ 2048 chars).
//
// _Requirements: 5.2, 5.3, 5.5, 5.6_

import { describe, it, expect } from 'vitest';
import { createHash } from 'node:crypto';
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
      detail: { kind: 'execution_failed', diagnosticExcerpt: expect.stringMatching(/rank deficient/i) },
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
      detail: { kind: 'execution_failed', diagnosticExcerpt: expect.stringMatching(/encoded as 0\/1/) },
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

  it('maps a thrown engine error to execution_failed with a bounded excerpt', async () => {
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
      expect(detail.kind).toBe('execution_failed');
      if (detail.kind === 'execution_failed') {
        expect(detail.diagnosticExcerpt.length).toBeLessThanOrEqual(2048);
        expect(detail.diagnosticExcerpt).toContain('column not found');
      }
    }
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
  });
});

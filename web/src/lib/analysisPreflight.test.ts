import { describe, expect, it } from 'vitest';
import type { DatasetSummary, ResearchProtocol, RunRequest } from '../api/types';
import { buildAnalysisPreflight } from './analysisPreflight';

const dataset: DatasetSummary = {
  dataset_id: 'ds-1',
  file_name: 'cohort.csv',
  size_bytes: 2048,
  encoding: 'Utf8',
  row_count: 20,
  columns: [
    { name: 'outcome', inferred_type: 'Numeric', missing_count: 0 },
    { name: 'age', inferred_type: 'Numeric', missing_count: 1 },
    { name: 'bmi', inferred_type: 'Numeric', missing_count: 4 },
    { name: 'group', inferred_type: 'Categorical', missing_count: 0 },
  ],
  uploaded_at: '2026-01-01T00:00:00Z',
};

describe('buildAnalysisPreflight', () => {
  it('summarises method, variables, sample size and 5%/20% missing boundaries', () => {
    const request: RunRequest = {
      skill_id: 'model_linear',
      dataset_id: dataset.dataset_id,
      args: { outcome: 'outcome', predictors: ['age', 'bmi'] },
    };

    const result = buildAnalysisPreflight(dataset, request, '分析 outcome 与 age、bmi 的关系');

    expect(result.methodLabel).toBe('多元线性回归');
    expect(result.rowCount).toBe(20);
    expect(result.variables).toEqual(['outcome', 'age', 'bmi']);
    expect(result.missingRates).toEqual([
      { variable: 'outcome', missingCount: 0, rate: 0 },
      { variable: 'age', missingCount: 1, rate: 5 },
      { variable: 'bmi', missingCount: 4, rate: 20 },
    ]);
    expect(result.warnings.map((warning) => warning.code)).toEqual([
      'small-sample',
      'missing-data',
      'missing-data',
    ]);
    expect(result.warnings.find((warning) => warning.message.includes('bmi'))?.severity).toBe('high');
  });

  it('warns about causal wording and paired wording for an independent t-test', () => {
    const request: RunRequest = {
      skill_id: 'ttest',
      dataset_id: dataset.dataset_id,
      args: { group: 'group', testVar: 'outcome' },
    };

    const result = buildAnalysisPreflight(
      { ...dataset, row_count: 100 },
      request,
      '比较干预前后差异并判断是否导致 outcome 改善',
    );

    expect(result.variables).toEqual(['group', 'outcome']);
    expect(result.warnings.map((warning) => warning.code)).toContain('causal-language');
    expect(result.warnings.map((warning) => warning.code)).toContain('paired-design');
  });

  it('handles an empty dataset without dividing by zero', () => {
    const request: RunRequest = {
      skill_id: 'tableone',
      dataset_id: dataset.dataset_id,
      args: { continuous: ['age'], categorical: [] },
    };

    const result = buildAnalysisPreflight({ ...dataset, row_count: 0 }, request, '描述基线');

    expect(result.missingRates[0]?.rate).toBe(0);
    expect(result.warnings.some((warning) => warning.code === 'small-sample')).toBe(true);
  });

  it('flags outcome and method mismatches against an approved protocol', () => {
    const request: RunRequest = {
      skill_id: 'model_linear',
      dataset_id: dataset.dataset_id,
      args: { outcome: 'bmi', predictors: ['age'] },
    };
    const protocol: ResearchProtocol = {
      status: 'Approved',
      research_question: '吸烟与疾病结局是否相关？',
      study_design: 'cross_sectional',
      population: '成人观察性队列',
      eligibility_criteria: '有基线记录',
      exposure: 'smoke',
      comparator: 'never',
      outcome: 'disease（二分类疾病结局）',
      time_zero: '基线',
      follow_up: '不适用',
      analysis_unit: '参与者',
      estimand: '吸烟与疾病患病几率的调整后 OR',
      confounders: 'age, bmi',
      missing_data_strategy: '完整案例',
      primary_analysis: '多变量 Logistic 回归',
      sensitivity_analysis: '',
      version: 1,
      content_sha256: 'a'.repeat(64),
      state_sha256: 'e'.repeat(64),
      approval_id: '11111111-1111-4111-8111-111111111111',
      approved_at: '2026-01-01T00:00:00Z',
      updated_at: '2026-01-01T00:00:00Z',
    };

    const result = buildAnalysisPreflight(dataset, request, '对 bmi 建立线性回归', protocol);

    expect(result.warnings.map((warning) => warning.code)).toEqual(expect.arrayContaining([
      'protocol-outcome-mismatch',
      'protocol-method-mismatch',
    ]));
    expect(result.warnings.find((warning) => warning.code === 'protocol-outcome-mismatch')?.severity).toBe('high');
  });
});

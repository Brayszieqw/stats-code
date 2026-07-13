import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { ThreeLineTable } from './ThreeLineTable';
import type { SkillResult } from '../api/types';

describe('ThreeLineTable engine tableone payload', () => {
  it('renders continuous and categorical rows from payload.groups', () => {
    const skillResult: SkillResult = {
      schema_version: '1.0',
      payload: {
        strata: null,
        continuous: ['age', 'bmi'],
        categorical: ['sex'],
        groups: [
          {
            label: 'Overall',
            n: 240,
            continuous: [
              {
                variable: 'age',
                type: 'continuous',
                n: 240,
                missing: 0,
                mean: 49.86,
                sd: 12.95,
                median: 50,
                q1: 39,
                q3: 61,
              },
              {
                variable: 'bmi',
                type: 'continuous',
                n: 240,
                missing: 0,
                mean: 26.9,
                sd: 3.47,
                median: 26.85,
                q1: 23.9,
                q3: 29.9,
              },
            ],
            categorical: [
              {
                variable: 'sex',
                type: 'categorical',
                n: 240,
                missing: 0,
                levels: [
                  { level: 'female', count: 120, percent: 50 },
                  { level: 'male', count: 120, percent: 50 },
                ],
              },
            ],
          },
        ],
      },
      risk_signals: [],
    };

    render(<ThreeLineTable skillResult={skillResult} />);
    expect(screen.getByLabelText('Table One 三线表')).toBeInTheDocument();
    expect(screen.getByText('Overall (N=240)')).toBeInTheDocument();
    expect(screen.getByText('age')).toBeInTheDocument();
    expect(screen.getByText('bmi')).toBeInTheDocument();
    expect(screen.getByText('sex')).toBeInTheDocument();
    expect(screen.getByText('female')).toBeInTheDocument();
    expect(screen.getAllByText(/120 \(50\.0%\)/).length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText(/49\.860/)).toBeInTheDocument();
  });

  it('labels regression coefficients from analysis params when term is missing', () => {
    const skillResult: SkillResult = {
      schema_version: '1.0',
      payload: {
        coefficients: [
          { index: 0, estimate: 26.7, stdError: 0.9, pValue: 0.001, ciLower: 24, ciUpper: 28 },
          { index: 1, estimate: 0.003, stdError: 0.017, pValue: 0.86, ciLower: -0.03, ciUpper: 0.04 },
        ],
      },
      risk_signals: [],
      analysis: {
        algorithm_id: 'linear',
        dataset_id: 'ds-1',
        dataset_sha256: null,
        columns: [],
        params: { outcome: 'bmi', predictors: ['age'] },
        run_id: 'run-1',
        run_status: 'completed',
      },
    };

    render(<ThreeLineTable skillResult={skillResult} />);
    expect(screen.getByText('(Intercept)')).toBeInTheDocument();
    expect(screen.getByText('age')).toBeInTheDocument();
  });

  it('renders SMD plus per-group valid and missing counts for a two-group Table One', () => {
    const skillResult: SkillResult = {
      schema_version: '1.0',
      payload: {
        strata: 'arm',
        continuous: ['age'],
        categorical: ['sex'],
        groups: [
          {
            label: 'A',
            n: 100,
            continuous: [{ variable: 'age', type: 'continuous', n: 98, missing: 2, mean: 50, sd: 10, median: 50, q1: 43, q3: 57 }],
            categorical: [{
              variable: 'sex', type: 'categorical', n: 99, missing: 1,
              levels: [{ level: 'female', count: 55, percent: 55.56 }, { level: 'male', count: 44, percent: 44.44 }],
            }],
          },
          {
            label: 'B',
            n: 100,
            continuous: [{ variable: 'age', type: 'continuous', n: 100, missing: 0, mean: 51.25, sd: 10, median: 51, q1: 44, q3: 58 }],
            categorical: [{
              variable: 'sex', type: 'categorical', n: 100, missing: 0,
              levels: [{ level: 'female', count: 45, percent: 45 }, { level: 'male', count: 55, percent: 55 }],
            }],
          },
        ],
        standardized_differences: {
          comparison: { first: 'A', second: 'B' },
          continuous: [{ variable: 'age', smd: 0.125 }],
          categorical: [{
            variable: 'sex',
            smd: 0.212,
            levels: [{ level: 'female', smd: 0.212 }, { level: 'male', smd: 0.212 }],
          }],
        },
        categorical_tests: [{
          variable: 'sex',
          status: 'computed',
          method: 'fisher_exact',
          statistic: null,
          degrees_of_freedom: null,
          p_value: 0.031,
          min_expected_count: 4.5,
          expected_below_5: 1,
          observed_zero_cells: 1,
          reason: '期望频数小于 5 或出现零格，已自动使用 Fisher 精确检验。',
        }],
      },
      risk_signals: [],
    };

    render(<ThreeLineTable skillResult={skillResult} />);

    expect(screen.getByRole('columnheader', { name: 'SMD' })).toBeInTheDocument();
    expect(screen.getByText('有效 n=98 · 缺失=2')).toBeInTheDocument();
    expect(screen.getAllByText('有效 n=100 · 缺失=0')).toHaveLength(2);
    expect(screen.getByText('0.125')).toBeInTheDocument();
    expect(screen.getByText('最大 |SMD| 0.212')).toBeInTheDocument();
    expect(screen.getAllByText('0.212')).toHaveLength(2);
    expect(screen.getByText('Fisher 精确检验 · p=0.031 · 零频单元 1')).toBeInTheDocument();
  });

  it('shows why a sparse categorical comparison was not computed', () => {
    const skillResult: SkillResult = {
      schema_version: '1.0',
      payload: {
        strata: 'arm',
        categorical: ['stage'],
        groups: [
          { label: 'A', n: 5, categorical: [{ variable: 'stage', n: 5, missing: 0, levels: [{ level: 'I', count: 1, percent: 20 }, { level: 'II', count: 4, percent: 80 }] }] },
          { label: 'B', n: 5, categorical: [{ variable: 'stage', n: 5, missing: 0, levels: [{ level: 'I', count: 2, percent: 40 }, { level: 'III', count: 3, percent: 60 }] }] },
        ],
        categorical_tests: [{
          variable: 'stage',
          status: 'not_computed',
          method: null,
          statistic: null,
          degrees_of_freedom: null,
          p_value: null,
          min_expected_count: 0.5,
          expected_below_5: 6,
          observed_zero_cells: 2,
          reason: '存在期望频数小于 5；当前仅对 2×2 表提供 Fisher 精确检验。',
        }],
      },
      risk_signals: [],
    };

    render(<ThreeLineTable skillResult={skillResult} />);
    expect(screen.getByText(/组间检验未计算：存在期望频数小于 5/)).toBeInTheDocument();
  });
});

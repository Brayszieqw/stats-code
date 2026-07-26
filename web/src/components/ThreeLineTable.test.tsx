import { describe, it, expect } from 'vitest';
import { render, screen, within } from '@testing-library/react';
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
    expect(screen.getByText('Fisher 精确检验 · 零频单元 1')).toBeInTheDocument();
    expect(screen.getByRole('columnheader', { name: 'p 值' })).toBeInTheDocument();
    expect(screen.getByText('0.031')).toHaveClass('sig');
  });

  it('renders a unified p column with continuous test results, degenerate flag, and not_computed placeholder', () => {
    const skillResult: SkillResult = {
      schema_version: '1.0',
      payload: {
        strata: 'arm',
        continuous: ['age', 'bmi', 'height'],
        categorical: [],
        groups: [
          {
            label: 'A',
            n: 3,
            continuous: [
              { variable: 'age', type: 'continuous', n: 3, missing: 0, mean: 50, sd: 10, median: 50, q1: 43, q3: 57 },
              { variable: 'bmi', type: 'continuous', n: 3, missing: 0, mean: 25, sd: 0, median: 25, q1: 25, q3: 25 },
              { variable: 'height', type: 'continuous', n: 3, missing: 0, mean: 170, sd: 5, median: 170, q1: 165, q3: 175 },
            ],
            categorical: [],
          },
          {
            label: 'B',
            n: 3,
            continuous: [
              { variable: 'age', type: 'continuous', n: 3, missing: 0, mean: 55, sd: 10, median: 55, q1: 48, q3: 62 },
              { variable: 'bmi', type: 'continuous', n: 3, missing: 0, mean: 25, sd: 0, median: 25, q1: 25, q3: 25 },
              { variable: 'height', type: 'continuous', n: 3, missing: 0, mean: 172, sd: 5, median: 172, q1: 167, q3: 177 },
            ],
            categorical: [],
          },
          {
            label: 'C',
            n: 3,
            continuous: [
              { variable: 'age', type: 'continuous', n: 3, missing: 0, mean: 60, sd: 10, median: 60, q1: 53, q3: 67 },
              { variable: 'bmi', type: 'continuous', n: 3, missing: 0, mean: 25, sd: 0, median: 25, q1: 25, q3: 25 },
              { variable: 'height', type: 'continuous', n: 3, missing: 0, mean: 174, sd: 5, median: 174, q1: 169, q3: 179 },
            ],
            categorical: [],
          },
        ],
        continuous_tests: [
          {
            variable: 'age',
            status: 'computed',
            method: 'one_way_anova',
            statistic: 4.2,
            degrees_of_freedom: 2,
            degrees_of_freedom_denominator: 6,
            p_value: 0.031,
            groups: ['A', 'B', 'C'],
            group_ns: [3, 3, 3],
            degenerate: false,
            reason: null,
          },
          {
            variable: 'bmi',
            status: 'computed',
            method: 'one_way_anova',
            statistic: null,
            degrees_of_freedom: 2,
            degrees_of_freedom_denominator: 6,
            p_value: 1,
            groups: ['A', 'B', 'C'],
            group_ns: [3, 3, 3],
            degenerate: true,
            reason: '组内方差为零，检验统计量退化。',
          },
          {
            variable: 'height',
            status: 'not_computed',
            method: null,
            statistic: null,
            degrees_of_freedom: null,
            degrees_of_freedom_denominator: null,
            p_value: null,
            groups: ['A', 'B', 'C'],
            group_ns: [3, 3, 3],
            degenerate: false,
            reason: '样本量不足。',
          },
        ],
      },
      risk_signals: [],
    };

    render(<ThreeLineTable skillResult={skillResult} />);

    expect(screen.getByRole('columnheader', { name: 'p 值' })).toBeInTheDocument();
    expect(screen.getAllByText('单因素 ANOVA')).toHaveLength(2);
    expect(screen.getByText('0.031')).toHaveClass('sig');
    expect(screen.getByTitle('组内方差为零，检验统计量退化。')).toBeInTheDocument();
    expect(screen.getByText('组间检验未计算：样本量不足。')).toBeInTheDocument();
    expect(screen.getAllByText('-').length).toBeGreaterThanOrEqual(1);

    // strata row colSpan = 1 (label) + 3 (groups) + 0 (no SMD) + 1 (p column) = 5
    const strataCell = screen.getByText('按 arm 分层');
    expect(strataCell).toHaveAttribute('colspan', '5');
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

describe('ThreeLineTable variable filtering', () => {
  const tableOne: SkillResult = {
    schema_version: '1.0',
    payload: {
      strata: 'sex',
      continuous: ['age', 'bmi'],
      categorical: ['smoke'],
      groups: [
        {
          label: 'female',
          n: 120,
          continuous: [
            { variable: 'age', n: 120, missing: 0, mean: 48.9, sd: 13.4, median: 48, q1: 37, q3: 61 },
            { variable: 'bmi', n: 120, missing: 0, mean: 27.0, sd: 3.5, median: 26.9, q1: 24.0, q3: 30.0 },
          ],
          categorical: [
            { variable: 'smoke', n: 120, missing: 0, levels: [{ level: 'never', count: 40, percent: 33.3 }] },
          ],
        },
        {
          label: 'male',
          n: 120,
          continuous: [
            { variable: 'age', n: 120, missing: 0, mean: 50.8, sd: 12.5, median: 51.5, q1: 40.8, q3: 61 },
            { variable: 'bmi', n: 120, missing: 0, mean: 26.9, sd: 3.5, median: 26.9, q1: 23.8, q3: 29.7 },
          ],
          categorical: [
            { variable: 'smoke', n: 120, missing: 0, levels: [{ level: 'never', count: 40, percent: 33.3 }] },
          ],
        },
      ],
    },
    risk_signals: [],
  };

  it('keeps every variable when no keyword is given', () => {
    render(<ThreeLineTable skillResult={tableOne} />);
    expect(screen.getByText('age')).toBeInTheDocument();
    expect(screen.getByText('bmi')).toBeInTheDocument();
    expect(screen.getByText('smoke')).toBeInTheDocument();
  });

  it('hides variables whose name does not match the keyword', () => {
    render(<ThreeLineTable skillResult={tableOne} filterKeyword="bmi" />);
    expect(screen.getByText('bmi')).toBeInTheDocument();
    expect(screen.queryByText('age')).not.toBeInTheDocument();
    expect(screen.queryByText('smoke')).not.toBeInTheDocument();
    // 隐藏计数与「快照仍含全部变量」的说明必须出现，否则用户会以为结果只有一个变量
    expect(screen.getByText(/已隐藏 2 个变量/)).toBeInTheDocument();
  });

  it('drops a categorical variable together with its level rows', () => {
    render(<ThreeLineTable skillResult={tableOne} filterKeyword="age" />);
    expect(screen.getByText('age')).toBeInTheDocument();
    expect(screen.queryByText('smoke')).not.toBeInTheDocument();
    // 水平行不得成为孤儿留在表里
    expect(screen.queryByText('never')).not.toBeInTheDocument();
  });

  it('matches case-insensitively', () => {
    render(<ThreeLineTable skillResult={tableOne} filterKeyword="BMI" />);
    expect(screen.getByText('bmi')).toBeInTheDocument();
    expect(screen.queryByText('age')).not.toBeInTheDocument();
  });

  it('states plainly when nothing matches', () => {
    render(<ThreeLineTable skillResult={tableOne} filterKeyword="zzz" />);
    expect(screen.getByText(/没有变量名匹配/)).toBeInTheDocument();
    expect(screen.queryByText('age')).not.toBeInTheDocument();
  });
});

describe('ThreeLineTable one-way ANOVA payload', () => {
  const anova: SkillResult = {
    schema_version: '1.0',
    payload: {
      method: 'one_way_anova',
      group_variable: 'smoke',
      test_variable: 'age',
      groups: ['current', 'former', 'never'],
      group_ns: { current: 80, former: 80, never: 80 },
      k: 3,
      n_total: 240,
      ss_between: 260.5,
      ss_within: 40234.4,
      ss_total: 40494.9,
      df_between: 2,
      df_within: 237,
      ms_between: 130.25,
      ms_within: 169.77,
      f_statistic: 0.767,
      p_value: 0.4655,
      eta_squared: 0.0064,
      degenerate: false,
    },
    risk_signals: [],
    analysis: { algorithm_id: 'anova' } as SkillResult['analysis'],
  };

  it('renders the standard source-of-variation table instead of an empty shell', () => {
    render(<ThreeLineTable skillResult={anova} />);
    expect(screen.getByRole('table', { name: '单因素方差分析表' })).toBeInTheDocument();
    expect(screen.getByText('组间（smoke）')).toBeInTheDocument();
    expect(screen.getByText('组内（误差）')).toBeInTheDocument();
    expect(screen.getByText('总计')).toBeInTheDocument();
  });

  it('shows F, df and p, and derives the total df', () => {
    render(<ThreeLineTable skillResult={anova} />);
    expect(screen.getByText('0.767')).toBeInTheDocument();
    expect(screen.getByText('0.466')).toBeInTheDocument();
    expect(screen.getByText('2')).toBeInTheDocument();
    expect(screen.getByText('237')).toBeInTheDocument();
    // 总计 df = df_between + df_within
    expect(screen.getByText('239')).toBeInTheDocument();
  });

  it('reports the effect size and the assumptions it did not check', () => {
    render(<ThreeLineTable skillResult={anova} />);
    expect(screen.getByText(/η²=0\.006/)).toBeInTheDocument();
    expect(screen.getByText(/未做事后两两比较/)).toBeInTheDocument();
    expect(screen.getByText(/正态性与方差齐性未自动检验/)).toBeInTheDocument();
  });

  it('flags a degenerate statistic as uninterpretable', () => {
    const degenerate: SkillResult = {
      ...anova,
      payload: { ...(anova.payload as Record<string, unknown>), degenerate: true },
    };
    render(<ThreeLineTable skillResult={degenerate} />);
    expect(screen.getByText(/统计量数值退化，结果不可解释/)).toBeInTheDocument();
  });

  it('does not hijack a tableone payload that uses one_way_anova per variable', () => {
    const tableOneWithAnova: SkillResult = {
      schema_version: '1.0',
      payload: {
        strata: 'smoke',
        continuous: ['age'],
        groups: [
          { label: 'current', n: 80, continuous: [{ variable: 'age', n: 80, missing: 0, mean: 49, sd: 13, median: 49, q1: 38, q3: 60 }] },
          { label: 'never', n: 80, continuous: [{ variable: 'age', n: 80, missing: 0, mean: 50, sd: 12, median: 50, q1: 40, q3: 61 }] },
        ],
        continuous_tests: [{
          variable: 'age', status: 'computed', method: 'one_way_anova',
          statistic: 0.77, degrees_of_freedom: 2, degrees_of_freedom_denominator: 237,
          p_value: 0.466, groups: ['current', 'never'], group_ns: [80, 80], degenerate: false, reason: null,
        }],
      },
      risk_signals: [],
      analysis: { algorithm_id: 'tableone' } as SkillResult['analysis'],
    };
    render(<ThreeLineTable skillResult={tableOneWithAnova} />);
    // 走 tableone 分支，不是方差分析表
    expect(screen.getByRole('table', { name: 'Table One 三线表' })).toBeInTheDocument();
    expect(screen.queryByRole('table', { name: '单因素方差分析表' })).not.toBeInTheDocument();
  });
});

/**
 * 真机验收（chrome-acceptance-report.json）把 t 检验记为 PASS_WITH_UI_GAP：
 * 「Report omitted t, df, mean difference, and confidence interval」。
 * 引擎数值一直是对的，缺的是渲染分支。
 */
describe('ThreeLineTable two-sample t-test payload', () => {
  const ttest: SkillResult = {
    schema_version: '1.0',
    payload: {
      method: 'Welch two-sample t-test',
      group_variable: 'sex',
      test_variable: 'age',
      groups: [
        { label: 'female', n: 151, mean: 47.96026490066225 },
        { label: 'male', n: 89, mean: 53.08988764044944 },
      ],
      mean_diff: -5.129622739787193,
      t_statistic: -3.0530510730860057,
      df: 180.6821,
      p_value: 0.0025861956291723435,
      ci_lower: -8.444,
      ci_upper: -1.815,
      alpha: 0.05,
    },
    risk_signals: [],
    analysis: { algorithm_id: 'ttest' } as SkillResult['analysis'],
  };

  it('renders the t-test statistics table instead of an empty shell', () => {
    render(<ThreeLineTable skillResult={ttest} />);
    expect(screen.getByRole('table', { name: 't 检验结果表' })).toBeInTheDocument();
  });

  it('shows t, df, mean difference and the confidence interval', () => {
    render(<ThreeLineTable skillResult={ttest} />);
    expect(screen.getByText('-5.130')).toBeInTheDocument();
    expect(screen.getByText('-3.053')).toBeInTheDocument();
    expect(screen.getByText('180.682')).toBeInTheDocument();
    expect(screen.getByText('[-8.444, -1.815]')).toBeInTheDocument();
    expect(screen.getByText('0.003')).toBeInTheDocument();
  });

  it('reports each group mean with its own n', () => {
    render(<ThreeLineTable skillResult={ttest} />);
    expect(screen.getByText('female')).toBeInTheDocument();
    expect(screen.getByText('47.960')).toBeInTheDocument();
    expect(screen.getByText('（n=151）')).toBeInTheDocument();
    expect(screen.getByText('male')).toBeInTheDocument();
    expect(screen.getByText('53.090')).toBeInTheDocument();
    expect(screen.getByText('（n=89）')).toBeInTheDocument();
  });

  it('labels the interval by the alpha actually used', () => {
    render(<ThreeLineTable skillResult={{ ...ttest, payload: { ...ttest.payload as object, alpha: 0.01 } }} />);
    expect(screen.getByText('99% 置信区间')).toBeInTheDocument();
  });
});

/**
 * 真机验收同样把相关分析记为 PASS_WITH_UI_GAP：
 * 「Report omitted r and CI and rendered no chart」。
 */
describe('ThreeLineTable correlation payload', () => {
  const pearson: SkillResult = {
    schema_version: '1.0',
    payload: {
      method: 'pearson',
      x: 'x',
      y: 'y',
      n: 36,
      r: 0.9999968729369666,
      t_statistic: 842.5,
      df: 34,
      p_value: 4.654040852734e-90,
      ci_lower: 0.9999938129117611,
      ci_upper: 0.9999984195286316,
      alpha: 0.05,
    },
    risk_signals: [],
    analysis: { algorithm_id: 'correlation' } as SkillResult['analysis'],
  };

  it('renders the coefficient, its interval and the test statistics', () => {
    render(<ThreeLineTable skillResult={pearson} />);
    expect(screen.getByRole('table', { name: '相关分析结果表' })).toBeInTheDocument();
    expect(screen.getByText('Pearson r')).toBeInTheDocument();
    // r 与 R² 在这个近乎完美相关的数据上都四舍五入到 1.000，按行定位避免歧义。
    const coefficientRow = screen.getByRole('row', { name: /Pearson r/ });
    expect(within(coefficientRow).getByText('1.000')).toBeInTheDocument();
    expect(screen.getByText('[1.000, 1.000]')).toBeInTheDocument();
    expect(screen.getByText('842.500')).toBeInTheDocument();
    expect(screen.getByText('<0.001')).toBeInTheDocument();
    expect(screen.getByText('36')).toBeInTheDocument();
  });

  it('labels Spearman as rho and withholds R² for a rank coefficient', () => {
    render(
      <ThreeLineTable
        skillResult={{ ...pearson, payload: { ...pearson.payload as object, method: 'spearman', r: 0.9931758706783417 } }}
      />,
    );
    expect(screen.getByText('Spearman ρ')).toBeInTheDocument();
    expect(screen.queryByText('决定系数 R²')).not.toBeInTheDocument();
  });

  it('reports R² for Pearson, where proportion-of-variance is meaningful', () => {
    render(<ThreeLineTable skillResult={{ ...pearson, payload: { ...pearson.payload as object, r: 0.6 } }} />);
    expect(screen.getByText('决定系数 R²')).toBeInTheDocument();
    expect(screen.getByText('0.360')).toBeInTheDocument();
  });
});

/**
 * 真机验收把功效分析记为 UI_FAIL_ENGINE_PASS：引擎算出每组 64、合计 128，
 * 界面却「no sample-size result」。power 不进算法覆盖矩阵，analysis 可能
 * 没有 algorithm_id，所以判据落在载荷形状上。
 */
describe('ThreeLineTable power / sample-size payload', () => {
  const power: SkillResult = {
    schema_version: '1.0',
    payload: {
      required_n: 64,
      achieved_power: 0.8014,
      effect_size: 0.5,
      alpha: 0.05,
      method: 'two_means',
      converged: true,
    },
    risk_signals: [],
  };

  it('renders per-group and total sample size without any dataset', () => {
    render(<ThreeLineTable skillResult={power} />);
    expect(screen.getByRole('table', { name: '功效与样本量结果表' })).toBeInTheDocument();
    expect(screen.getByText('每组样本量')).toBeInTheDocument();
    expect(screen.getByText('64')).toBeInTheDocument();
    expect(screen.getByText('两组合计')).toBeInTheDocument();
    expect(screen.getByText('128')).toBeInTheDocument();
    expect(screen.getByText('0.801')).toBeInTheDocument();
  });

  it('warns when the bounded search did not converge', () => {
    render(<ThreeLineTable skillResult={{ ...power, payload: { ...power.payload as object, converged: false } }} />);
    expect(screen.getByText(/迭代未收敛/)).toBeInTheDocument();
  });
});

/**
 * Tests for ReportViewer.
 *
 * Validates: Requirements 6.1, 6.2, 6.4
 */

import { describe, it, expect, vi } from 'vitest';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { ReportViewer } from './ReportViewer';
import type { ChatMessage } from '../../hooks/useSseChat';
import type { DatasetSummary, SkillResult } from '../../api/types';

vi.mock('../../components/StatsChartRenderer', () => ({
  StatsChartRenderer: ({ skillResult }: { skillResult: SkillResult | null }) => (
    <div data-testid="stats-chart">
      <span>统计图表</span>
      <span>{skillResult?.analysis?.run_id ?? 'no-run'}</span>
    </div>
  ),
}));

const skillResult: SkillResult = {
  schema_version: '1.0',
  payload: {
    coefficients: [
      { term: 'age', beta: 0.12, standard_error: 0.03, ci_lower: 0.06, ci_upper: 0.18, p_value: 0.001 },
    ],
  },
  risk_signals: ['VifTooHigh'],
};

function agentMessage(overrides: Partial<ChatMessage> = {}): ChatMessage {
  return {
    id: 'a1',
    role: 'agent',
    content: '分析完成',
    timestamp: new Date(),
    ...overrides,
  };
}

const dataset: DatasetSummary = {
  dataset_id: 'ds-1',
  file_name: 'cohort.csv',
  size_bytes: 1024,
  encoding: 'Utf8',
  row_count: 100,
  columns: [{ name: 'age', inferred_type: 'Numeric', missing_count: 0 }],
  uploaded_at: '2026-01-01T00:00:00Z',
  sha256: null,
};

function correlationResult(runId: string, r: number): SkillResult {
  return {
    schema_version: '1.0',
    payload: {
      method: 'pearson',
      x: 'x',
      y: 'y',
      n: 36,
      r,
      t_statistic: 4.37,
      df: 34,
      p_value: 0.0001,
      ci_lower: r - 0.1,
      ci_upper: r + 0.1,
      alpha: 0.05,
    },
    risk_signals: [],
    analysis: {
      algorithm_id: 'correlation',
      dataset_id: 'ds-1',
      dataset_sha256: null,
      columns: [],
      params: {},
      run_id: runId,
      run_status: 'completed',
    },
  };
}

describe('ReportViewer (Requirements 6.1, 6.2, 6.4)', () => {
  it('renders the report table and risk-signal tags for the latest result (R6.1, R6.4)', () => {
    render(<ReportViewer messages={[agentMessage({ skillResult })]} selectedDataset={null} />);
    expect(screen.getByText('分析报告结果')).toBeInTheDocument();
    expect(screen.getByText('效应量')).toBeInTheDocument();
    expect(screen.getByText('P 值')).toBeInTheDocument();
    // Regression coefficient table renders the term.
    expect(screen.getByText('age')).toBeInTheDocument();
    // Risk-signal tag.
    expect(screen.getByText('VIF > 10')).toBeInTheDocument();
  });

  it('renders the AI interpretation card when present (R6.2)', () => {
    render(
      <ReportViewer
        messages={[agentMessage({ skillResult, interpretation: '该系数具有统计学意义' })]}
        selectedDataset={null}
      />,
    );
    expect(screen.getByText('方法学提示')).toBeInTheDocument();
    expect(screen.getByText('该系数具有统计学意义')).toBeInTheDocument();
  });

  it('keeps a pinned historical result and its owning message together', () => {
    const older = correlationResult('old-run', 0.2);
    const newer = correlationResult('new-run', 0.8);
    const olderMessage = {
      ...agentMessage({
        id: 'older',
        content: '旧运行关键结论',
        interpretation: '旧运行方法提示',
      }),
      skillResult: older,
    };
    const newerMessage = {
      ...agentMessage({
        id: 'newer',
        content: '新运行关键结论',
        interpretation: '新运行方法提示',
      }),
      skillResult: newer,
    };

    render(
      <ReportViewer
        messages={[olderMessage, newerMessage]}
        selectedDataset={null}
        pinnedArtifact={{ resultMessage: olderMessage }}
      />,
    );

    expect(screen.getAllByText('0.200').length).toBeGreaterThan(0);
    expect(screen.queryByText('0.800')).not.toBeInTheDocument();
    expect(screen.getByText('旧运行关键结论')).toBeInTheDocument();
    expect(screen.getByText('旧运行方法提示')).toBeInTheDocument();
    expect(screen.queryByText('新运行关键结论')).not.toBeInTheDocument();
    expect(screen.queryByText('新运行方法提示')).not.toBeInTheDocument();
  });

  it('uses the latest result and owning message when no artifact is pinned', () => {
    const older = correlationResult('old-run', 0.2);
    const newer = correlationResult('new-run', 0.8);
    const olderMessage = agentMessage({ id: 'older', content: '旧运行关键结论', skillResult: older });
    const newerMessage = agentMessage({ id: 'newer', content: '新运行关键结论', skillResult: newer });

    render(<ReportViewer messages={[olderMessage, newerMessage]} selectedDataset={null} />);

    expect(screen.getAllByText('0.800').length).toBeGreaterThan(0);
    expect(screen.queryByText('0.200')).not.toBeInTheDocument();
    expect(screen.getByText('新运行关键结论')).toBeInTheDocument();
    expect(screen.queryByText('旧运行关键结论')).not.toBeInTheDocument();
  });

  it('passes the pinned run to the chart view', () => {
    const older = correlationResult('old-run', 0.2);
    const newer = correlationResult('new-run', 0.8);
    const olderMessage = { ...agentMessage({ id: 'older' }), skillResult: older };
    const newerMessage = { ...agentMessage({ id: 'newer' }), skillResult: newer };

    render(
      <ReportViewer
        messages={[olderMessage, newerMessage]}
        selectedDataset={null}
        activeView="chart"
        pinnedArtifact={{ resultMessage: olderMessage }}
      />,
    );

    expect(screen.getByTestId('stats-chart')).toHaveTextContent('old-run');
    expect(screen.getByTestId('stats-chart')).not.toHaveTextContent('new-run');
  });

  it('shows the data explorer when there is no result but a dataset is selected', () => {
    const { container } = render(<ReportViewer messages={[]} selectedDataset={dataset} />);
    expect(container.querySelectorAll('.data-explorer-details > .ant-col-lg-24')).toHaveLength(2);
    expect(container.querySelectorAll('.data-explorer-details .ant-table-scroll-horizontal')).toHaveLength(1);
    expect(screen.getByText('未缓存原始行')).toBeInTheDocument();
    expect(screen.queryByText('智能预渲染模式')).not.toBeInTheDocument();
    expect(screen.getByText(/数据集已装载/)).toBeInTheDocument();
  });

  it('shows the empty state when there is neither a result nor a selected dataset', () => {
    render(<ReportViewer messages={[]} selectedDataset={null} />);
    expect(screen.getByText(/暂无分析结果/)).toBeInTheDocument();
  });

  it('keeps report and chart as focused artifact views instead of stacking both', () => {
    render(
      <ReportViewer
        messages={[agentMessage({ skillResult })]}
        selectedDataset={dataset}
        activeView="chart"
      />,
    );
    expect(screen.getByText('统计图表')).toBeInTheDocument();
    expect(screen.queryByText('分析报告结果')).not.toBeInTheDocument();
  });

  it('uses the substantive predictor rather than the intercept for headline metrics', () => {
    const resultWithIntercept: SkillResult = {
      ...skillResult,
      payload: {
        coefficients: [
          { term: 'β0', beta: 26.754, standard_error: 0.896, ci_lower: 24.989, ci_upper: 28.519, p_value: 0.0001 },
          { term: 'age', beta: 0.003, standard_error: 0.017, ci_lower: -0.031, ci_upper: 0.037, p_value: 0.864 },
        ],
      },
    };
    render(<ReportViewer messages={[agentMessage({ skillResult: resultWithIntercept })]} selectedDataset={dataset} />);
    const metrics = screen.getByLabelText('关键统计量');
    expect(within(metrics).getByText('0.003')).toBeInTheDocument();
    expect(within(metrics).getByText('0.864')).toBeInTheDocument();
    expect(within(metrics).getByText('[-0.031, 0.037]')).toBeInTheDocument();
  });

  it('renders per-group event/censor counts and the log-rank result', () => {
    const survivalResult: SkillResult = {
      schema_version: '1.0',
      payload: {
        groups: ['A', 'B'],
        steps: [
          { group: 'A', time: 1, survival: 0.8 },
          { group: 'B', time: 3, survival: 0.75 },
        ],
        group_summaries: [
          { group: 'A', n: 5, event_n: 3, censored_n: 2, median_survival: 4 },
          { group: 'B', n: 5, event_n: 2, censored_n: 3, median_survival: 6 },
        ],
        log_rank: { status: 'computed', method: 'log_rank', statistic: 1.533379867, degrees_of_freedom: 1, p_value: 0.215605895 },
      },
      risk_signals: [],
    };

    render(<ReportViewer messages={[agentMessage({ skillResult: survivalResult })]} selectedDataset={null} />);
    const summary = screen.getByLabelText('生存分析分组摘要');
    expect(within(summary).getByText('n=5 · 事件=3 · 删失=2')).toBeInTheDocument();
    expect(within(summary).getByText('n=5 · 事件=2 · 删失=3')).toBeInTheDocument();
    expect(within(summary).getByText('χ²=1.533 · df=1 · p=0.216')).toBeInTheDocument();
  });

  it('renders the standardized result contract with counts, diagnostics and provenance', () => {
    const resultWithContract: SkillResult = {
      ...skillResult,
      analysis: {
        algorithm_id: 'linear',
        dataset_id: dataset.dataset_id,
        dataset_sha256: null,
        columns: dataset.columns,
        params: { outcome: 'bmi', predictors: ['age', 'disease'] },
        run_id: 'run-contract',
        run_status: 'completed',
        result_contract: {
          schema_version: '1.0',
          method: { algorithm_id: 'linear', method_version: '1.0' },
          estimates: [{
            term: 'age',
            estimate: 0.12,
            ci_95: { lower: 0.06, upper: 0.18 },
            p_value: 0.001,
            effect_unit: 'Beta',
            adjustment: 'adjusted',
          }],
          counts: {
            input_n: 100,
            complete_case_n: 98,
            missing_n: 2,
            event_n: null,
            person_time: null,
          },
          analysis_availability: { unadjusted: 'not_computed', adjusted: 'available' },
          effect_unit: 'Beta',
          convergence: { status: 'not_applicable' },
          assumption_diagnostics: [{
            code: 'linear-residuals',
            status: 'not_evaluated',
            message: '残差正态性与同方差性未在当前运行中自动诊断。',
          }],
          exclusions: [{ reason: '完整案例损失', n: 2 }],
          interpretation: {
            statistical: null,
            practical_significance: null,
            unsupported_conclusions: ['不能仅凭观察性统计关联作出因果结论。'],
          },
          provenance: {
            engine_name: '@stats-code/engine',
            engine_version: '0.5.0',
            validation_coverage: { R: 'live', SAS: 'recorded' },
          },
        },
      },
    };

    render(<ReportViewer messages={[agentMessage({ skillResult: resultWithContract })]} selectedDataset={dataset} />);

    // 审计材料默认折叠（表格才是交付物，先呈现表格），但内容一条不少：
    // 展开后下面每一项断言都仍须成立。
    fireEvent.click(screen.getByText(/结果合同 v1\.0 与审计材料/));

    const contract = screen.getByLabelText('标准化结果合同');
    expect(within(contract).getByText('结果合同 v1.0')).toBeInTheDocument();
    expect(within(contract).getByText('98 / 100')).toBeInTheDocument();
    expect(within(contract).getByText('2')).toBeInTheDocument();
    expect(within(contract).getByText('不适用')).toBeInTheDocument();
    expect(within(contract).getByText('@stats-code/engine 0.5.0')).toBeInTheDocument();
    expect(within(contract).getByText('R: live')).toBeInTheDocument();
    expect(within(contract).getByText('残差正态性与同方差性未在当前运行中自动诊断。')).toBeInTheDocument();
    expect(within(contract).getByText('不能仅凭观察性统计关联作出因果结论。')).toBeInTheDocument();
  });

  /**
   * 扁平载荷（t 检验 / 相关 / 功效）没有 coefficients，指标卡此前整块空白，
   * 表格外壳也不渲染——真机验收记为「报告不完整」。
   */
  it('surfaces mean difference and interval for a t-test instead of only p and n', () => {
    const ttest: SkillResult = {
      schema_version: '1.0',
      payload: {
        method: 'Welch two-sample t-test',
        group_variable: 'sex',
        test_variable: 'age',
        groups: [
          { label: 'female', n: 151, mean: 47.96 },
          { label: 'male', n: 89, mean: 53.09 },
        ],
        mean_diff: -5.1296,
        t_statistic: -3.0531,
        df: 180.68,
        p_value: 0.00258,
        ci_lower: -8.444,
        ci_upper: -1.815,
        alpha: 0.05,
      },
      risk_signals: [],
      analysis: { algorithm_id: 'ttest' } as SkillResult['analysis'],
    };

    render(<ReportViewer messages={[agentMessage({ skillResult: ttest })]} selectedDataset={null} />);
    const metrics = screen.getByLabelText('关键统计量');
    expect(within(metrics).getByText('-5.130')).toBeInTheDocument();
    expect(within(metrics).getByText('[-8.444, -1.815]')).toBeInTheDocument();
    expect(within(metrics).getByText(/均值差/)).toBeInTheDocument();
    expect(screen.getByRole('table', { name: 't 检验结果表' })).toBeInTheDocument();
  });

  it('surfaces the correlation coefficient and its interval', () => {
    const correlation: SkillResult = {
      schema_version: '1.0',
      payload: {
        method: 'pearson',
        x: 'x',
        y: 'y',
        n: 36,
        r: 0.6,
        t_statistic: 4.37,
        df: 34,
        p_value: 0.0001,
        ci_lower: 0.34,
        ci_upper: 0.78,
        alpha: 0.05,
      },
      risk_signals: [],
      analysis: { algorithm_id: 'correlation' } as SkillResult['analysis'],
    };

    render(<ReportViewer messages={[agentMessage({ skillResult: correlation })]} selectedDataset={null} />);
    const metrics = screen.getByLabelText('关键统计量');
    expect(within(metrics).getByText('0.600')).toBeInTheDocument();
    expect(within(metrics).getByText('[0.340, 0.780]')).toBeInTheDocument();
    expect(within(metrics).getByText(/Pearson r/)).toBeInTheDocument();
    expect(screen.getByRole('table', { name: '相关分析结果表' })).toBeInTheDocument();
  });

  it('reports the required sample size for a design-phase power run, not the dataset row count', () => {
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

    // 传入 100 行的数据集：功效分析不读数据，卡片不得显示 100。
    render(<ReportViewer messages={[agentMessage({ skillResult: power })]} selectedDataset={dataset} />);
    const metrics = screen.getByLabelText('关键统计量');
    expect(within(metrics).getByText('每组样本量')).toBeInTheDocument();
    expect(within(metrics).getByText('64')).toBeInTheDocument();
    expect(within(metrics).queryByText('100')).not.toBeInTheDocument();
    expect(screen.getByRole('table', { name: '功效与样本量结果表' })).toBeInTheDocument();
  });
});

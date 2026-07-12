/**
 * Tests for ReportViewer.
 *
 * Validates: Requirements 6.1, 6.2, 6.4
 */

import { describe, it, expect } from 'vitest';
import { render, screen, within } from '@testing-library/react';
import { ReportViewer } from './ReportViewer';
import type { ChatMessage } from '../../hooks/useSseChat';
import type { DatasetSummary, SkillResult } from '../../api/types';

const skillResult: SkillResult = {
  schema_version: '1.0',
  payload: {
    coefficients: [
      { term: 'age', beta: 0.12, standard_error: 0.03, ci_lower: 0.06, ci_upper: 0.18, p_value: 0.001 },
    ],
  },
  risk_signals: ['PValueAboveAlpha'],
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

describe('ReportViewer (Requirements 6.1, 6.2, 6.4)', () => {
  it('renders the report table and risk-signal tags for the latest result (R6.1, R6.4)', () => {
    render(<ReportViewer messages={[agentMessage({ skillResult })]} selectedDataset={null} />);
    expect(screen.getByText('分析报告结果')).toBeInTheDocument();
    expect(screen.getByText('效应量')).toBeInTheDocument();
    expect(screen.getByText('P 值')).toBeInTheDocument();
    // Regression coefficient table renders the term.
    expect(screen.getByText('age')).toBeInTheDocument();
    // Risk-signal tag.
    expect(screen.getByText('P > 0.05')).toBeInTheDocument();
  });

  it('renders the AI interpretation card when present (R6.2)', () => {
    render(
      <ReportViewer
        messages={[agentMessage({ skillResult, interpretation: '该系数具有统计学意义' })]}
        selectedDataset={null}
      />,
    );
    expect(screen.getByText('AI 统计解读')).toBeInTheDocument();
    expect(screen.getByText('该系数具有统计学意义')).toBeInTheDocument();
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
});

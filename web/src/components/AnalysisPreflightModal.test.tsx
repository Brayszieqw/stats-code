import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { DatasetSummary, RunRequest } from '../api/types';
import { AnalysisPreflightModal } from './AnalysisPreflightModal';

const dataset: DatasetSummary = {
  dataset_id: 'ds-1',
  file_name: 'cohort.csv',
  size_bytes: 1024,
  encoding: 'Utf8',
  row_count: 24,
  columns: [
    { name: 'outcome', inferred_type: 'Numeric', missing_count: 0 },
    { name: 'age', inferred_type: 'Numeric', missing_count: 5 },
  ],
  uploaded_at: '2026-01-01T00:00:00Z',
};

const request: RunRequest = {
  skill_id: 'model_linear',
  dataset_id: dataset.dataset_id,
  args: { outcome: 'outcome', predictors: ['age'] },
};

describe('AnalysisPreflightModal', () => {
  it('shows the exact run summary, risk warnings and trust statement', async () => {
    render(
      <AnalysisPreflightModal
        open
        dataset={dataset}
        request={request}
        promptText="分析 outcome 与 age 是否存在因果关系"
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />,
    );

    expect(await screen.findByRole('dialog', { name: '执行前确认' })).toBeInTheDocument();
    expect(await screen.findByText('多元线性回归')).toBeInTheDocument();
    expect(screen.getByText('cohort.csv')).toBeInTheDocument();
    expect(screen.getByText('n = 24')).toBeInTheDocument();
    expect(screen.getByText(/age · 5 例 · 20\.8%/)).toBeInTheDocument();
    expect(screen.getByText(/观察性统计关联不能直接证明因果关系/)).toBeInTheDocument();
    expect(screen.getByText('本机确定性引擎 · 数值非 LLM 生成 · 可审计')).toBeInTheDocument();
  });

  it('keeps confirm and cancel as separate explicit actions', async () => {
    const onConfirm = vi.fn();
    const onCancel = vi.fn();
    render(
      <AnalysisPreflightModal
        open
        dataset={dataset}
        request={request}
        promptText="分析 outcome 与 age"
        onConfirm={onConfirm}
        onCancel={onCancel}
      />,
    );

    expect(await screen.findByRole('dialog', { name: '执行前确认' })).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: /取\s*消/ }));
    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(onConfirm).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole('button', { name: '确认并运行' }));
    expect(onConfirm).toHaveBeenCalledTimes(1);
  });
});


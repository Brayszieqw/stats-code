import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { DatasetSummary } from '../api/types';
import { AnalysisConfigurator } from './AnalysisConfigurator';

const summary: DatasetSummary = {
  dataset_id: 'dataset-1',
  file_name: 'demo_cohort.csv',
  size_bytes: 100,
  encoding: 'Utf8',
  row_count: 240,
  uploaded_at: '2026-07-12T00:00:00Z',
  sha256: 'abc',
  columns: [
    { name: 'disease', inferred_type: 'Numeric', missing_count: 0 },
    { name: 'age', inferred_type: 'Numeric', missing_count: 0 },
    { name: 'sex', inferred_type: 'String', missing_count: 0 },
  ],
};

describe('AnalysisConfigurator', () => {
  it('uses a two-column module grid with power analysis spanning the last row', async () => {
    render(<AnalysisConfigurator summary={summary} onSubmit={vi.fn()} />);

    const power = await screen.findByRole('radio', { name: /功效分析/ });
    const group = power.closest('.ant-radio-group');
    const button = power.closest('.ant-radio-button-wrapper');

    expect(group).toHaveStyle({
      display: 'grid',
      gridTemplateColumns: 'repeat(2, minmax(0, 1fr))',
    });
    expect(group?.querySelectorAll('input[type="radio"]')).toHaveLength(7);
    expect(button).toHaveStyle({ gridColumn: '1 / -1' });
  });

  it(
    'allows numeric binary fields to be selected as grouping variables',
    async () => {
      render(<AnalysisConfigurator summary={summary} onSubmit={vi.fn()} />);

      const groupField = await screen.findByRole('combobox', { name: /分组比较变量/ });
      fireEvent.mouseDown(groupField);

      // 类型标签是中文全角括号（displayLabels.columnTypeLabel），
      // 界面上不再出现 `(Numeric)` 这种裸英文枚举值。
      expect(await screen.findByText('disease（数值）')).toBeInTheDocument();
    },
    15_000,
  );

  it('exposes ANOVA and correlation analysis modules', async () => {
    render(<AnalysisConfigurator summary={summary} onSubmit={vi.fn()} />);

    expect(await screen.findByRole('radio', { name: /方差分析/ })).toBeInTheDocument();
    expect(screen.getByRole('radio', { name: /相关分析/ })).toBeInTheDocument();
  });

  it('shows ANOVA fields when the module is selected', async () => {
    render(<AnalysisConfigurator summary={summary} onSubmit={vi.fn()} />);
    fireEvent.click(await screen.findByRole('radio', { name: /方差分析/ }));
    expect(await screen.findByRole('combobox', { name: /分组自变量/ })).toBeInTheDocument();
    expect(screen.getByRole('combobox', { name: /连续因变量/ })).toBeInTheDocument();
  });

  it('shows correlation fields when the module is selected', async () => {
    render(<AnalysisConfigurator summary={summary} onSubmit={vi.fn()} />);
    fireEvent.click(await screen.findByRole('radio', { name: /相关分析/ }));
    expect(await screen.findByRole('combobox', { name: /变量 X/ })).toBeInTheDocument();
    expect(screen.getByRole('combobox', { name: /变量 Y/ })).toBeInTheDocument();
    expect(screen.getByRole('radio', { name: /Pearson/ })).toBeInTheDocument();
    expect(screen.getByRole('radio', { name: /Spearman/ })).toBeInTheDocument();
  });

  it('exposes a power analysis module', async () => {
    render(<AnalysisConfigurator summary={summary} onSubmit={vi.fn()} />);
    expect(await screen.findByRole('radio', { name: /功效分析/ })).toBeInTheDocument();
  });

  it('shows scalar-only fields for power analysis and no column pickers', async () => {
    render(<AnalysisConfigurator summary={summary} onSubmit={vi.fn()} />);
    fireEvent.click(await screen.findByRole('radio', { name: /功效分析/ }));

    expect(await screen.findByRole('combobox', { name: /检验类型/ })).toBeInTheDocument();
    // 功效分析是设计阶段工具：不应出现任何引用数据集列的选择器
    expect(screen.queryByRole('combobox', { name: /分组比较变量/ })).not.toBeInTheDocument();
    expect(screen.queryByRole('combobox', { name: /连续性数值变量/ })).not.toBeInTheDocument();
    expect(screen.getByText(/不读取数据集内容/)).toBeInTheDocument();
  });

  it('submits power analysis with the scalar contract the server expects', async () => {
    const onSubmit = vi.fn();
    render(<AnalysisConfigurator summary={summary} onSubmit={onSubmit} />);
    fireEvent.click(await screen.findByRole('radio', { name: /功效分析/ }));

    const effect = await screen.findByLabelText(/预期效应量/);
    fireEvent.change(effect, { target: { value: '0.5' } });
    fireEvent.click(screen.getByRole('button', { name: /开始统计计算/ }));

    await vi.waitFor(() => expect(onSubmit).toHaveBeenCalled());
    const [request] = onSubmit.mock.calls[0]!;
    expect(request.skill_id).toBe('power');
    // 服务端 power 分支只读这四个标量；alpha/power 走表单默认值
    expect(request.args).toEqual({
      test_type: 'ttest',
      effect_size: 0.5,
      alpha: 0.05,
      power: 0.8,
    });
  });

  it('rejects a non-positive effect size before submitting', async () => {
    const onSubmit = vi.fn();
    render(<AnalysisConfigurator summary={summary} onSubmit={onSubmit} />);
    fireEvent.click(await screen.findByRole('radio', { name: /功效分析/ }));

    fireEvent.change(await screen.findByLabelText(/预期效应量/), { target: { value: '0' } });
    fireEvent.click(screen.getByRole('button', { name: /开始统计计算/ }));

    expect(await screen.findByText('效应量必须为正数')).toBeInTheDocument();
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it('labels column types in Chinese without leaking the raw enum value', async () => {
    render(<AnalysisConfigurator summary={summary} onSubmit={vi.fn()} />);
    fireEvent.mouseDown(await screen.findByRole('combobox', { name: /分组比较变量/ }));

    expect(await screen.findByText('sex（文本）')).toBeInTheDocument();
    expect(screen.queryByText(/\(String\)/)).not.toBeInTheDocument();
    expect(screen.queryByText(/\(Numeric\)/)).not.toBeInTheDocument();
  });
});


import { render } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { SkillResult } from '../api/types';
import { StatsChartRenderer } from './StatsChartRenderer';

const chartCapture = vi.hoisted(() => ({ option: null as any }));

vi.mock('echarts-for-react', () => ({
  default: ({ option }: { option: unknown }) => {
    chartCapture.option = option;
    return <div data-testid="stats-chart" />;
  },
}));

function result(payload: unknown): SkillResult {
  return { schema_version: '1.0', payload, risk_signals: [] };
}

function renderCustomSeries(payload: unknown, row: number[]) {
  chartCapture.option = null;
  render(<StatsChartRenderer skillResult={result(payload)} />);

  const customSeries = chartCapture.option.series.find((series: any) => series.type === 'custom');
  const api = {
    value: vi.fn((index: number) => row[index]),
    coord: vi.fn((point: number[]) => [point[0]! * 10, point[1]! * 10]),
  };

  expect(() => customSeries.renderItem({}, api)).not.toThrow();
  expect(api.coord).toHaveBeenCalledTimes(2);
}

describe('StatsChartRenderer custom error bars', () => {
  it('uses the ECharts coord API for regression confidence intervals', () => {
    renderCustomSeries(
      {
        coefficients: [
          { term: 'age', beta: 0.12, ci_lower: 0.06, ci_upper: 0.18, p_value: 0.001 },
        ],
      },
      [0.12, 0, 0.06, 0.18],
    );
  });

  it('uses the ECharts coord API for ANOVA error bars', () => {
    renderCustomSeries(
      {
        variable: 'age',
        overall_mean: 50,
        p_value: 0.03,
        groups: [{ group: 'control', mean: 48, sd: 4 }],
      },
      [0, 44, 52],
    );
  });
});

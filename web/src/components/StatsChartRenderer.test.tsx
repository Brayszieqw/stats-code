import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { StatsChartRenderer } from './StatsChartRenderer';
import type { SkillResult } from '../api/types';

vi.mock('echarts-for-react', () => ({
  default: ({ option }: { option: { title?: { text?: string; subtext?: string }; series?: Array<{ name?: string; data?: unknown[] }> } }) => (
    <div data-testid="echarts-option">
      {option.title?.text} | {option.title?.subtext} |
      {option.series?.map((series) => `${series.name}:${series.data?.length ?? 0}`).join(',')}
    </div>
  ),
}));

describe('StatsChartRenderer Kaplan-Meier', () => {
  it('renders separate step curves and a finite log-rank p-value', () => {
    const result: SkillResult = {
      schema_version: '1.0',
      payload: {
        groups: ['A', 'B'],
        steps: [
          { group: 'A', time: 1, survival: 0.8 },
          { group: 'A', time: 4, survival: 0.3 },
          { group: 'B', time: 3, survival: 0.75 },
        ],
        log_rank: { status: 'computed', statistic: 1.533, degrees_of_freedom: 1, p_value: 0.215605895 },
      },
      risk_signals: [],
    };

    render(<StatsChartRenderer skillResult={result} />);
    expect(screen.getByTestId('echarts-option')).toHaveTextContent('Kaplan-Meier 生存曲线');
    expect(screen.getByTestId('echarts-option')).toHaveTextContent('Log-rank p: 0.2156');
    expect(screen.getByTestId('echarts-option')).toHaveTextContent('A:3,B:2');
  });

  it('does not display a false significant p-value when log-rank is unavailable', () => {
    const result: SkillResult = {
      schema_version: '1.0',
      payload: {
        groups: ['A', 'B'],
        steps: [{ group: 'A', time: 1, survival: 1 }],
        log_rank: { status: 'not_computed', p_value: null, reason: 'no_events' },
      },
      risk_signals: [],
    };

    render(<StatsChartRenderer skillResult={result} />);
    expect(screen.getByTestId('echarts-option')).not.toHaveTextContent('Log-rank p:');
  });
});

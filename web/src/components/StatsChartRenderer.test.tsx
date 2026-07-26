import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { StatsChartRenderer } from './StatsChartRenderer';
import type { SkillResult } from '../api/types';

type CapturedOption = {
  title?: { text?: string; subtext?: string };
  tooltip?: { formatter?: (params: any) => string };
  // Gauge series carry `{ value }` entries; other series carry plain numbers or
  // coordinate tuples.
  series?: Array<{
    name?: string;
    type?: string;
    data?: Array<{ value?: number } | number | number[] | null>;
  }>;
};

const { capturedOptions } = vi.hoisted(() => ({ capturedOptions: [] as CapturedOption[] }));

vi.mock('echarts-for-react', () => ({
  default: ({ option }: { option: CapturedOption }) => {
    capturedOptions.push(option);
    return (
    <div data-testid="echarts-option">
      {option.title?.text} | {option.title?.subtext} |
      {option.series?.map((series) => `${series.name}:${series.data?.length ?? 0}`).join(',')}
    </div>
    );
  },
}));

beforeEach(() => {
  capturedOptions.length = 0;
});

describe('StatsChartRenderer unsupported chart state', () => {
  it('shows an explicit empty state for a Table One result without chart data', () => {
    const result: SkillResult = {
      schema_version: '1.0',
      payload: {
        strata: 'disease',
        continuous: ['age', 'bmi'],
        categorical: ['sex'],
        groups: [
          {
            label: '0',
            n: 151,
            continuous: [],
            categorical: [],
          },
          {
            label: '1',
            n: 89,
            continuous: [],
            categorical: [],
          },
        ],
      },
      risk_signals: [],
    };

    render(<StatsChartRenderer skillResult={result} />);

    expect(screen.getByText('当前分析暂无可用图表')).toBeInTheDocument();
    expect(screen.queryByTestId('echarts-option')).not.toBeInTheDocument();
  });
});

function latestOption(): CapturedOption {
  const option = capturedOptions[capturedOptions.length - 1];
  if (!option) throw new Error('ECharts option was not captured.');
  return option;
}

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

describe('StatsChartRenderer tooltip escaping', () => {
  const malicious = '<img src=x onerror="alert(1)">';
  const escaped = '&lt;img src=x onerror=&quot;alert(1)&quot;&gt;';

  it('escapes Kaplan-Meier series names while preserving tooltip markup', () => {
    const result: SkillResult = {
      schema_version: '1.0',
      payload: {
        groups: [malicious],
        steps: [{ group: malicious, time: 1, survival: 0.5 }],
      },
      risk_signals: [],
    };
    render(<StatsChartRenderer skillResult={result} />);
    const html = latestOption().tooltip?.formatter?.([
      { marker: '<span></span>', seriesName: malicious, value: [1, 0.5] },
    ]);
    expect(html).toContain(escaped);
    expect(html).not.toContain(malicious);
    expect(html).toContain('<br/>');
  });

  it('escapes regression terms in the forest-plot tooltip', () => {
    const result: SkillResult = {
      schema_version: '1.0',
      payload: {
        coefficients: [{ term: malicious, beta: 1, ciLower: 0.5, ciUpper: 1.5, pValue: 0.04 }],
      },
      risk_signals: [],
    };
    render(<StatsChartRenderer skillResult={result} />);
    const option = latestOption();
    const pointSeries = option.series?.find((series) => series.type === 'scatter');
    const html = option.tooltip?.formatter?.([{ seriesName: pointSeries?.name, dataIndex: 0 }]);
    expect(html).toContain(`<b>${escaped}</b>`);
    expect(html).not.toContain(malicious);
  });

  // Payload shape mirrors the anova skill's actual return (skill-runner.ts):
  // `groups` is a list of label strings and the per-group mean/sd live in
  // `group_stats`. The previous fixture invented an `overall_mean` field and a
  // `groups: [{group, mean, sd}]` shape that no backend path emits, so this
  // test passed against a chart branch that could never render in production.
  it('escapes ANOVA group names in the comparison tooltip', () => {
    const result: SkillResult = {
      schema_version: '1.0',
      payload: {
        method: 'One-way ANOVA',
        test_variable: 'outcome',
        p_value: 0.2,
        f_statistic: 4.2,
        groups: [malicious],
        group_ns: { [malicious]: 10 },
        group_stats: [{ group: malicious, n: 10, mean: 1, sd: 0.1 }],
      },
      risk_signals: [],
    };
    render(<StatsChartRenderer skillResult={result} />);
    const option = latestOption();
    const barSeries = option.series?.find((series) => series.type === 'bar');
    const html = option.tooltip?.formatter?.([{ seriesName: barSeries?.name, dataIndex: 0 }]);
    expect(html).toContain(`<b>${escaped}</b>`);
    expect(html).not.toContain(malicious);
  });

  // Guards the gauge against two past defects: the branch keyed off fields the
  // power skill never emits (so it never rendered), and its value expression
  // `payload.power || 0.8` would have displayed a hard-coded 0.8.
  it('plots the computed achieved power rather than a default', () => {
    const result: SkillResult = {
      schema_version: '1.0',
      payload: {
        method: 'two_means',
        required_n: 64,
        total_n: 128,
        achieved_power: 0.8013,
        effect_size: 0.5,
        alpha: 0.05,
        converged: true,
      },
      risk_signals: [],
    };
    render(<StatsChartRenderer skillResult={result} />);
    const option = latestOption();
    const gauge = option.series?.find((series) => series.type === 'gauge');
    const datum = gauge?.data?.[0];
    expect(datum).toBeTypeOf('object');
    expect((datum as { value?: number }).value).toBeCloseTo(0.8013, 4);
  });
});

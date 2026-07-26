import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { MessageList } from './MessageList';
import type { ChatMessage } from '../hooks/useSseChat';
import type { SkillResult } from '../api/types';

vi.mock('./StatsChartRenderer', () => ({
  StatsChartRenderer: () => null,
}));

const baseMessage = {
  timestamp: new Date('2026-01-01T00:00:00Z'),
};

function userMessage(content: string): ChatMessage {
  return {
    ...baseMessage,
    id: `user-${content}`,
    role: 'user',
    content,
  };
}

function emptyAgentMessage(): ChatMessage {
  return {
    ...baseMessage,
    id: 'agent-empty',
    role: 'agent',
    content: '',
  };
}

describe('MessageList', () => {
  it('does not render an empty agent placeholder bubble', () => {
    const { container } = render(<MessageList messages={[userMessage('hello'), emptyAgentMessage()]} />);

    expect(screen.getByText('hello')).toBeInTheDocument();
    expect(container.querySelector('.anticon-robot')).not.toBeInTheDocument();
  });

  it('renders a compact result reference in the professional workspace', () => {
    const onOpenResult = vi.fn();
    const skillResult: SkillResult = {
      schema_version: '1.0',
      payload: { coefficients: [{ term: 'age', beta: 0.1, ci_lower: 0.01, ci_upper: 0.2, p_value: 0.02 }] },
      risk_signals: [],
      analysis: {
        algorithm_id: 'model_linear',
        dataset_id: 'ds-1',
        dataset_sha256: null,
        columns: [],
        params: {},
        run_id: 'run-12345678',
        run_status: 'completed',
      },
    };
    const message: ChatMessage = {
      ...baseMessage,
      id: 'agent-result',
      role: 'agent',
      content: '模型已经完成。',
      skillResult,
    };

    render(
      <MessageList
        messages={[message]}
        resultPresentation="reference"
        onOpenResult={onOpenResult}
      />,
    );

    expect(screen.getByTestId('analysis-result-reference')).toBeInTheDocument();
    expect(screen.queryByTestId('analysis-result-view')).not.toBeInTheDocument();
    expect(screen.getByText('本机确定性引擎 · 数值非 LLM 生成 · 可审计')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '查看报告' }));
    fireEvent.click(screen.getByRole('button', { name: '查看图表' }));
    fireEvent.click(screen.getByRole('button', { name: '查看代码' }));
    expect(onOpenResult).toHaveBeenNthCalledWith(1, 'report', 'agent-result');
    expect(onOpenResult).toHaveBeenNthCalledWith(2, 'chart', 'agent-result');
    expect(onOpenResult).toHaveBeenNthCalledWith(3, 'code', 'agent-result');
  });

  /**
   * isStructuredTable 曾经只给 ANOVA 加了判据，ttest/correlation/power 三个
   * 同批新增的分析类型被漏掉，导致 Simple 模式（inline 呈现，默认走这里而不是
   * reference）下这三种结果会跌进 GenericKVTable，显示英文字段名与
   * JSON.stringify 裸转储，而不是 ThreeLineTable 已经写好的中文表格。
   * payload 形状照抄 ThreeLineTable.test.tsx 的真机验收用例。
   */
  it('renders the t-test result through ThreeLineTable instead of the generic KV fallback', () => {
    const skillResult: SkillResult = {
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
    const message: ChatMessage = {
      ...baseMessage,
      id: 'agent-ttest',
      role: 'agent',
      content: 't 检验已完成。',
      skillResult,
    };

    render(<MessageList messages={[message]} />);

    expect(screen.getByRole('table', { name: 't 检验结果表' })).toBeInTheDocument();
    // GenericKVTable 会把 payload 的裸键名当「指标」列文本渲染出来；
    // ThreeLineTable 的专用分支只显示中文标签，不会出现这个原始字段名。
    expect(screen.queryByText('t_statistic')).not.toBeInTheDocument();
  });

  it('renders the correlation result through ThreeLineTable instead of the generic KV fallback', () => {
    const skillResult: SkillResult = {
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
    const message: ChatMessage = {
      ...baseMessage,
      id: 'agent-correlation',
      role: 'agent',
      content: '相关分析已完成。',
      skillResult,
    };

    render(<MessageList messages={[message]} />);

    expect(screen.getByRole('table', { name: '相关分析结果表' })).toBeInTheDocument();
    expect(screen.queryByText('t_statistic')).not.toBeInTheDocument();
  });

  it('renders the power/sample-size result through ThreeLineTable instead of the generic KV fallback', () => {
    const skillResult: SkillResult = {
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
    const message: ChatMessage = {
      ...baseMessage,
      id: 'agent-power',
      role: 'agent',
      content: '功效分析已完成。',
      skillResult,
    };

    render(<MessageList messages={[message]} />);

    expect(screen.getByRole('table', { name: '功效与样本量结果表' })).toBeInTheDocument();
    expect(screen.queryByText('achieved_power')).not.toBeInTheDocument();
  });
});

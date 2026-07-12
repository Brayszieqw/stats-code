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
    expect(onOpenResult).toHaveBeenNthCalledWith(1, 'report');
    expect(onOpenResult).toHaveBeenNthCalledWith(2, 'chart');
    expect(onOpenResult).toHaveBeenNthCalledWith(3, 'code');
  });
});

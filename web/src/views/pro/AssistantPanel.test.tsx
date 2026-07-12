/**
 * Tests for AssistantPanel.
 *
 * Validates: Requirements 8.2, 8.3, 8.5
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { AssistantPanel } from './AssistantPanel';
import type { UseSseChatReturn, ChatMessage } from '../../hooks/useSseChat';
import type { DatasetSummary } from '../../api/types';

function makeChat(overrides: Partial<UseSseChatReturn> = {}): UseSseChatReturn {
  return {
    messages: [],
    setMessages: vi.fn(),
    sendMessage: vi.fn(),
    status: 'idle',
    error: null,
    isStreaming: false,
    ...overrides,
  };
}

// A non-empty history so the conversation-mode ChatInputBar renders (the empty
// state shows the WelcomeHero composer instead).
const history: ChatMessage[] = [{ id: 'a1', role: 'agent', content: '你好', timestamp: new Date() }];

const dataset: DatasetSummary = {
  dataset_id: 'ds-1',
  file_name: 'cohort.csv',
  size_bytes: 1024,
  encoding: 'Utf8',
  row_count: 12,
  columns: [{ name: 'age', inferred_type: 'Numeric', missing_count: 0 }],
  uploaded_at: '2026-01-01T00:00:00Z',
  sha256: null,
};

describe('AssistantPanel (Requirements 8.2, 8.3, 8.5)', () => {
  it('wires the empty-state dataset, model, and voice controls', () => {
    const onOpenDatasetPicker = vi.fn();
    const onOpenSettings = vi.fn();
    const onOpenVoiceInput = vi.fn();
    render(
      <AssistantPanel
        sessionId="s1"
        chat={makeChat()}
        isArchived={false}
        onSend={() => {}}
        onChoiceSubmit={() => {}}
        onRetry={() => {}}
        onVoiceTranscript={() => {}}
        datasets={[dataset]}
        selectedDatasetId="ds-1"
        modelLabel="DeepSeek"
        onOpenDatasetPicker={onOpenDatasetPicker}
        onOpenSettings={onOpenSettings}
        onOpenVoiceInput={onOpenVoiceInput}
      />,
    );

    fireEvent.click(screen.getByLabelText('选择数据集'));
    fireEvent.click(screen.getByLabelText('模型选择'));
    fireEvent.click(screen.getByLabelText('语音输入'));

    expect(onOpenDatasetPicker).toHaveBeenCalledTimes(1);
    expect(onOpenSettings).toHaveBeenCalledTimes(1);
    expect(onOpenVoiceInput).toHaveBeenCalledTimes(1);
    expect(screen.getByText(/cohort\.csv/)).toBeInTheDocument();
  });

  it('routes send through onSend (R8.2)', () => {
    const onSend = vi.fn();
    render(
      <AssistantPanel
        sessionId="s1"
        chat={makeChat({ messages: history })}
        isArchived={false}
        onSend={onSend}
        onChoiceSubmit={() => {}}
        onRetry={() => {}}
        onVoiceTranscript={() => {}}
      />,
    );
    fireEvent.change(screen.getByLabelText('助手消息输入框'), { target: { value: '追问一下' } });
    fireEvent.click(screen.getByLabelText('发送'));
    expect(onSend).toHaveBeenCalledWith('追问一下');
  });

  it('keeps input and send clickable mid-stream for interruptive follow-up (R8.3, R8.5)', () => {
    const onSend = vi.fn();
    render(
      <AssistantPanel
        sessionId="s1"
        chat={makeChat({ messages: history, isStreaming: true, status: 'streaming' })}
        isArchived={false}
        onSend={onSend}
        onChoiceSubmit={() => {}}
        onRetry={() => {}}
        onVoiceTranscript={() => {}}
      />,
    );
    const input = screen.getByLabelText('助手消息输入框');
    expect(input).not.toBeDisabled();
    // Voice recorder stays usable during streaming.
    expect(screen.getByLabelText('开始录音')).not.toBeDisabled();
    fireEvent.change(input, { target: { value: '打断追问' } });
    fireEvent.click(screen.getByLabelText('发送'));
    expect(onSend).toHaveBeenCalledWith('打断追问');
  });

  it('disables input and send when the session is archived (R9.3)', () => {
    render(
      <AssistantPanel
        sessionId="s1"
        chat={makeChat({ messages: history })}
        isArchived
        onSend={() => {}}
        onChoiceSubmit={() => {}}
        onRetry={() => {}}
        onVoiceTranscript={() => {}}
      />,
    );
    expect(screen.getByLabelText('助手消息输入框')).toBeDisabled();
    expect(screen.getByLabelText('发送')).toBeDisabled();
  });

  it('keeps analysis output as a reference to the artifact pane', () => {
    const onOpenResult = vi.fn();
    const resultMessage: ChatMessage = {
      id: 'analysis-1',
      role: 'agent',
      content: '分析完成',
      timestamp: new Date(),
      skillResult: {
        schema_version: '1.0',
        payload: {},
        risk_signals: [],
        analysis: {
          algorithm_id: 'model_linear',
          dataset_id: 'ds-1',
          dataset_sha256: null,
          columns: [],
          params: {},
          run_id: 'run-1',
          run_status: 'completed',
        },
      },
    };
    render(
      <AssistantPanel
        sessionId="s1"
        chat={makeChat({ messages: [resultMessage] })}
        isArchived={false}
        onSend={() => {}}
        onChoiceSubmit={() => {}}
        onRetry={() => {}}
        onVoiceTranscript={() => {}}
        onOpenResult={onOpenResult}
      />,
    );

    fireEvent.click(screen.getByLabelText('查看图表'));
    expect(onOpenResult).toHaveBeenCalledWith('chart');
    expect(screen.queryByTestId('analysis-result-view')).not.toBeInTheDocument();
  });
});

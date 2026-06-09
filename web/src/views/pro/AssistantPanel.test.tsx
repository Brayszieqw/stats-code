/**
 * Tests for AssistantPanel.
 *
 * Validates: Requirements 8.2, 8.3, 8.5
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { AssistantPanel } from './AssistantPanel';
import type { UseSseChatReturn, ChatMessage } from '../../hooks/useSseChat';

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

describe('AssistantPanel (Requirements 8.2, 8.3, 8.5)', () => {
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
});

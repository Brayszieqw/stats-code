/**
 * Property 5: 只读态封锁写操作.
 *
 * When isArchived === true, the send/input write controls are disabled in both
 * mode views, while the ModeToggle remains enabled (disabled === false).
 *
 * Validates: Requirements 9.3, 9.4
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import * as fc from 'fast-check';

const { useCoverageMatrixSpy } = vi.hoisted(() => ({ useCoverageMatrixSpy: vi.fn() }));
vi.mock('../lib/coverageMatrixContext', () => ({
  useCoverageMatrix: useCoverageMatrixSpy,
}));

import { SimpleModeView } from './SimpleModeView';
import { ProModeView } from './ProModeView';
import type { SessionController } from '../hooks/useSessionController';
import type { UseSseChatReturn, ChatMessage } from '../hooks/useSseChat';
import type { UseSessionListReturn } from '../hooks/useSessionList';

beforeEach(() => {
  useCoverageMatrixSpy.mockReturnValue({
    matrix: { schema_version: 1, release_version: '1.0.0', algorithms: [] },
    loading: false,
    error: undefined,
  });
});

function makeController(isArchived: boolean): SessionController {
  return {
    sessionId: 's1',
    loading: false,
    error: null,
    isArchived,
    datasets: [],
    decisionAssistant: true,
    setDecisionAssistant: vi.fn(),
    addDataset: vi.fn(),
    initialMessages: [],
    startNewSession: vi.fn(async () => {}),
    loadSession: vi.fn(async () => {}),
  };
}

function makeChat(messages: ChatMessage[]): UseSseChatReturn {
  return {
    messages,
    setMessages: vi.fn(),
    sendMessage: vi.fn(),
    status: 'idle',
    error: null,
    isStreaming: false,
  };
}

function makeList(): UseSessionListReturn {
  return { sessions: [], loading: false, error: null, refresh: vi.fn(async () => {}) };
}

const oneMessage: ChatMessage[] = [{ id: 'u1', role: 'user', content: '你好', timestamp: new Date() }];

describe('Property 5: 只读态封锁写操作 (Requirements 9.3, 9.4)', () => {
  it('SimpleModeView: archived disables input/send but never the ModeToggle', () => {
    fc.assert(
      fc.property(fc.boolean(), (archived) => {
        const { unmount } = render(
          <SimpleModeView
            controller={makeController(archived)}
            chat={makeChat(oneMessage)}
            sessionList={makeList()}
            mode="simple"
            onModeChange={vi.fn()}
            onSend={vi.fn()}
            onChoiceSubmit={vi.fn()}
            onRetry={vi.fn()}
            onVoiceTranscript={vi.fn()}
          />,
        );
        const input = screen.getByLabelText('消息输入框') as HTMLTextAreaElement;
        const send = screen.getByLabelText('发送') as HTMLButtonElement;
        const toggle = screen.getByLabelText('界面模式切换');

        expect(input.disabled).toBe(archived);
        if (archived) expect(send.disabled).toBe(true);
        // ModeToggle (antd Segmented) must never be in the disabled state.
        expect(toggle.querySelector('.ant-segmented-disabled')).toBeNull();
        unmount();
      }),
      { numRuns: 6 },
    );
  }, 20000);

  it('ProModeView: archived disables the assistant input but never the ModeToggle', () => {
    fc.assert(
      fc.property(fc.boolean(), (archived) => {
        const { unmount } = render(
          <ProModeView
            controller={makeController(archived)}
            chat={makeChat(oneMessage)}
            sessionList={makeList()}
            mode="pro"
            onModeChange={vi.fn()}
            onSend={vi.fn()}
            onChoiceSubmit={vi.fn()}
            onRetry={vi.fn()}
            onVoiceTranscript={vi.fn()}
          />,
        );
        const input = screen.getByLabelText('助手消息输入框') as HTMLTextAreaElement;
        const toggle = screen.getByLabelText('界面模式切换');

        expect(input.disabled).toBe(archived);
        expect(toggle.querySelector('.ant-segmented-disabled')).toBeNull();
        unmount();
      }),
      { numRuns: 6 },
    );
  }, 20000);
});

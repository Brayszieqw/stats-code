/**
 * Tests for SimpleModeView.
 *
 * Validates: Requirements 2.3, 3.1, 9.3, 9.4
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { SimpleModeView } from './SimpleModeView';
import type { SessionController } from '../hooks/useSessionController';
import type { UseSseChatReturn } from '../hooks/useSseChat';
import type { UseSessionListReturn } from '../hooks/useSessionList';
import type { ChatMessage } from '../hooks/useSseChat';

function makeController(overrides: Partial<SessionController> = {}): SessionController {
  return {
    sessionId: 's1',
    loading: false,
    error: null,
    isArchived: false,
    datasets: [],
    decisionAssistant: true,
    researchProtocol: null,
    datasetAudits: [],
    analysisPlanApprovals: [],
    integrityWarnings: [],
    setDecisionAssistant: vi.fn(),
    addDataset: vi.fn(),
    saveResearchProtocol: vi.fn(async () => { throw new Error('not used'); }),
    compileResearchProtocol: vi.fn(async () => { throw new Error('not used'); }),
    auditDataset: vi.fn(async () => { throw new Error('not used'); }),
    approveAnalysisPlan: vi.fn(async () => { throw new Error('not used'); }),
    initialMessages: [],
    startNewSession: vi.fn(async () => {}),
    loadSession: vi.fn(async () => {}),
    ...overrides,
  };
}

function makeChat(messages: ChatMessage[] = [], overrides: Partial<UseSseChatReturn> = {}): UseSseChatReturn {
  return {
    messages,
    setMessages: vi.fn(),
    sendMessage: vi.fn(),
    status: 'idle',
    error: null,
    isStreaming: false,
    ...overrides,
  };
}

function makeList(): UseSessionListReturn {
  return { sessions: [], loading: false, error: null, refresh: vi.fn(async () => {}) };
}

function renderView(props: Partial<Parameters<typeof SimpleModeView>[0]> = {}) {
  return render(
    <SimpleModeView
      controller={props.controller ?? makeController()}
      chat={props.chat ?? makeChat()}
      sessionList={props.sessionList ?? makeList()}
      mode="simple"
      onModeChange={props.onModeChange ?? vi.fn()}
      onSend={props.onSend ?? vi.fn()}
      onChoiceSubmit={vi.fn()}
      onRetry={vi.fn()}
      onVoiceTranscript={vi.fn()}
      modelLabel={props.modelLabel}
      onOpenSettings={props.onOpenSettings}
    />,
  );
}

const userMessage: ChatMessage = {
  id: 'u1',
  role: 'user',
  content: '你好',
  timestamp: new Date(),
};

describe('SimpleModeView (Requirements 2.3, 3.1, 9.3, 9.4)', () => {
  it('renders the welcome hero in the empty state (R2.3)', () => {
    renderView({ chat: makeChat([]) });
    expect(screen.getByText('开始你的统计分析')).toBeInTheDocument();
    // The centered composer input is present.
    expect(screen.getByLabelText('消息输入框')).toBeInTheDocument();
  });

  it('renders the message list in the conversation state (R3.1)', () => {
    renderView({ chat: makeChat([userMessage]) });
    expect(screen.getByText('你好')).toBeInTheDocument();
    // The conversation input bar is present.
    expect(screen.getByLabelText('消息输入框')).toBeInTheDocument();
  });

  it('keeps the ModeToggle usable but disables the input when archived (R9.3, R9.4)', () => {
    renderView({
      controller: makeController({ isArchived: true }),
      chat: makeChat([userMessage]),
    });
    expect(screen.getByLabelText('消息输入框')).toBeDisabled();
    expect(screen.getByLabelText('发送')).toBeDisabled();
    // ModeToggle stays enabled.
    expect(screen.getByLabelText('界面模式切换')).toBeInTheDocument();
  });

  it('wires the welcome dataset and voice controls to real drawers', () => {
    renderView({ chat: makeChat([]), onOpenSettings: vi.fn() });

    fireEvent.click(screen.getByLabelText('选择数据集'));
    expect(screen.getByText('选择 / 上传数据集')).toBeInTheDocument();

    fireEvent.click(screen.getByLabelText('语音输入'));
    expect(screen.getByText('语音输入')).toBeInTheDocument();
  });
});

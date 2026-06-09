/**
 * Tests for ProModeView.
 *
 * Validates: Requirements 4.1, 4.3
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';

const { useCoverageMatrixSpy } = vi.hoisted(() => ({ useCoverageMatrixSpy: vi.fn() }));
vi.mock('../lib/coverageMatrixContext', () => ({
  useCoverageMatrix: useCoverageMatrixSpy,
}));

import { ProModeView } from './ProModeView';
import type { SessionController } from '../hooks/useSessionController';
import type { UseSseChatReturn, ChatMessage } from '../hooks/useSseChat';

beforeEach(() => {
  useCoverageMatrixSpy.mockReturnValue({
    matrix: { schema_version: 1, release_version: '1.0.0', algorithms: [] },
    loading: false,
    error: undefined,
  });
});

function makeController(overrides: Partial<SessionController> = {}): SessionController {
  return {
    sessionId: 's1',
    loading: false,
    error: null,
    isArchived: false,
    datasets: [],
    decisionAssistant: true,
    setDecisionAssistant: vi.fn(),
    addDataset: vi.fn(),
    initialMessages: [],
    startNewSession: vi.fn(async () => {}),
    loadSession: vi.fn(async () => {}),
    ...overrides,
  };
}

function makeChat(messages: ChatMessage[] = []): UseSseChatReturn {
  return {
    messages,
    setMessages: vi.fn(),
    sendMessage: vi.fn(),
    status: 'idle',
    error: null,
    isStreaming: false,
  };
}

function renderView() {
  return render(
    <ProModeView
      controller={makeController()}
      chat={makeChat()}
      mode="pro"
      onModeChange={vi.fn()}
      onSend={vi.fn()}
      onChoiceSubmit={vi.fn()}
      onRetry={vi.fn()}
      onVoiceTranscript={vi.fn()}
      model="deepseek-chat"
    />,
  );
}

describe('ProModeView (Requirements 4.1, 4.3)', () => {
  it('renders the multiple panels (TopBar, ReportViewer, CodePanel, AssistantPanel)', () => {
    renderView();
    // TopBar title.
    expect(screen.getByText('Stats 智能科研分析')).toBeInTheDocument();
    // ModeToggle in the TopBar.
    expect(screen.getByLabelText('界面模式切换')).toBeInTheDocument();
    // CodePanel section heading (preserved on small screens, R4.3).
    expect(screen.getByText('等价代码')).toBeInTheDocument();
    // AssistantPanel input.
    expect(screen.getByLabelText('助手消息输入框')).toBeInTheDocument();
    // ReportViewer empty state (no result, no selected dataset).
    expect(screen.getByText(/暂无分析结果/)).toBeInTheDocument();
  });

  it('keeps the CodePanel visible even when ExplorerPanel collapses at narrow widths (R4.3)', () => {
    // matchMedia is mocked to matches:false → screens.lg is falsy → explorer collapses.
    renderView();
    expect(screen.getByText('等价代码')).toBeInTheDocument();
  });
});

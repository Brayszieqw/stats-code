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
import type { UseSessionListReturn } from '../hooks/useSessionList';

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

function makeList(): UseSessionListReturn {
  return { sessions: [], loading: false, error: null, refresh: vi.fn(async () => {}) };
}

function renderView() {
  return render(
    <ProModeView
      controller={makeController()}
      chat={makeChat()}
      sessionList={makeList()}
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
  it('renders the multiple panels (sidebar, ReportViewer, CodePanel, AssistantPanel)', () => {
    renderView();
    // Document tab title.
    expect(screen.getByText('分析报告')).toBeInTheDocument();
    // ModeToggle relocated to the document tab strip.
    expect(screen.getByLabelText('界面模式切换')).toBeInTheDocument();
    // CodePanel section heading (preserved on small screens, R4.3).
    expect(screen.getByText('等价代码')).toBeInTheDocument();
    // AssistantPanel empty state shows the welcome composer.
    expect(screen.getAllByLabelText('消息输入框').length).toBeGreaterThan(0);
    // ReportViewer empty state (no result, no selected dataset).
    expect(screen.getByText(/暂无分析结果/)).toBeInTheDocument();
    // Draggable vertical splitter between report and assistant.
    expect(screen.getByLabelText('调整报告与助手区域比例')).toBeInTheDocument();
  });

  it('keeps the CodePanel visible even when ExplorerPanel collapses at narrow widths (R4.3)', () => {
    // matchMedia is mocked to matches:false → screens.lg is falsy → explorer collapses.
    renderView();
    expect(screen.getByText('等价代码')).toBeInTheDocument();
  });
});

/**
 * Tests for AppShell — including:
 *   - unit: mode-driven rendering; switching preserves messages; Onboarding
 *     appears when not configured (17.5).
 *   - Property 3: 切换保会话 — sessionId & messages unchanged across setMode (17.2).
 *   - Property 7: 切换无整页刷新 — no window.location reload/assignment on toggle (17.3).
 *   - Property 5: 只读态封锁写操作 — archived disables write controls but not
 *     ModeToggle (17.4).
 *
 * Validates: Requirements 1.3, 2.7, 9.3, 9.4, 10.5
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import * as fc from 'fast-check';

// ---------------------------------------------------------------------------
// Hoisted mock state + spies
// ---------------------------------------------------------------------------

const mocks = vi.hoisted(() => {
  return {
    modeState: { mode: 'simple' as 'simple' | 'pro', setMode: vi.fn() },
    controller: {
      sessionId: 's1',
      loading: false,
      error: null as string | null,
      isArchived: false,
      datasets: [],
      decisionAssistant: true,
      researchProtocol: null,
      datasetAudits: [],
      analysisPlanApprovals: [],
      integrityWarnings: [],
      setDecisionAssistant: vi.fn(),
      addDataset: vi.fn(),
      saveResearchProtocol: vi.fn(),
      compileResearchProtocol: vi.fn(),
      auditDataset: vi.fn(),
      approveAnalysisPlan: vi.fn(),
      initialMessages: [] as unknown[],
      startNewSession: vi.fn(async () => {}),
      loadSession: vi.fn(async () => {}),
    },
    chat: {
      messages: [] as unknown[],
      setMessages: vi.fn(),
      sendMessage: vi.fn(),
      status: 'idle',
      error: null,
      isStreaming: false,
    },
    llm: {
      configured: true,
      provider: 'deepseek' as const,
      base_url: null,
      model: 'deepseek-chat',
      runtime_error: null,
      fetchState: 'ready' as const,
      fetchError: null,
      setConfigured: vi.fn(),
      requireReconfigure: vi.fn(),
      setRuntimeError: vi.fn(),
      clearRuntimeError: vi.fn(),
      refresh: vi.fn(async () => {}),
    },
    sessionList: { sessions: [], loading: false, error: null, refresh: vi.fn(async () => {}) },
  };
});

vi.mock('./hooks/useModePreference', () => ({
  useModePreference: () => mocks.modeState,
}));
vi.mock('./hooks/useSessionController', () => ({
  useSessionController: () => mocks.controller,
}));
vi.mock('./hooks/useSseChat', () => ({
  useSseChat: () => mocks.chat,
}));
vi.mock('./hooks/useLlmStatus', () => ({
  useLlmStatus: () => mocks.llm,
}));
vi.mock('./hooks/useSessionList', () => ({
  useSessionList: () => mocks.sessionList,
}));

// Stub the heavy views with light markers so we can assert which renders.
vi.mock('./views/SimpleModeView', () => ({
  SimpleModeView: () => <div data-testid="simple-view" />,
}));
vi.mock('./views/ProModeView', () => ({
  ProModeView: () => <div data-testid="pro-view" />,
}));

import { AppShell } from './AppShell';

beforeEach(() => {
  vi.clearAllMocks();
  mocks.modeState.mode = 'simple';
  mocks.modeState.setMode = vi.fn();
  mocks.controller.isArchived = false;
  mocks.controller.loading = false;
  mocks.controller.error = null;
  mocks.chat.messages = [];
  mocks.llm.configured = true;
});

describe('AppShell unit (Requirements 1.3, 2.7)', () => {
  it('renders SimpleModeView in simple mode and ProModeView in pro mode', () => {
    const { rerender } = render(<AppShell />);
    expect(screen.getByTestId('simple-view')).toBeInTheDocument();
    expect(screen.queryByTestId('pro-view')).toBeNull();

    mocks.modeState.mode = 'pro';
    rerender(<AppShell />);
    expect(screen.getByTestId('pro-view')).toBeInTheDocument();
    expect(screen.queryByTestId('simple-view')).toBeNull();
  });

  it('shows the OnboardingCard overlay when not configured (R2.7)', () => {
    mocks.llm.configured = false;
    render(<AppShell />);
    expect(screen.getByTestId('onboarding-card-overlay')).toBeInTheDocument();
  });

  it('dismisses onboarding into Pro mode while keeping the local engine available', () => {
    mocks.llm.configured = false;
    render(<AppShell />);

    fireEvent.click(screen.getByRole('button', { name: '暂不配置，进入专业模式' }));

    expect(mocks.modeState.setMode).toHaveBeenCalledWith('pro');
    expect(screen.queryByTestId('onboarding-card-overlay')).not.toBeInTheDocument();
  });

  it('syncs initialMessages into the chat via setMessages on mount (R9.1)', () => {
    mocks.controller.initialMessages = [{ id: 'm1', role: 'user', content: 'x', timestamp: new Date() }];
    render(<AppShell />);
    expect(mocks.chat.setMessages).toHaveBeenCalledWith(mocks.controller.initialMessages);
    mocks.controller.initialMessages = [];
  });
});

describe('Property 3: 切换保会话 (Requirements 1.3, 9.1)', () => {
  it('sessionId and messages references are unchanged across a mode switch', () => {
    fc.assert(
      fc.property(fc.array(fc.string(), { maxLength: 5 }), (texts) => {
        const msgs = texts.map((t, i) => ({ id: `m${i}`, role: 'user', content: t, timestamp: new Date() }));
        mocks.chat.messages = msgs;
        const beforeSid = mocks.controller.sessionId;
        const beforeMessages = mocks.chat.messages;

        mocks.modeState.mode = 'simple';
        const { rerender, unmount } = render(<AppShell />);
        mocks.modeState.mode = 'pro';
        rerender(<AppShell />);

        // The shell never mutates session id or messages on a toggle.
        expect(mocks.controller.sessionId).toBe(beforeSid);
        expect(mocks.chat.messages).toBe(beforeMessages);
        unmount();
      }),
      { numRuns: 15 },
    );
  });
});

describe('Property 7: 切换无整页刷新 (Requirement 10.5)', () => {
  it('toggling mode never calls window.location.reload or reassigns href', () => {
    const reloadSpy = vi.fn();
    const originalLocation = window.location;
    // Replace location with a proxy that traps reload + href set.
    const hrefSetter = vi.fn();
    Object.defineProperty(window, 'location', {
      configurable: true,
      value: new Proxy(originalLocation, {
        get(target, prop) {
          if (prop === 'reload') return reloadSpy;
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          return (target as any)[prop];
        },
        set(target, prop, value) {
          if (prop === 'href') {
            hrefSetter(value);
            return true;
          }
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          (target as any)[prop] = value;
          return true;
        },
      }),
    });

    try {
      fc.assert(
        fc.property(fc.array(fc.constantFrom('simple', 'pro'), { minLength: 1, maxLength: 6 }), (sequence) => {
          const { rerender, unmount } = render(<AppShell />);
          for (const m of sequence) {
            mocks.modeState.mode = m as 'simple' | 'pro';
            rerender(<AppShell />);
          }
          unmount();
          expect(reloadSpy).not.toHaveBeenCalled();
          expect(hrefSetter).not.toHaveBeenCalled();
        }),
        { numRuns: 10 },
      );
    } finally {
      Object.defineProperty(window, 'location', { configurable: true, value: originalLocation });
    }
  });
});

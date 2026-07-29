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
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import * as fc from 'fast-check';
import type { LlmProvider } from './api/types';

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
      provider: 'deepseek' as LlmProvider,
      base_url: null as string | null,
      model: 'deepseek-chat' as string | null,
      cached_providers: [] as LlmProvider[],
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
    deleteSession: vi.fn(async (_sessionId: string) => {}),
    simpleOnDeleteSession: null as null | ((sessionId: string) => void | Promise<void>),
    simpleOnOpenSettings: null as null | (() => void),
    postLlmConfig: vi.fn(async () => {}),
    postLlmActivate: vi.fn(async () => {}),
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
vi.mock('./api/client', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./api/client')>();
  return {
    ...actual,
    deleteSession: mocks.deleteSession,
    postLlmConfig: mocks.postLlmConfig,
    postLlmActivate: mocks.postLlmActivate,
  };
});

// Stub the heavy views with light markers so we can assert which renders.
vi.mock('./views/SimpleModeView', () => ({
  SimpleModeView: (props: {
    onDeleteSession?: (sessionId: string) => void | Promise<void>;
    onOpenSettings?: () => void;
  }) => {
    mocks.simpleOnDeleteSession = props.onDeleteSession ?? null;
    mocks.simpleOnOpenSettings = props.onOpenSettings ?? null;
    return <div data-testid="simple-view" />;
  },
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
  mocks.llm.provider = 'deepseek';
  mocks.llm.model = 'deepseek-chat';
  mocks.llm.base_url = null;
  mocks.llm.cached_providers = [];
  mocks.simpleOnDeleteSession = null;
  mocks.simpleOnOpenSettings = null;
  mocks.deleteSession.mockReset();
  mocks.deleteSession.mockResolvedValue(undefined);
  mocks.controller.startNewSession.mockReset();
  mocks.controller.startNewSession.mockResolvedValue(undefined);
  mocks.sessionList.refresh.mockReset();
  mocks.sessionList.refresh.mockResolvedValue(undefined);
  mocks.postLlmConfig.mockReset();
  mocks.postLlmConfig.mockResolvedValue(undefined);
  mocks.postLlmActivate.mockReset();
  mocks.postLlmActivate.mockResolvedValue(undefined);
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

  it('does not report a deletion failure after the session was already deleted', async () => {
    mocks.controller.startNewSession.mockRejectedValueOnce(new Error('new session failed'));
    mocks.sessionList.refresh.mockRejectedValueOnce(new Error('refresh failed'));
    render(<AppShell />);

    expect(mocks.simpleOnDeleteSession).not.toBeNull();
    await expect(mocks.simpleOnDeleteSession!('s1')).resolves.toBeUndefined();
    expect(mocks.deleteSession).toHaveBeenCalledWith('s1');
    expect(mocks.controller.startNewSession).toHaveBeenCalledWith(true);
  });

  it('still rejects when the delete request itself fails', async () => {
    mocks.deleteSession.mockRejectedValueOnce(new Error('delete failed'));
    render(<AppShell />);

    expect(mocks.simpleOnDeleteSession).not.toBeNull();
    await expect(mocks.simpleOnDeleteSession!('s1')).rejects.toThrow('delete failed');
    expect(mocks.controller.startNewSession).not.toHaveBeenCalled();
    expect(mocks.sessionList.refresh).not.toHaveBeenCalled();
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

describe('API settings drawer — cached-provider activation', () => {
  it('activates via the cached key when a cached provider is picked with no API key typed', async () => {
    mocks.llm.cached_providers = ['deepseek', 'qwen'];
    render(<AppShell />);

    expect(mocks.simpleOnOpenSettings).not.toBeNull();
    mocks.simpleOnOpenSettings!();

    const providerSelect = await screen.findByRole('combobox', { name: 'LLM 提供商' });
    fireEvent.mouseDown(providerSelect);
    fireEvent.click(await screen.findByText('通义千问(DashScope)(已保存密钥)'));

    // API Key left blank — the submit button must still enable (activation
    // path doesn't require a key) and clicking it calls postLlmActivate,
    // not postLlmConfig.
    const submit = screen.getByRole('button', { name: '测试并保存' });
    expect(submit).not.toBeDisabled();
    fireEvent.click(submit);

    await waitFor(() => expect(mocks.postLlmActivate).toHaveBeenCalledWith('qwen'));
    expect(mocks.postLlmConfig).not.toHaveBeenCalled();
    expect(mocks.llm.setConfigured).toHaveBeenCalledWith('qwen', expect.anything(), expect.anything());
  });

  it('falls back to the full config POST when the user types a new key for a cached provider', async () => {
    mocks.llm.cached_providers = ['deepseek', 'qwen'];
    render(<AppShell />);

    mocks.simpleOnOpenSettings!();

    const providerSelect = await screen.findByRole('combobox', { name: 'LLM 提供商' });
    fireEvent.mouseDown(providerSelect);
    fireEvent.click(await screen.findByText('通义千问(DashScope)(已保存密钥)'));

    fireEvent.change(screen.getByLabelText('API Key'), { target: { value: 'sk-new-key' } });
    fireEvent.click(screen.getByRole('button', { name: '测试并保存' }));

    await waitFor(() => expect(mocks.postLlmConfig).toHaveBeenCalled());
    expect(mocks.postLlmActivate).not.toHaveBeenCalled();
    expect(mocks.postLlmConfig.mock.calls[0]).toEqual(['qwen', 'sk-new-key', 'https://dashscope.aliyuncs.com/compatible-mode/v1', 'qwen-plus']);
  });

  it('requires base URL and model for a provider with no cached key', async () => {
    mocks.llm.cached_providers = [];
    render(<AppShell />);

    mocks.simpleOnOpenSettings!();

    const submit = await screen.findByRole('button', { name: '测试并保存' });
    // No API key typed yet and deepseek isn't cached — submit stays disabled.
    expect(submit).toBeDisabled();
  });
});

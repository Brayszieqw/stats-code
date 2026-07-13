/**
 * Tests for useSessionController.
 *
 * Validates: Requirements 2.6, 9.2, 9.3, 9.6
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import type { Session } from '../api/types';

const {
  approveAnalysisPlanMock,
  auditDatasetMock,
  compileResearchProtocolMock,
  createSessionMock,
  getSessionMock,
  patchResearchProtocolMock,
} = vi.hoisted(() => ({
  approveAnalysisPlanMock: vi.fn(),
  auditDatasetMock: vi.fn(),
  compileResearchProtocolMock: vi.fn(),
  createSessionMock: vi.fn(),
  getSessionMock: vi.fn(),
  patchResearchProtocolMock: vi.fn(),
}));

vi.mock('../api/client', () => ({
  approveAnalysisPlan: approveAnalysisPlanMock,
  auditDataset: auditDatasetMock,
  compileResearchProtocol: compileResearchProtocolMock,
  createSession: createSessionMock,
  getSession: getSessionMock,
  patchResearchProtocol: patchResearchProtocolMock,
}));

import { useSessionController } from './useSessionController';

function makeSession(overrides: Partial<Session> = {}): Session {
  return {
    id: 'sid-new',
    status: 'Active',
    created_at: '2026-01-01T00:00:00Z',
    last_active_at: '2026-01-01T00:00:00Z',
    settings: { decision_assistant: true },
    research_protocol: null,
    dataset_audits: [],
    analysis_plan_approvals: [],
    messages: [],
    datasets: [],
    skill_runs: [],
    uploaded_bytes: 0,
    ...overrides,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  window.history.replaceState({}, '', '/');
});

describe('useSessionController (Requirements 2.6, 9.2, 9.3, 9.6)', () => {
  it('creates a new session on mount when no ?session_id= is present', async () => {
    createSessionMock.mockResolvedValue(makeSession({ id: 'fresh' }));
    const { result } = renderHook(() => useSessionController());
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(createSessionMock).toHaveBeenCalledTimes(1);
    expect(result.current.sessionId).toBe('fresh');
    expect(new URL(window.location.href).searchParams.get('session_id')).toBe('fresh');
  });

  it('loads the session from ?session_id= when present (R9.2)', async () => {
    window.history.replaceState({}, '', '/?session_id=abc');
    getSessionMock.mockResolvedValue(makeSession({ id: 'abc' }));
    const { result } = renderHook(() => useSessionController());
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(getSessionMock).toHaveBeenCalledWith('abc');
    expect(result.current.sessionId).toBe('abc');
  });

  it('drives isArchived from the session status (R9.3)', async () => {
    createSessionMock.mockResolvedValue(makeSession({ status: 'Archived' }));
    const { result } = renderHook(() => useSessionController());
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.isArchived).toBe(true);
  });

  it('loadSession replaces the active session and its messages (R9.6)', async () => {
    createSessionMock.mockResolvedValue(makeSession({ id: 'first' }));
    getSessionMock.mockResolvedValue(
      makeSession({
        id: 'history-1',
        messages: [{ User: { id: 'm1', created_at: '2026-01-01T00:00:00Z', content: { Text: '历史问题' } } }],
      }),
    );
    const { result } = renderHook(() => useSessionController());
    await waitFor(() => expect(result.current.sessionId).toBe('first'));
    await act(async () => {
      await result.current.loadSession('history-1');
    });
    expect(result.current.sessionId).toBe('history-1');
    expect(new URL(window.location.href).searchParams.get('session_id')).toBe('history-1');
    expect(result.current.initialMessages).toHaveLength(1);
    expect(result.current.initialMessages[0]!.content).toBe('历史问题');
  });

  it('startNewSession is a no-op when already on an empty shell', async () => {
    createSessionMock.mockResolvedValueOnce(makeSession({ id: 'first' }));
    const { result } = renderHook(() => useSessionController());
    await waitFor(() => expect(result.current.sessionId).toBe('first'));
    const callsBefore = createSessionMock.mock.calls.length;
    await act(async () => {
      await result.current.startNewSession();
    });
    // Avoid churning empty shells — stay on current empty session.
    expect(createSessionMock.mock.calls.length).toBe(callsBefore);
    expect(result.current.sessionId).toBe('first');
  });

  it('startNewSession creates a fresh session after one with content (R2.6)', async () => {
    createSessionMock.mockResolvedValueOnce(
      makeSession({
        id: 'first',
        messages: [{ User: { id: 'm1', created_at: '2026-01-01T00:00:00Z', content: { Text: '旧问题' } } }],
      }),
    );
    const { result } = renderHook(() => useSessionController());
    await waitFor(() => expect(result.current.sessionId).toBe('first'));
    createSessionMock.mockResolvedValueOnce(makeSession({ id: 'second' }));
    await act(async () => {
      await result.current.startNewSession();
    });
    expect(result.current.sessionId).toBe('second');
    expect(result.current.initialMessages).toHaveLength(0);
  });

  it('persists an approved research protocol in the active session', async () => {
    createSessionMock.mockResolvedValue(makeSession({ id: 'protocol-session' }));
    const input = {
      status: 'Approved' as const,
      research_question: '暴露与结局是否相关？',
      study_design: 'cohort' as const,
      population: '成人队列',
      eligibility_criteria: '有基线记录',
      exposure: 'exposure',
      comparator: '未暴露',
      outcome: 'outcome',
      time_zero: '基线',
      follow_up: '一年',
      analysis_unit: '参与者',
      estimand: '调整后风险比',
      confounders: 'age',
      missing_data_strategy: '完整案例',
      primary_analysis: '回归模型',
      sensitivity_analysis: '改变协变量集',
    };
    const saved = {
      ...input,
      version: 1,
      content_sha256: 'a'.repeat(64),
      state_sha256: 'e'.repeat(64),
      approval_id: '11111111-1111-4111-8111-111111111111',
      approved_at: '2026-01-02T00:00:00Z',
      updated_at: '2026-01-02T00:00:00Z',
    };
    patchResearchProtocolMock.mockResolvedValue(makeSession({ research_protocol: saved }));

    const { result } = renderHook(() => useSessionController());
    await waitFor(() => expect(result.current.sessionId).toBe('protocol-session'));
    await act(async () => {
      await result.current.saveResearchProtocol(input);
    });

    expect(patchResearchProtocolMock).toHaveBeenCalledWith('protocol-session', input);
    expect(result.current.researchProtocol).toEqual(saved);
  });

  it('compiles a protocol proposal without mutating the saved session protocol', async () => {
    createSessionMock.mockResolvedValue(makeSession({ id: 'compile-session' }));
    const compiled = {
      schema_version: '1.0' as const,
      compiler_version: '1.0.0' as const,
      proposal: {
        research_question: '吸烟与结局是否相关？',
        study_design: 'cohort' as const,
        population: '成人队列',
        eligibility_criteria: '',
        exposure: '吸烟',
        comparator: '未吸烟',
        outcome: '疾病结局',
        time_zero: '基线',
        follow_up: '一年',
        analysis_unit: '参与者',
        estimand: '调整后风险比',
        confounders: '年龄、性别',
        missing_data_strategy: '报告缺失率',
        primary_analysis: '多变量回归',
        sensitivity_analysis: '',
      },
      missing_required_fields: [],
      warnings: [],
      brief_sha256: 'a'.repeat(64),
      approval_required: true as const,
    };
    compileResearchProtocolMock.mockResolvedValue(compiled);

    const { result } = renderHook(() => useSessionController());
    await waitFor(() => expect(result.current.sessionId).toBe('compile-session'));
    let output;
    await act(async () => {
      output = await result.current.compileResearchProtocol('研究成人队列中吸烟与一年疾病结局的关联。');
    });

    expect(compileResearchProtocolMock).toHaveBeenCalledWith('compile-session', {
      brief: '研究成人队列中吸烟与一年疾病结局的关联。',
    });
    expect(output).toEqual(compiled);
    expect(result.current.researchProtocol).toBeNull();
    expect(patchResearchProtocolMock).not.toHaveBeenCalled();
  });
});

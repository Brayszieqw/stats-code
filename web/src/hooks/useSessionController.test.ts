/**
 * Tests for useSessionController.
 *
 * Validates: Requirements 2.6, 9.2, 9.3, 9.6
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import type { Session } from '../api/types';

const { createSessionMock, getSessionMock } = vi.hoisted(() => ({
  createSessionMock: vi.fn(),
  getSessionMock: vi.fn(),
}));

vi.mock('../api/client', () => ({
  createSession: createSessionMock,
  getSession: getSessionMock,
}));

import { useSessionController } from './useSessionController';

function makeSession(overrides: Partial<Session> = {}): Session {
  return {
    id: 'sid-new',
    status: 'Active',
    created_at: '2026-01-01T00:00:00Z',
    last_active_at: '2026-01-01T00:00:00Z',
    settings: { decision_assistant: true },
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
    expect(result.current.initialMessages).toHaveLength(1);
    expect(result.current.initialMessages[0]!.content).toBe('历史问题');
  });

  it('startNewSession creates a fresh empty session (R2.6)', async () => {
    createSessionMock.mockResolvedValueOnce(makeSession({ id: 'first' }));
    const { result } = renderHook(() => useSessionController());
    await waitFor(() => expect(result.current.sessionId).toBe('first'));
    createSessionMock.mockResolvedValueOnce(makeSession({ id: 'second' }));
    await act(async () => {
      await result.current.startNewSession();
    });
    expect(result.current.sessionId).toBe('second');
    expect(result.current.initialMessages).toHaveLength(0);
  });
});

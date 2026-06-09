/**
 * Tests for useSessionList.
 *
 * Validates: Requirements 9.7, 11.1
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor, act } from '@testing-library/react';
import type { ReactNode } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { SessionSummary } from '../api/types';

const { listSessionsMock } = vi.hoisted(() => ({ listSessionsMock: vi.fn() }));
vi.mock('../api/client', () => ({ listSessions: listSessionsMock }));

import { useSessionList } from './useSessionList';

function wrapper() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
}

const summary: SessionSummary = {
  id: 's1',
  status: 'Active',
  created_at: '2026-01-01T00:00:00Z',
  last_active_at: '2026-01-02T00:00:00Z',
  message_count: 2,
  title: '分析',
  dataset_count: 1,
};

beforeEach(() => {
  vi.clearAllMocks();
});

describe('useSessionList (Requirements 9.7, 11.1)', () => {
  it('maps the loaded session summaries', async () => {
    listSessionsMock.mockResolvedValue([summary]);
    const { result } = renderHook(() => useSessionList(), { wrapper: wrapper() });
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.sessions).toEqual([summary]);
    expect(result.current.error).toBeNull();
  });

  it('exposes an error message on failure', async () => {
    listSessionsMock.mockRejectedValue(new Error('网络异常'));
    const { result } = renderHook(() => useSessionList(), { wrapper: wrapper() });
    await waitFor(() => expect(result.current.error).not.toBeNull());
    expect(result.current.error).toBe('网络异常');
  });

  it('refresh re-queries the endpoint', async () => {
    listSessionsMock.mockResolvedValue([]);
    const { result } = renderHook(() => useSessionList(), { wrapper: wrapper() });
    await waitFor(() => expect(result.current.loading).toBe(false));
    listSessionsMock.mockResolvedValue([summary]);
    await act(async () => {
      await result.current.refresh();
    });
    await waitFor(() => expect(result.current.sessions).toEqual([summary]));
    expect(listSessionsMock).toHaveBeenCalledTimes(2);
  });
});

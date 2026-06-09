/**
 * useSessionList — history-session list backed by TanStack Query over
 * listSessions() (GET /api/sessions). Exposes sessions / loading / error /
 * refresh for the simple-mode sidebar.
 *
 * Validates: Requirements 9.7, 11.1
 */

import { useCallback } from 'react';
import { useQuery } from '@tanstack/react-query';
import { listSessions } from '../api/client';
import type { SessionSummary } from '../api/types';

export const SESSION_LIST_QUERY_KEY = ['sessions'] as const;

export interface UseSessionListReturn {
  sessions: SessionSummary[];
  loading: boolean;
  error: string | null;
  refresh: () => Promise<void>;
}

export function useSessionList(): UseSessionListReturn {
  const query = useQuery({
    queryKey: SESSION_LIST_QUERY_KEY,
    queryFn: listSessions,
  });

  const refresh = useCallback(async () => {
    await query.refetch();
  }, [query]);

  return {
    sessions: query.data ?? [],
    loading: query.isLoading,
    error: query.isError ? (query.error instanceof Error ? query.error.message : '加载会话列表失败') : null,
    refresh,
  };
}

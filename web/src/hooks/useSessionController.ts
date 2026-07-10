/**
 * useSessionController — owns the session lifecycle that the former ChatPage
 * managed inline: read ?session_id= on mount (else create), expose loading /
 * error / archived state, datasets, decision-assistant flag, the initial
 * mapped messages, and start/load/addDataset actions.
 *
 * Validates: Requirements 2.6, 9.1, 9.2, 9.3, 9.6
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import { createSession, getSession } from '../api/client';
import { mapSessionMessages } from '../lib/sessionMessages';
import type { ChatMessage } from './useSseChat';
import type { DatasetSummary, Session } from '../api/types';

export interface SessionController {
  sessionId: string;
  loading: boolean;
  error: string | null;
  isArchived: boolean;
  datasets: DatasetSummary[];
  decisionAssistant: boolean;
  setDecisionAssistant: (v: boolean) => void;
  addDataset: (s: DatasetSummary) => void;
  /** Messages mapped from the loaded session; the shell syncs them into useSseChat. */
  initialMessages: ChatMessage[];
  startNewSession: () => Promise<void>;
  loadSession: (sid: string) => Promise<void>;
}

function readUrlSessionId(): string | null {
  try {
    return new URLSearchParams(window.location.search).get('session_id');
  } catch {
    return null;
  }
}

export function useSessionController(): SessionController {
  const [sessionId, setSessionId] = useState('');
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [isArchived, setIsArchived] = useState(false);
  const [datasets, setDatasets] = useState<DatasetSummary[]>([]);
  const [decisionAssistant, setDecisionAssistant] = useState(true);
  const [initialMessages, setInitialMessages] = useState<ChatMessage[]>([]);

  const applySession = useCallback((session: Session) => {
    setSessionId(session.id);
    setDecisionAssistant(session.settings.decision_assistant);
    setDatasets(session.datasets ?? []);
    setIsArchived(session.status === 'Archived');
    setInitialMessages(mapSessionMessages(session.messages ?? []));
  }, []);

  // 请求序号：快速连点切换会话时，只允许最后一次请求落地，
  // 防止慢响应覆盖新会话状态。
  const requestSeqRef = useRef(0);

  const loadSession = useCallback(
    async (sid: string) => {
      const seq = ++requestSeqRef.current;
      setLoading(true);
      setError(null);
      try {
        const session = await getSession(sid);
        if (seq !== requestSeqRef.current) return; // 已被更新的请求取代
        applySession(session);
      } catch (err) {
        if (seq !== requestSeqRef.current) return;
        setError(err instanceof Error ? err.message : '加载会话失败');
      } finally {
        if (seq === requestSeqRef.current) setLoading(false);
      }
    },
    [applySession],
  );

  const startNewSession = useCallback(async () => {
    const seq = ++requestSeqRef.current;
    setLoading(true);
    setError(null);
    try {
      const session = await createSession();
      if (seq !== requestSeqRef.current) return;
      applySession(session);
    } catch (err) {
      if (seq !== requestSeqRef.current) return;
      setError(err instanceof Error ? err.message : '创建会话失败');
    } finally {
      if (seq === requestSeqRef.current) setLoading(false);
    }
  }, [applySession]);

  const addDataset = useCallback((s: DatasetSummary) => {
    setDatasets((prev) => (prev.some((d) => d.dataset_id === s.dataset_id) ? prev : [...prev, s]));
  }, []);

  // Mount: load from ?session_id= or create a fresh session.
  const didInit = useRef(false);
  useEffect(() => {
    if (didInit.current) return;
    didInit.current = true;
    const urlSid = readUrlSessionId();
    void (urlSid ? loadSession(urlSid) : startNewSession());
  }, [loadSession, startNewSession]);

  return {
    sessionId,
    loading,
    error,
    isArchived,
    datasets,
    decisionAssistant,
    setDecisionAssistant,
    addDataset,
    initialMessages,
    startNewSession,
    loadSession,
  };
}

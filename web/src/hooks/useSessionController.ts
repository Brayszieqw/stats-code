/**
 * useSessionController — owns the session lifecycle that the former ChatPage
 * managed inline: read ?session_id= on mount (else create), expose loading /
 * error / archived state, datasets, decision-assistant flag, the initial
 * mapped messages, and start/load/addDataset actions.
 *
 * Validates: Requirements 2.6, 9.1, 9.2, 9.3, 9.6
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import {
  approveAnalysisPlan as requestAnalysisPlanApproval,
  auditDataset as requestDatasetAudit,
  compileResearchProtocol as requestProtocolCompilation,
  createSession,
  getSession,
  patchResearchProtocol,
} from '../api/client';
import { mapSessionMessages } from '../lib/sessionMessages';
import type { ChatMessage } from './useSseChat';
import type {
  AnalysisPlanApproval,
  AnalysisPlanApprovalRequest,
  DatasetAudit,
  DatasetAuditRequest,
  DatasetSummary,
  ProtocolCompileResult,
  ResearchProtocol,
  ResearchProtocolInput,
  Session,
  SessionIntegrityWarning,
} from '../api/types';

export interface SessionController {
  sessionId: string;
  loading: boolean;
  error: string | null;
  isArchived: boolean;
  datasets: DatasetSummary[];
  decisionAssistant: boolean;
  researchProtocol: ResearchProtocol | null;
  datasetAudits: DatasetAudit[];
  analysisPlanApprovals: AnalysisPlanApproval[];
  integrityWarnings: SessionIntegrityWarning[];
  setDecisionAssistant: (v: boolean) => void;
  addDataset: (s: DatasetSummary) => void;
  saveResearchProtocol: (input: ResearchProtocolInput) => Promise<ResearchProtocol>;
  compileResearchProtocol: (brief: string) => Promise<ProtocolCompileResult>;
  auditDataset: (datasetId: string, input: DatasetAuditRequest) => Promise<DatasetAudit>;
  approveAnalysisPlan: (input: AnalysisPlanApprovalRequest) => Promise<AnalysisPlanApproval>;
  /** Messages mapped from the loaded session; the shell syncs them into useSseChat. */
  initialMessages: ChatMessage[];
  startNewSession: (force?: boolean) => Promise<void>;
  loadSession: (sid: string) => Promise<void>;
}

function readUrlSessionId(): string | null {
  try {
    return new URLSearchParams(window.location.search).get('session_id');
  } catch {
    return null;
  }
}

function writeUrlSessionId(sessionId: string | null): void {
  try {
    const url = new URL(window.location.href);
    if (url.searchParams.get('session_id') === sessionId) return;
    if (sessionId) {
      url.searchParams.set('session_id', sessionId);
    } else {
      url.searchParams.delete('session_id');
    }
    window.history.replaceState(window.history.state, '', url);
  } catch {
    // Embedded shells may not expose a mutable History API; session state still works in memory.
  }
}

export function useSessionController(): SessionController {
  const [sessionId, setSessionId] = useState('');
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [isArchived, setIsArchived] = useState(false);
  const [datasets, setDatasets] = useState<DatasetSummary[]>([]);
  const [decisionAssistant, setDecisionAssistant] = useState(true);
  const [researchProtocol, setResearchProtocol] = useState<ResearchProtocol | null>(null);
  const [datasetAudits, setDatasetAudits] = useState<DatasetAudit[]>([]);
  const [analysisPlanApprovals, setAnalysisPlanApprovals] = useState<AnalysisPlanApproval[]>([]);
  const [integrityWarnings, setIntegrityWarnings] = useState<SessionIntegrityWarning[]>([]);
  const [initialMessages, setInitialMessages] = useState<ChatMessage[]>([]);

  const applySession = useCallback((session: Session) => {
    writeUrlSessionId(session.id);
    setSessionId(session.id);
    setDecisionAssistant(session.settings.decision_assistant);
    setResearchProtocol(session.research_protocol ?? null);
    setDatasetAudits(session.dataset_audits ?? []);
    setAnalysisPlanApprovals(session.analysis_plan_approvals ?? []);
    setIntegrityWarnings(session.integrity_warnings ?? []);
    setDatasets(session.datasets ?? []);
    setIsArchived(session.status === 'Archived');
    setInitialMessages(mapSessionMessages(session.messages ?? []));
  }, []);

  // 请求序号：快速连点切换会话时，只允许最后一次请求落地，
  // 防止慢响应覆盖新会话状态。
  const requestSeqRef = useRef(0);
  const activeSessionIdRef = useRef('');
  activeSessionIdRef.current = sessionId;

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

  const startNewSession = useCallback(async (force = false) => {
    // Already on an empty shell — avoid churning session creates.
    if (
      !force &&
      sessionId &&
      datasets.length === 0 &&
      initialMessages.length === 0 &&
      !researchProtocol &&
      !isArchived
    ) {
      setError(null);
      return;
    }
    const seq = ++requestSeqRef.current;
    setLoading(true);
    setError(null);
    try {
      // Backend reuses/purges empty shells; this is still a single POST.
      const session = await createSession();
      if (seq !== requestSeqRef.current) return;
      applySession(session);
    } catch (err) {
      if (seq !== requestSeqRef.current) return;
      const message = err instanceof Error ? err.message : '创建会话失败';
      setError(message);
      if (force) {
        // The old session was already deleted. Keep reload as a recovery path.
        writeUrlSessionId(null);
        throw err instanceof Error ? err : new Error(message);
      }
    } finally {
      if (seq === requestSeqRef.current) setLoading(false);
    }
  }, [applySession, sessionId, datasets.length, initialMessages.length, researchProtocol, isArchived]);

  const addDataset = useCallback((s: DatasetSummary) => {
    setDatasets((prev) => (prev.some((d) => d.dataset_id === s.dataset_id) ? prev : [...prev, s]));
  }, []);

  const saveResearchProtocol = useCallback(async (input: ResearchProtocolInput) => {
    if (!sessionId) throw new Error('会话尚未就绪');
    const targetSessionId = sessionId;
    const session = await patchResearchProtocol(targetSessionId, {
      ...input,
      ...(researchProtocol ? { expected_version: researchProtocol.version } : {}),
    });
    const saved = session.research_protocol;
    if (!saved) throw new Error('后端未返回研究协议');
    if (activeSessionIdRef.current === targetSessionId) setResearchProtocol(saved);
    return saved;
  }, [researchProtocol, sessionId]);

  const compileResearchProtocol = useCallback(async (brief: string) => {
    if (!sessionId) throw new Error('会话尚未就绪');
    return requestProtocolCompilation(sessionId, { brief });
  }, [sessionId]);

  const auditDataset = useCallback(async (datasetId: string, input: DatasetAuditRequest) => {
    if (!sessionId) throw new Error('会话尚未就绪');
    const targetSessionId = sessionId;
    const audit = await requestDatasetAudit(targetSessionId, datasetId, input);
    if (activeSessionIdRef.current === targetSessionId) {
      setDatasetAudits((current) => current.some((item) => item.audit_id === audit.audit_id)
        ? current
        : [...current, audit]);
    }
    return audit;
  }, [sessionId]);

  const approveAnalysisPlan = useCallback(async (input: AnalysisPlanApprovalRequest) => {
    if (!sessionId) throw new Error('会话尚未就绪');
    const targetSessionId = sessionId;
    const approval = await requestAnalysisPlanApproval(targetSessionId, input);
    if (activeSessionIdRef.current === targetSessionId) {
      setAnalysisPlanApprovals((current) => current.some((item) => item.plan_id === approval.plan_id)
        ? current
        : [...current, approval]);
    }
    return approval;
  }, [sessionId]);

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
    researchProtocol,
    datasetAudits,
    analysisPlanApprovals,
    integrityWarnings,
    setDecisionAssistant,
    addDataset,
    saveResearchProtocol,
    compileResearchProtocol,
    auditDataset,
    approveAnalysisPlan,
    initialMessages,
    startNewSession,
    loadSession,
  };
}

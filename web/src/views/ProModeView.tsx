/**
 * ProModeView — 专业模式视图。
 *
 * 左侧采用与简易模式一致的 Stats 智能分析导航（SimpleSidebar：导航 + 历史会话），
 * 替换原 IDE 活动栏 + 资源管理器。中部为文档标签 + 报告区（含数据集上传/选择与
 * 画像），右侧为多语言等价代码面板 + 运行控制，中下为常驻 AI 助手。
 * 响应式：低于 lg(992px) 折叠左侧栏，保留 CodePanel 可见。
 *
 * Validates: Requirements 4.1, 4.2, 4.3, 4.4
 */

import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { Button, Drawer, Grid, Layout, Tag, Typography } from 'antd';
import {
  AreaChartOutlined,
  CloseOutlined,
  DatabaseOutlined,
  FileProtectOutlined,
  MenuOutlined,
  UploadOutlined,
} from '@ant-design/icons';
import { SimpleSidebar } from './simple/SimpleSidebar';
import { ModeToggle } from '../components/ModeToggle';
import { AssistantPanel } from './pro/AssistantPanel';
import { AnalysisWorkspace, type WorkspaceView } from './pro/AnalysisWorkspace';
import type { ReportArtifact } from './pro/ReportViewer';
import { DatasetUploader } from '../components/DatasetUploader';
import { AnalysisPreflightModal } from '../components/AnalysisPreflightModal';
import { ResearchProtocolDrawer } from '../components/ResearchProtocolDrawer';
import { ResearchWorkflowBar } from '../components/ResearchWorkflowBar';
import { VoiceRecorder } from '../components/VoiceRecorder';
import { SessionIntegrityAlert } from '../components/SessionIntegrityAlert';
import { useLatestAnalysis } from '../hooks/useLatestAnalysis';
import { methodShortLabel } from '../lib/displayLabels';
import { runSkill, ApiError } from '../api/client';
import type { SessionController } from '../hooks/useSessionController';
import type { UseSseChatReturn } from '../hooks/useSseChat';
import type { UseSessionListReturn } from '../hooks/useSessionList';
import type { ViewMode } from '../hooks/useModePreference';
import type { ChoiceAnswer, DatasetAudit, DatasetSummary, RunRequest, SkillResult } from '../api/types';
import type { ResearchProtocolInput } from '../api/types';
import type { ChatMessage } from '../hooks/useSseChat';

const { Sider, Content } = Layout;
const { useBreakpoint } = Grid;
const { Text } = Typography;

const PANEL_BG = '#f7f7f5';
const BORDER = '1px solid #e3e3df';

export function mergeWorkspaceMessages(
  chatMessages: ChatMessage[],
  directMessages: ChatMessage[],
  directRunId?: string,
): ChatMessage[] {
  if (directMessages.length === 0) return chatMessages;
  if (directRunId && chatMessages.some((message) => message.skillResult?.analysis?.run_id === directRunId)) {
    return chatMessages;
  }
  return [...chatMessages, ...directMessages]
    .map((message, index) => ({ message, index }))
    .sort((left, right) => {
      const byTime = left.message.timestamp.getTime() - right.message.timestamp.getTime();
      return byTime === 0 ? left.index - right.index : byTime;
    })
    .map(({ message }) => message);
}

function findPreviousUserPrompt(messages: ChatMessage[], beforeMessageId?: string): string | null {
  const messageIndex = beforeMessageId
    ? messages.findIndex((message) => message.id === beforeMessageId)
    : messages.length;
  const startIndex = messageIndex < 0 ? messages.length - 1 : messageIndex - 1;
  for (let index = startIndex; index >= 0; index--) {
    const message = messages[index];
    if (message?.role === 'user' && message.content.trim()) {
      return message.content.trim();
    }
  }
  return null;
}

function hasSkillResult(message: ChatMessage | null): message is ChatMessage & { skillResult: SkillResult } {
  return Boolean(message?.skillResult);
}

export interface ProModeViewProps {
  controller: SessionController;
  chat: UseSseChatReturn;
  sessionList: UseSessionListReturn;
  mode: ViewMode;
  onModeChange: (m: ViewMode) => void;
  onSend: (text: string) => void;
  onChoiceSubmit: (a: ChoiceAnswer) => void;
  onRetry: () => void;
  onVoiceTranscript: (t: string) => void;
  model?: string | null;
  llmConfigured?: boolean;
  onOpenSettings?: () => void;
  onDeleteSession?: (sessionId: string) => void | Promise<void>;
  onPurgeEmptySessions?: () => void | Promise<void>;
}

export function ProModeView({
  controller,
  chat,
  sessionList,
  mode,
  onModeChange,
  onSend,
  onChoiceSubmit,
  onRetry,
  onVoiceTranscript,
  model,
  llmConfigured = true,
  onOpenSettings,
  onDeleteSession,
  onPurgeEmptySessions,
}: ProModeViewProps) {
  const screens = useBreakpoint();
  const { sessionId, datasets, isArchived, addDataset, integrityWarnings } = controller;

  const [selectedDataset, setSelectedDataset] = useState<DatasetSummary | null>(null);
  const [lastProfiledDataset, setLastProfiledDataset] = useState<DatasetSummary | null>(null);
  const [uploaderOpen, setUploaderOpen] = useState(false);
  const [voiceDrawerOpen, setVoiceDrawerOpen] = useState(false);
  const [directRunResult, setDirectRunResult] = useState<SkillResult | null>(null);
  const [directRunPrompt, setDirectRunPrompt] = useState('');
  const [directRunStartedAt, setDirectRunStartedAt] = useState<Date | null>(null);
  const [directRunCompletedAt, setDirectRunCompletedAt] = useState<Date | null>(null);
  const [directRunError, setDirectRunError] = useState<string | null>(null);
  const [directRunRunning, setDirectRunRunning] = useState(false);
  const [pendingRun, setPendingRun] = useState<{ request: RunRequest; promptText: string } | null>(null);
  const [protocolDrawerOpen, setProtocolDrawerOpen] = useState(false);
  const [protocolSaving, setProtocolSaving] = useState(false);
  const [protocolError, setProtocolError] = useState<string | null>(null);
  const [pendingAudit, setPendingAudit] = useState<DatasetAudit | null>(null);
  const [pendingAuditLoading, setPendingAuditLoading] = useState(false);
  const [pendingAuditError, setPendingAuditError] = useState<string | null>(null);
  const [planApprovalRunning, setPlanApprovalRunning] = useState(false);
  const [workspaceView, setWorkspaceView] = useState<WorkspaceView>('report');
  const [workspaceOpen, setWorkspaceOpen] = useState(false);
  const [selectedMessageId, setSelectedMessageId] = useState<string | null>(null);
  const activeSessionIdRef = useRef(sessionId);
  const directRunRequestRef = useRef(0);
  const preflightRequestRef = useRef(0);
  const directRunAbortRef = useRef<AbortController | null>(null);
  activeSessionIdRef.current = sessionId;

  // 会话切换时重置本地派生状态，避免上个会话的数据集选择/直跑结果串台。
  useLayoutEffect(() => {
    directRunAbortRef.current?.abort();
    directRunAbortRef.current = null;
    directRunRequestRef.current += 1;
    setSelectedDataset(null);
    setLastProfiledDataset(null);
    setDirectRunResult(null);
    setDirectRunPrompt('');
    setDirectRunStartedAt(null);
    setDirectRunCompletedAt(null);
    setDirectRunError(null);
    setDirectRunRunning(false);
    setPendingRun(null);
    setProtocolDrawerOpen(false);
    setProtocolSaving(false);
    setProtocolError(null);
    setPendingAudit(null);
    setPendingAuditLoading(false);
    setPendingAuditError(null);
    setPlanApprovalRunning(false);
    setWorkspaceView('report');
    setWorkspaceOpen(false);
    setSelectedMessageId(null);
  }, [sessionId]);

  // 仅 1 个数据集时自动选中，避免用户必须再点 Tag 才出现分析配置器。
  useEffect(() => {
    if (datasets.length === 1) {
      const only = datasets[0]!;
      setSelectedDataset((prev) => (prev?.dataset_id === only.dataset_id ? prev : only));
      setLastProfiledDataset((prev) => (prev?.dataset_id === only.dataset_id ? prev : only));
    }
  }, [datasets, sessionId]);

  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  useEffect(() => {
    if (typeof screens.lg === 'boolean') setSidebarCollapsed(!screens.lg);
  }, [screens.lg]);

  const directRunMessages = useMemo<ChatMessage[]>(() => {
    if (!directRunResult) return [];
    const runId = directRunResult.analysis?.run_id ?? 'local';
    const resultTimestamp = directRunCompletedAt ?? new Date(0);
    const resultMessage: ChatMessage = {
      id: `direct-run-result-${runId}`,
      role: 'agent',
      content: '',
      skillResult: directRunResult,
      timestamp: resultTimestamp,
    };
    // Do not inject the configurator prompt as a fake user bubble — that looks
    // like the user wrote an LLM prompt and can imply LLM computed the numbers.
    const algorithmId = directRunResult.analysis?.algorithm_id;
    const methodLabel = algorithmId ? methodShortLabel(algorithmId) : '统计分析';
    const planCard: ChatMessage = {
      id: `direct-run-plan-${runId}`,
      role: 'agent',
      content:
        `已提交结构化分析方案 #${runId.slice(0, 8)}（${methodLabel}）。` +
        '统计数值由本机确定性引擎生成，不是大模型推算。',
      timestamp: directRunStartedAt ?? resultTimestamp,
    };
    return [planCard, resultMessage];
  }, [directRunCompletedAt, directRunPrompt, directRunResult, directRunStartedAt]);

  const workspaceMessages = useMemo(() => {
    return mergeWorkspaceMessages(
      chat.messages,
      directRunMessages,
      directRunResult?.analysis?.run_id,
    );
  }, [chat.messages, directRunMessages, directRunResult?.analysis?.run_id]);

  const { result: latestResult, resultMessage: latestResultMessage } = useLatestAnalysis(workspaceMessages);
  const pinnedMessage = useMemo(
    () => selectedMessageId
      ? workspaceMessages.find((message) => message.id === selectedMessageId) ?? null
      : null,
    [selectedMessageId, workspaceMessages],
  );
  const pinnedArtifact: ReportArtifact | null = hasSkillResult(pinnedMessage)
    ? { resultMessage: pinnedMessage }
    : null;
  const result = pinnedArtifact?.resultMessage.skillResult ?? latestResult;
  const latestArtifactKey = latestResultMessage?.id ?? null;
  useLayoutEffect(() => {
    setSelectedMessageId(null);
  }, [latestArtifactKey]);
  const isViewingHistorical = Boolean(
    pinnedArtifact && pinnedArtifact.resultMessage.id !== latestResultMessage?.id,
  );
  const analysis = result?.analysis ?? null;
  const analysisDataset = useMemo(
    () => (analysis ? datasets.find((dataset) => dataset.dataset_id === analysis.dataset_id) ?? null : null),
    [analysis, datasets],
  );
  const currentUserPrompt = useMemo(
    () => findPreviousUserPrompt(workspaceMessages) ?? '尚未提出研究问题',
    [workspaceMessages],
  );
  const latestArtifactPrompt = useMemo(
    () => (
      latestResultMessage
        ? findPreviousUserPrompt(workspaceMessages, latestResultMessage.id)
        : findPreviousUserPrompt(workspaceMessages)
    ) ?? '尚未提出研究问题',
    [latestResultMessage, workspaceMessages],
  );
  const historicalUserPrompt = useMemo(
    () => selectedMessageId
      ? findPreviousUserPrompt(workspaceMessages, selectedMessageId)
      : null,
    [selectedMessageId, workspaceMessages],
  );
  const workspaceTitle = pinnedArtifact
    ? historicalUserPrompt ?? '历史分析结果'
    : latestArtifactPrompt;

  const handleSelect = (ds: DatasetSummary | null) => {
    setSelectedDataset(ds);
    if (ds) setLastProfiledDataset(ds);
    setSelectedMessageId(null);
    setDirectRunResult(null);
    setDirectRunPrompt('');
    setDirectRunStartedAt(null);
    setDirectRunCompletedAt(null);
    setDirectRunError(null);
    setPendingAudit(null);
    setPendingAuditError(null);
    setWorkspaceView('data');
    setWorkspaceOpen(true);
  };

  const executeConfiguredRun = async (request: RunRequest, promptText: string) => {
    if (isArchived) return;
    const runSessionId = sessionId;
    const requestId = ++directRunRequestRef.current;
    directRunAbortRef.current?.abort();
    const abortController = new AbortController();
    directRunAbortRef.current = abortController;
    setDirectRunRunning(true);
    setDirectRunError(null);
    setDirectRunResult(null);
    setDirectRunCompletedAt(null);
    setDirectRunPrompt(promptText);
    setDirectRunStartedAt(new Date());
    try {
      const skillResult = await runSkill(runSessionId, request, abortController.signal);
      if (activeSessionIdRef.current !== runSessionId || directRunRequestRef.current !== requestId) return;
      setDirectRunResult(skillResult);
      setDirectRunCompletedAt(new Date());
      setWorkspaceView('report');
      setWorkspaceOpen(true);
      await sessionList.refresh();
    } catch (err) {
      if (abortController.signal.aborted) return;
      if (activeSessionIdRef.current !== runSessionId || directRunRequestRef.current !== requestId) return;
      setDirectRunError(err instanceof ApiError ? err.payload.message : err instanceof Error ? err.message : '运行失败');
    } finally {
      if (activeSessionIdRef.current === runSessionId && directRunRequestRef.current === requestId) {
        setDirectRunRunning(false);
      }
      if (directRunAbortRef.current === abortController) directRunAbortRef.current = null;
    }
  };

  const handleConfiguredRun = async (request: RunRequest, promptText: string) => {
    if (isArchived) return;
    setPendingRun({ request, promptText });
  };

  useEffect(() => {
    const pending = pendingRun;
    const protocol = controller.researchProtocol;
    const requestId = ++preflightRequestRef.current;
    if (!pending || protocol?.status !== 'Approved') {
      setPendingAudit(null);
      setPendingAuditLoading(false);
      setPendingAuditError(null);
      return;
    }
    const targetSessionId = sessionId;
    setPendingAudit(null);
    setPendingAuditLoading(true);
    setPendingAuditError(null);
    void controller.auditDataset(pending.request.dataset_id, {
      skill_id: pending.request.skill_id,
      args: pending.request.args,
      expected_protocol_version: protocol.version,
    }).then((audit) => {
      if (requestId !== preflightRequestRef.current || activeSessionIdRef.current !== targetSessionId) return;
      setPendingAudit(audit);
    }).catch((err) => {
      if (requestId !== preflightRequestRef.current || activeSessionIdRef.current !== targetSessionId) return;
      setPendingAuditError(err instanceof ApiError ? err.payload.message : err instanceof Error ? err.message : '服务端审计失败');
    }).finally(() => {
      if (requestId === preflightRequestRef.current && activeSessionIdRef.current === targetSessionId) {
        setPendingAuditLoading(false);
      }
    });
  }, [controller.auditDataset, controller.researchProtocol, pendingRun, sessionId]);

  const handleApproveAndRun = async () => {
    const confirmed = pendingRun;
    const protocol = controller.researchProtocol;
    if (!confirmed || protocol?.status !== 'Approved' || !pendingAudit || pendingAudit.status === 'blocked') return;
    const targetSessionId = sessionId;
    const requestId = preflightRequestRef.current;
    setPlanApprovalRunning(true);
    setPendingAuditError(null);
    try {
      const approval = await controller.approveAnalysisPlan({
        skill_id: confirmed.request.skill_id,
        dataset_id: confirmed.request.dataset_id,
        args: confirmed.request.args,
        expected_protocol_version: protocol.version,
        expected_audit_id: pendingAudit.audit_id,
        expected_audit_sha256: pendingAudit.audit_sha256,
        audit_roles: pendingAudit.roles,
      });
      if (requestId !== preflightRequestRef.current || activeSessionIdRef.current !== targetSessionId) return;
      setPendingRun(null);
      void executeConfiguredRun({ ...confirmed.request, plan_id: approval.plan_id }, confirmed.promptText);
    } catch (err) {
      if (requestId !== preflightRequestRef.current || activeSessionIdRef.current !== targetSessionId) return;
      setPendingAuditError(err instanceof ApiError ? err.payload.message : err instanceof Error ? err.message : '方案审批失败');
    } finally {
      if (requestId === preflightRequestRef.current && activeSessionIdRef.current === targetSessionId) {
        setPlanApprovalRunning(false);
      }
    }
  };

  const handleSaveProtocol = async (input: ResearchProtocolInput) => {
    setProtocolSaving(true);
    setProtocolError(null);
    try {
      await controller.saveResearchProtocol(input);
      setProtocolDrawerOpen(false);
    } catch (err) {
      setProtocolError(err instanceof Error ? err.message : '研究协议保存失败');
    } finally {
      setProtocolSaving(false);
    }
  };

  // Chat gate errors: surface the correct drawer/panel instead of a dead-end banner.
  useEffect(() => {
    const code = chat.error?.error_code;
    if (code === 'ResearchProtocolRequired' || code === 'ResearchVersionConflict') {
      setProtocolDrawerOpen(true);
    }
    if (
      code === 'ResearchApprovalRequired'
      || code === 'ResearchApprovalStale'
      || code === 'ResearchAuditBlocked'
    ) {
      setWorkspaceOpen(true);
    }
  }, [chat.error?.error_code]);

  const handleInspectorRunComplete = (skillResult: SkillResult, runSessionId: string) => {
    if (activeSessionIdRef.current !== runSessionId) return;
    setDirectRunPrompt('');
    setDirectRunStartedAt(null);
    setDirectRunResult(skillResult);
    setDirectRunCompletedAt(new Date());
    setDirectRunError(null);
    setWorkspaceView('report');
    setWorkspaceOpen(true);
    void sessionList.refresh();
  };

  const handleOpenResult = (view: 'report' | 'chart' | 'code', messageId: string) => {
    setSelectedMessageId(messageId === latestResultMessage?.id ? null : messageId);
    setWorkspaceView(view);
    setWorkspaceOpen(true);
  };

  const activeDataset = selectedDataset ?? lastProfiledDataset;
  const currentAudit = useMemo(() => {
    const protocol = controller.researchProtocol;
    if (!activeDataset || !protocol) return null;
    if (
      pendingAudit?.dataset_id === activeDataset.dataset_id
      && pendingAudit.protocol_version === protocol.version
    ) return pendingAudit;
    return [...controller.datasetAudits].reverse().find((audit) => (
      audit.dataset_id === activeDataset.dataset_id
      && audit.protocol_version === protocol.version
    )) ?? null;
  }, [activeDataset, controller.datasetAudits, controller.researchProtocol, pendingAudit]);
  const currentPlanApproval = useMemo(() => {
    const protocol = controller.researchProtocol;
    if (!activeDataset || protocol?.status !== 'Approved' || !protocol.approval_id) return null;
    return [...controller.analysisPlanApprovals].reverse().find((approval) => (
      approval.dataset_id === activeDataset.dataset_id
      && approval.protocol_version === protocol.version
      && approval.protocol_sha256 === protocol.content_sha256
      && approval.protocol_approval_id === protocol.approval_id
    )) ?? null;
  }, [activeDataset, controller.analysisPlanApprovals, controller.researchProtocol]);
  const artifactDataset = pinnedArtifact || workspaceView !== 'data'
    ? analysisDataset
    : activeDataset;
  const docTitle = analysis?.algorithm_id === 'power'
    ? '功效分析'
    : analysisDataset?.file_name ?? activeDataset?.file_name ?? '分析报告';
  const isWorking = directRunRunning || chat.isStreaming;
  const statusLabel = isWorking ? '分析执行中' : result ? '结果已生成' : activeDataset ? '数据已就绪' : '等待研究问题';

  const sidebarWidth = screens.xxl ? 270 : 248;
  const workspaceWidth = screens.xxl ? 520 : 480;
  const [wideWorkspace, setWideWorkspace] = useState(() => {
    if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') return true;
    return window.matchMedia('(min-width: 1360px)').matches;
  });
  useEffect(() => {
    if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') return undefined;
    const media = window.matchMedia('(min-width: 1360px)');
    const sync = () => setWideWorkspace(media.matches);
    sync();
    media.addEventListener('change', sync);
    return () => media.removeEventListener('change', sync);
  }, []);
  const isNarrow = !wideWorkspace;
  const isSidebarOverlay = screens.lg === false;

  const workspace = (
    <AnalysisWorkspace
      view={workspaceView}
      onViewChange={setWorkspaceView}
      onClose={() => setWorkspaceOpen(false)}
      title={workspaceTitle}
      messages={workspaceMessages}
      selectedDataset={selectedDataset}
      artifactDataset={artifactDataset}
      analysisDataset={analysisDataset}
      analysis={analysis}
      hasResult={Boolean(result)}
      sessionId={sessionId}
      isArchived={isArchived}
      isRunning={directRunRunning}
      runError={directRunError}
      pinnedArtifact={pinnedArtifact}
      isViewingHistorical={isViewingHistorical}
      onReturnToLatest={() => setSelectedMessageId(null)}
      onConfiguredRun={handleConfiguredRun}
      onInspectorRunComplete={handleInspectorRunComplete}
    />
  );

  return (
    <Layout className="stats-shell stats-shell--pro">
      <Layout className="pro-shell" style={{ background: PANEL_BG }}>
        <Sider
          className="stats-sidebar"
          width={sidebarWidth}
          collapsible
          collapsed={sidebarCollapsed}
          collapsedWidth={0}
          trigger={null}
          breakpoint="lg"
          onBreakpoint={(broken) => setSidebarCollapsed(broken)}
          style={{
            background: '#f3f3f1',
            borderRight: BORDER,
            overflow: 'hidden',
            height: '100%',
            ...(isSidebarOverlay && !sidebarCollapsed
              ? {
                  position: 'fixed',
                  inset: '0 auto 0 0',
                  height: '100dvh',
                  zIndex: 30,
                  boxShadow: '8px 0 24px rgba(31, 43, 56, 0.16)',
                }
              : {}),
          }}
        >
          {isSidebarOverlay && !sidebarCollapsed ? (
            <Button
              type="text"
              size="small"
              icon={<CloseOutlined />}
              aria-label="收起侧边栏"
              onClick={() => setSidebarCollapsed(true)}
              style={{ position: 'absolute', top: 8, right: 8, zIndex: 2 }}
            />
          ) : null}
          <SimpleSidebar
            sessionList={sessionList}
            activeSessionId={sessionId}
            onNewSession={() => {
              void controller.startNewSession().then(() => sessionList.refresh());
            }}
            onSelectSession={(sid) => {
              void controller.loadSession(sid);
            }}
            sessionId={sessionId}
            isArchived={isArchived}
            decisionAssistant={controller.decisionAssistant}
            onDecisionAssistantChange={controller.setDecisionAssistant}
            onOpenDatasetUpload={() => setUploaderOpen(true)}
            onOpenSettings={onOpenSettings}
            onUseTemplate={onSend}
            onDeleteSession={onDeleteSession}
            onPurgeEmptySessions={onPurgeEmptySessions}
          />
        </Sider>

        <Layout className="pro-main-shell">
          <header className="pro-thread-topbar">
            {sidebarCollapsed ? (
              <Button
                type="text"
                size="small"
                icon={<MenuOutlined />}
                aria-label="打开侧边栏"
                onClick={() => setSidebarCollapsed(false)}
              />
            ) : null}
            <div className="pro-thread-heading">
              <strong title={currentUserPrompt}>{currentUserPrompt === '尚未提出研究问题' ? '专业统计分析' : currentUserPrompt}</strong>
              <span className={isWorking ? 'is-working' : ''} aria-live="polite">
                <i aria-hidden /> {statusLabel} · {docTitle}
              </span>
            </div>
            <div className="pro-thread-actions">
              <Button
                type="text"
                size="small"
                icon={<FileProtectOutlined />}
                onClick={() => setProtocolDrawerOpen(true)}
                disabled={isArchived}
                aria-label="打开研究协议"
              >
                协议
              </Button>
              <Button
                type="text"
                size="small"
                icon={<UploadOutlined />}
                onClick={() => setUploaderOpen(true)}
                disabled={isArchived}
                aria-label="上传数据集"
              >
                数据
              </Button>
              <Button
                type={workspaceOpen ? 'default' : 'text'}
                size="small"
                icon={<AreaChartOutlined />}
                aria-label={workspaceOpen ? '收起分析检查器' : '打开分析检查器'}
                onClick={() => setWorkspaceOpen((open) => !open)}
              >
                检查器
              </Button>
              <span className="pro-privacy">
                <span className="privacy-status__dot" aria-hidden />
                本机确定性引擎 · 数值非 LLM 生成
              </span>
              {!llmConfigured ? (
                <span className="pro-llm-status">AI 解读未配置 · 统计引擎可用</span>
              ) : null}
              <ModeToggle mode={mode} onChange={onModeChange} />
            </div>
          </header>

          <SessionIntegrityAlert warnings={integrityWarnings} style={{ margin: '10px 16px 0' }} />

          <div className="pro-thread-context" aria-label="分析上下文">
            <span className="pro-context-label"><DatabaseOutlined /> Context</span>
            <Text type="secondary" className="pro-context-count">{datasets.length} 个数据集</Text>
            {datasets.map((ds) => {
              const isSel = (selectedDataset?.dataset_id ?? null) === ds.dataset_id;
              return (
              <Button
                key={ds.dataset_id}
                type={isSel ? 'default' : 'text'}
                size="small"
                className={`pro-dataset-context-button${isSel ? ' is-selected' : ''}`}
                aria-pressed={isSel}
                onClick={() => {
                  if (isArchived) return;
                  handleSelect(isSel ? null : ds);
                }}
                aria-label={`数据集: ${ds.file_name}`}
              >
                {ds.file_name} · {ds.row_count} × {ds.columns.length}
              </Button>
              );
            })}
            {/* 契约上 algorithm_id 必填，但历史会话里实测存在缺该字段的记录；
                缺失时不显示方法胶囊，而不是渲染一个「未知」标签。 */}
            {analysis?.algorithm_id ? (
              <Tag className="pro-method-tag">{methodShortLabel(analysis.algorithm_id)}</Tag>
            ) : null}
          </div>

          <ResearchWorkflowBar
            protocol={controller.researchProtocol}
            datasetReady={Boolean(activeDataset)}
            auditStatus={currentAudit?.status ?? null}
            planApproved={Boolean(currentPlanApproval)}
            isRunning={isWorking}
            resultReady={Boolean(result)}
            onOpenProtocol={() => setProtocolDrawerOpen(true)}
          />

          <Layout className="pro-codex-body">
            <Content className="pro-thread-content">
              <main className="pro-thread-conversation" aria-label="研究对话">
                <AssistantPanel
                  sessionId={sessionId}
                  chat={chat}
                  messages={workspaceMessages}
                  isArchived={isArchived}
                  onSend={onSend}
                  onChoiceSubmit={onChoiceSubmit}
                  onRetry={onRetry}
                  onVoiceTranscript={onVoiceTranscript}
                  datasets={datasets}
                  selectedDatasetId={selectedDataset?.dataset_id ?? null}
                  modelLabel={model}
                  onOpenDatasetPicker={() => setUploaderOpen(true)}
                  onOpenSettings={onOpenSettings}
                  onOpenProtocol={() => setProtocolDrawerOpen(true)}
                  onOpenInspector={() => {
                    setWorkspaceOpen(true);
                    setWorkspaceView('report');
                  }}
                  onOpenVoiceInput={() => setVoiceDrawerOpen(true)}
                  onOpenResult={handleOpenResult}
                />
              </main>
            </Content>
            {!isNarrow && workspaceOpen ? (
              <Sider className="pro-workspace-sider" width={workspaceWidth} theme="light">
                {workspace}
              </Sider>
            ) : null}
          </Layout>
        </Layout>
      </Layout>

      <Drawer title="上传数据集" placement="left" width={420} open={uploaderOpen} onClose={() => setUploaderOpen(false)}>
        <DatasetUploader
          sessionId={sessionId}
          onUploadComplete={(s) => {
            addDataset(s);
            handleSelect(s);
            setUploaderOpen(false);
            void sessionList.refresh();
          }}
        />
      </Drawer>

      <ResearchProtocolDrawer
        open={protocolDrawerOpen}
        protocol={controller.researchProtocol}
        saving={protocolSaving}
        readOnly={isArchived}
        error={protocolError}
        onClose={() => {
          if (!protocolSaving) setProtocolDrawerOpen(false);
        }}
        onCompile={controller.compileResearchProtocol}
        onSave={handleSaveProtocol}
      />

      <Drawer title="语音输入" placement="right" width={360} open={voiceDrawerOpen} onClose={() => setVoiceDrawerOpen(false)}>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
          <Text type="secondary">
            录音完成后会把转写文本发送到当前会话；低置信度结果会先让你确认。
          </Text>
          <VoiceRecorder
            sessionId={sessionId}
            disabled={isArchived}
            onTranscript={(text) => {
              onVoiceTranscript(text);
              setVoiceDrawerOpen(false);
            }}
          />
        </div>
      </Drawer>

      <Drawer
        placement="right"
        width="min(100vw, 560px)"
        open={isNarrow && workspaceOpen}
        onClose={() => setWorkspaceOpen(false)}
        closable={false}
        styles={{ body: { padding: 0 } }}
      >
        {workspace}
      </Drawer>

      {pendingRun ? (
        <AnalysisPreflightModal
          open
          dataset={datasets.find((dataset) => dataset.dataset_id === pendingRun.request.dataset_id) ?? selectedDataset!}
          request={pendingRun.request}
          promptText={pendingRun.promptText}
          protocol={controller.researchProtocol}
          audit={pendingAudit}
          auditLoading={pendingAuditLoading}
          auditError={pendingAuditError}
          confirming={planApprovalRunning}
          onCancel={() => setPendingRun(null)}
          onEditProtocol={() => setProtocolDrawerOpen(true)}
          onConfirm={() => void handleApproveAndRun()}
        />
      ) : null}
    </Layout>
  );
}

export default ProModeView;

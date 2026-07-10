/**
 * AppShell — 双模式外壳层：统一持有跨模式共享状态并据 mode 渲染两个视图。
 *
 * 实例化 useModePreference / useSessionController / useSseChat（单实例）/
 * useLlmStatus / useSessionList。会话加载或切换后将 controller.initialMessages
 * 同步进 useSseChat.setMessages；顶层 loading→Spin、error→Result+重新加载。
 * 据 mode 渲染 SimpleModeView / ProModeView，下发共享状态与统一回调
 * （handleSend / handleChoiceSubmit / handleRetry / handleVoiceTranscript）。
 * !configured 时渲染 OnboardingCard 遮罩；handleSend 在 Archived 时不发送，
 * 流式途中调用 sendMessage 实现打断式追问。
 *
 * Validates: Requirements 1.3, 2.6, 2.7, 8.3, 9.1
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import { Spin, Result, Button, Drawer, Space, Typography, Input, Select, Alert } from 'antd';
import { LoadingOutlined } from '@ant-design/icons';
import { useModePreference } from './hooks/useModePreference';
import { useSessionController } from './hooks/useSessionController';
import { useSseChat } from './hooks/useSseChat';
import { useLlmStatus } from './hooks/useLlmStatus';
import { useSessionList } from './hooks/useSessionList';
import { SimpleModeView } from './views/SimpleModeView';
import { ProModeView } from './views/ProModeView';
import {
  CUSTOM_MODEL_VALUE,
  DEFAULT_BASE_URLS,
  getDefaultModel,
  getModelOptions,
  isKnownModel,
  OnboardingCard,
} from './components/OnboardingCard';
import { postLlmConfig, deleteSession, listSessions, ApiError } from './api/client';
import type { ChoiceAnswer, LlmProvider } from './api/types';

const { Paragraph } = Typography;

export function AppShell() {
  const { mode, setMode } = useModePreference();
  const controller = useSessionController();
  const chat = useSseChat(controller.sessionId);
  const llm = useLlmStatus();
  const sessionList = useSessionList();
  const deletingSessionIds = useRef(new Set<string>());

  const { setMessages } = chat;
  const { initialMessages, loading, error, isArchived } = controller;

  // ─── 会话加载/切换后同步消息进 useSseChat ───────────────────────────────
  // initialMessages 引用在 applySession 后变化；据此重置聊天消息。
  useEffect(() => {
    setMessages(initialMessages);
  }, [initialMessages, setMessages]);

  // ─── 统一发送逻辑（打断式追问）──────────────────────────────────────────
  const lastSentRef = useRef<string>('');
  const handleSend = useCallback(
    (text: string) => {
      const trimmed = text.trim();
      if (!trimmed || isArchived) return;
      lastSentRef.current = trimmed;
      // useSseChat.sendMessage 内部会 abort 进行中的流并发起新请求，
      // 故流式途中调用即实现打断式追问（R8.3）。
      chat.sendMessage(trimmed);
    },
    [chat, isArchived],
  );

  const handleChoiceSubmit = useCallback(
    (answer: ChoiceAnswer) => {
      const parts: string[] = [];
      if (answer.options.length > 0) parts.push(`已选择: ${answer.options.join(', ')}`);
      if (answer.custom_text) parts.push(answer.custom_text);
      handleSend(parts.join(' | ') || '继续');
    },
    [handleSend],
  );

  const handleRetry = useCallback(() => {
    if (lastSentRef.current) handleSend(lastSentRef.current);
  }, [handleSend]);

  const handleVoiceTranscript = useCallback((text: string) => handleSend(text), [handleSend]);

  const previousChatStatus = useRef(chat.status);
  useEffect(() => {
    const previous = previousChatStatus.current;
    const completed =
      (previous === 'connecting' || previous === 'streaming') &&
      (chat.status === 'idle' || chat.status === 'error');
    previousChatStatus.current = chat.status;
    if (completed) {
      void sessionList.refresh();
    }
  }, [chat.status, sessionList]);

  const handleDeleteSession = useCallback(
    async (sid: string) => {
      if (deletingSessionIds.current.has(sid)) {
        return;
      }
      deletingSessionIds.current.add(sid);
      try {
        await deleteSession(sid);
        if (sid === controller.sessionId) {
          await controller.startNewSession();
        }
        await sessionList.refresh();
      } finally {
        deletingSessionIds.current.delete(sid);
      }
    },
    [controller, sessionList],
  );

  /** Remove empty history shells (0 messages & 0 datasets), keep current session. */
  const handlePurgeEmptySessions = useCallback(async () => {
    const all = await listSessions();
    const currentId = controller.sessionId;
    const victims = all.filter(
      (s) => s.id !== currentId && s.message_count === 0 && s.dataset_count === 0,
    );
    for (const s of victims) {
      try {
        await deleteSession(s.id);
      } catch {
        /* best-effort */
      }
    }
    await sessionList.refresh();
  }, [controller.sessionId, sessionList]);

  // ─── LLM 配置（Onboarding + 设置抽屉）──────────────────────────────────
  const [llmSubmitting, setLlmSubmitting] = useState(false);
  const [llmError, setLlmError] = useState<string | null>(null);
  const handleLlmSubmit = useCallback(
    async (provider: LlmProvider, apiKey: string, baseUrl: string, model: string) => {
      setLlmSubmitting(true);
      setLlmError(null);
      try {
        await postLlmConfig(provider, apiKey, baseUrl, model);
        llm.setConfigured(provider, baseUrl, model);
      } catch (err) {
        setLlmError(err instanceof ApiError ? err.payload.message : err instanceof Error ? err.message : '配置保存失败');
      } finally {
        setLlmSubmitting(false);
      }
    },
    [llm],
  );

  // 设置抽屉（专业模式 TopBar 设置入口）。
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [sProvider, setSProvider] = useState<LlmProvider>('deepseek');
  const [sBaseUrl, setSBaseUrl] = useState('');
  const [sModel, setSModel] = useState(getDefaultModel('deepseek'));
  const [sCustomModel, setSCustomModel] = useState('');
  const [sApiKey, setSApiKey] = useState('');
  const [sSubmitting, setSSubmitting] = useState(false);
  const [sError, setSError] = useState<string | null>(null);
  const [sUrlDirty, setSUrlDirty] = useState(false);

  useEffect(() => {
    if (!settingsOpen) return;
    const provider = llm.provider ?? 'deepseek';
    const model = llm.model?.trim() ?? '';
    setSProvider(provider);
    if (llm.base_url) {
      setSBaseUrl(llm.base_url);
      setSUrlDirty(true);
    } else {
      setSBaseUrl(DEFAULT_BASE_URLS[provider]);
      setSUrlDirty(false);
    }
    if (model && isKnownModel(provider, model)) {
      setSModel(model);
      setSCustomModel('');
    } else if (model) {
      setSModel(CUSTOM_MODEL_VALUE);
      setSCustomModel(model);
    } else {
      setSModel(getDefaultModel(provider));
      setSCustomModel('');
    }
    setSApiKey('');
    setSError(null);
  }, [settingsOpen, llm.provider, llm.base_url, llm.model]);

  const handleSettingsProviderChange = (next: LlmProvider) => {
    setSProvider(next);
    if (!sUrlDirty) setSBaseUrl(DEFAULT_BASE_URLS[next]);
    setSModel(getDefaultModel(next));
    setSCustomModel('');
  };

  const handleSettingsSubmit = async () => {
    const key = sApiKey.trim();
    const url = sBaseUrl.trim();
    const model = sModel === CUSTOM_MODEL_VALUE ? sCustomModel.trim() : sModel.trim();
    if (!key || !url || !model) {
      setSError('请填写 API Key、API Base URL 与模型');
      return;
    }
    setSSubmitting(true);
    setSError(null);
    try {
      await postLlmConfig(sProvider, key, url, model);
      llm.setConfigured(sProvider, url, model);
      setSettingsOpen(false);
    } catch (err) {
      setSError(err instanceof ApiError ? err.payload.message : err instanceof Error ? err.message : '配置测试或保存失败');
    } finally {
      setSSubmitting(false);
    }
  };

  // ─── 顶层 loading / error ──────────────────────────────────────────────
  const modelLabel = llm.model ?? (llm.provider === 'openai' ? 'OpenAI' : 'DeepSeek');

  if (loading) {
    return (
      <div style={{ display: 'flex', justifyContent: 'center', alignItems: 'center', height: '100vh' }}>
        <Spin indicator={<LoadingOutlined style={{ fontSize: 36 }} spin />} />
      </div>
    );
  }

  if (error) {
    return (
      <Result
        status="error"
        title="会话加载失败"
        subTitle={error}
        extra={
          <Button type="primary" onClick={() => window.location.reload()}>
            重新加载
          </Button>
        }
      />
    );
  }

  // ─── 据 mode 渲染视图 ──────────────────────────────────────────────────
  return (
    <>
      {mode === 'simple' ? (
        <SimpleModeView
          controller={controller}
          chat={chat}
          sessionList={sessionList}
          mode={mode}
          onModeChange={setMode}
          onSend={handleSend}
          onChoiceSubmit={handleChoiceSubmit}
          onRetry={handleRetry}
          onVoiceTranscript={handleVoiceTranscript}
          modelLabel={modelLabel}
          onOpenSettings={() => setSettingsOpen(true)}
          onDeleteSession={handleDeleteSession}
          onPurgeEmptySessions={handlePurgeEmptySessions}
        />
      ) : (
        <ProModeView
          controller={controller}
          chat={chat}
          sessionList={sessionList}
          mode={mode}
          onModeChange={setMode}
          onSend={handleSend}
          onChoiceSubmit={handleChoiceSubmit}
          onRetry={handleRetry}
          onVoiceTranscript={handleVoiceTranscript}
          model={modelLabel}
          onOpenSettings={() => setSettingsOpen(true)}
          onDeleteSession={handleDeleteSession}
          onPurgeEmptySessions={handlePurgeEmptySessions}
        />
      )}

      {/* LLM 未配置遮罩 */}
      {!llm.configured && (
        <OnboardingCard onSubmit={handleLlmSubmit} submitting={llmSubmitting} errorMessage={llmError} />
      )}

      {/* API 设置抽屉 */}
      <Drawer
        title="API 设置"
        placement="right"
        width={380}
        open={settingsOpen}
        onClose={() => setSettingsOpen(false)}
        extra={
          <Button
            type="primary"
            onClick={handleSettingsSubmit}
            loading={sSubmitting}
            disabled={
              !sApiKey.trim() ||
              !sBaseUrl.trim() ||
              !(sModel === CUSTOM_MODEL_VALUE ? sCustomModel.trim() : sModel.trim())
            }
          >
            测试并保存
          </Button>
        }
      >
        <Space direction="vertical" size={16} style={{ width: '100%' }}>
          <label style={{ display: 'block' }}>
            <Paragraph style={{ marginBottom: 4, fontSize: 13 }}>提供商</Paragraph>
            <Select<LlmProvider>
              value={sProvider}
              onChange={handleSettingsProviderChange}
              options={[
                { value: 'deepseek', label: 'DeepSeek' },
                { value: 'openai', label: 'OpenAI' },
              ]}
              disabled={sSubmitting}
              style={{ width: '100%' }}
              aria-label="LLM 提供商"
            />
          </label>

          <label style={{ display: 'block' }}>
            <Paragraph style={{ marginBottom: 4, fontSize: 13 }}>API Base URL</Paragraph>
            <Input
              value={sBaseUrl}
              onChange={(e) => {
                setSBaseUrl(e.target.value);
                setSUrlDirty(true);
              }}
              disabled={sSubmitting}
              aria-label="API Base URL"
            />
          </label>

          <label style={{ display: 'block' }}>
            <Paragraph style={{ marginBottom: 4, fontSize: 13 }}>Model</Paragraph>
            <Select<string>
              value={sModel}
              onChange={setSModel}
              options={getModelOptions(sProvider)}
              disabled={sSubmitting}
              style={{ width: '100%' }}
              aria-label="LLM model"
            />
          </label>

          {sModel === CUSTOM_MODEL_VALUE ? (
            <label style={{ display: 'block' }}>
              <Paragraph style={{ marginBottom: 4, fontSize: 13 }}>Custom model</Paragraph>
              <Input
                value={sCustomModel}
                onChange={(e) => setSCustomModel(e.target.value)}
                disabled={sSubmitting}
                aria-label="Custom LLM model"
              />
            </label>
          ) : null}

          <label style={{ display: 'block' }}>
            <Paragraph style={{ marginBottom: 4, fontSize: 13 }}>API Key</Paragraph>
            <Input.Password
              value={sApiKey}
              onChange={(e) => setSApiKey(e.target.value)}
              placeholder="输入新的 API Key"
              autoComplete="off"
              disabled={sSubmitting}
              aria-label="API Key"
            />
          </label>

          {sError ? <Alert type="error" showIcon role="alert" message={<span style={{ color: '#cf1322', fontSize: 13 }}>{sError}</span>} /> : null}
        </Space>
      </Drawer>
    </>
  );
}

export default AppShell;

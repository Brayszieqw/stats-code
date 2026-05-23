/**
 * ChatPage — 主聊天页面（Lobe Chat / ChatGPT 风格双栏布局）
 *
 * 布局：
 *   ┌────────┬─────────────────────────────────┐
 *   │ Sider  │ Header（标题 + 辅助决策开关）      │
 *   │  数据   ├─────────────────────────────────┤
 *   │  集    │ MessageList                     │
 *   │  上传   │ ErrorBanner (when error)        │
 *   │  与    │                                 │
 *   │  历史   ├─────────────────────────────────┤
 *   │       │ Input row：[VoiceRecorder] [TextArea] [Send] │
 *   └────────┴─────────────────────────────────┘
 *
 * 集成所有子组件：
 *   - VoiceRecorder       (Requirements 2.1, 2.2, 2.4)
 *   - DatasetUploader     (Requirements 3.1, 3.2, 3.3)
 *   - DecisionAssistantToggle (Requirements 5.1, 5.4)
 *   - ErrorBanner         (Requirements 14.1, 14.2, 14.3)
 *   - MessageList + ChoicePromptCard
 *
 * Validates: Requirements 1.1, 7.5, plus the above.
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import {
  Layout,
  Input,
  Button,
  Space,
  Spin,
  Typography,
  Result,
  Drawer,
  Tag,
  Empty,
  Alert,
  Select,
  theme as antdTheme,
} from 'antd';
import {
  SendOutlined,
  LoadingOutlined,
  DatabaseOutlined,
  ThunderboltOutlined,
  SettingOutlined,
} from '@ant-design/icons';
import { createSession, getSession, postLlmConfig, ApiError } from '../api/client';
import { useSseChat, type ChatMessage } from '../hooks/useSseChat';
import { useLlmStatus } from '../hooks/useLlmStatus';
import { MessageList } from '../components/MessageList';
import { VoiceRecorder } from '../components/VoiceRecorder';
import { DatasetUploader } from '../components/DatasetUploader';
import { DecisionAssistantToggle } from '../components/DecisionAssistantToggle';
import { ErrorBanner } from '../components/ErrorBanner';
import {
  CUSTOM_MODEL_VALUE,
  DEFAULT_BASE_URLS,
  getDefaultModel,
  getModelOptions,
  isKnownModel,
  OnboardingCard,
} from '../components/OnboardingCard';
import type { ChoiceAnswer, ChoicePrompt, SkillResult, DatasetSummary, LlmProvider } from '../api/types';

const { Sider, Header, Content } = Layout;
const { Text, Title, Paragraph } = Typography;
const { TextArea } = Input;

const SIDER_WIDTH = 320;

export function ChatPage() {
  const { token } = antdTheme.useToken();

  // ─── LLM config state (Onboarding_Card) ───────────────────────────────
  const {
    configured,
    provider: currentProvider,
    base_url: currentBaseUrl,
    model: currentModel,
    setConfigured,
  } = useLlmStatus();
  const [llmSubmitting, setLlmSubmitting] = useState(false);
  const [llmError, setLlmError] = useState<string | null>(null);

  const handleLlmSubmit = useCallback(async (provider: LlmProvider, apiKey: string, baseUrl: string, model: string) => {
    setLlmSubmitting(true);
    setLlmError(null);
    try {
      await postLlmConfig(provider, apiKey, baseUrl, model);
      setConfigured(provider, baseUrl, model);
    } catch (err) {
      if (err instanceof ApiError) {
        setLlmError(err.payload.message);
      } else {
        setLlmError(err instanceof Error ? err.message : '配置保存失败');
      }
    } finally {
      setLlmSubmitting(false);
    }
  }, [setConfigured]);

  // ─── Settings Drawer state ──────────────────────────────────────────
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsProvider, setSettingsProvider] = useState<LlmProvider>('deepseek');
  const [settingsBaseUrl, setSettingsBaseUrl] = useState('');
  const [settingsModel, setSettingsModel] = useState(getDefaultModel('deepseek'));
  const [settingsCustomModel, setSettingsCustomModel] = useState('');
  const [settingsApiKey, setSettingsApiKey] = useState('');
  const [settingsSubmitting, setSettingsSubmitting] = useState(false);
  const [settingsError, setSettingsError] = useState<string | null>(null);
  const [isSettingsBaseUrlDirty, setIsSettingsBaseUrlDirty] = useState(false);

  useEffect(() => {
    if (settingsOpen) {
      const nextProvider = currentProvider ?? 'deepseek';
      const nextModel = currentModel?.trim() ?? '';
      setSettingsProvider(nextProvider);
      if (currentBaseUrl) {
        setSettingsBaseUrl(currentBaseUrl);
        setIsSettingsBaseUrlDirty(true);
      } else {
        setSettingsBaseUrl(DEFAULT_BASE_URLS[nextProvider]);
        setIsSettingsBaseUrlDirty(false);
      }
      if (nextModel && isKnownModel(nextProvider, nextModel)) {
        setSettingsModel(nextModel);
        setSettingsCustomModel('');
      } else if (nextModel) {
        setSettingsModel(CUSTOM_MODEL_VALUE);
        setSettingsCustomModel(nextModel);
      } else {
        setSettingsModel(getDefaultModel(nextProvider));
        setSettingsCustomModel('');
      }
      setSettingsApiKey('');
      setSettingsError(null);
    }
  }, [settingsOpen, currentProvider, currentBaseUrl, currentModel]);

  const handleSettingsProviderChange = (next: LlmProvider) => {
    setSettingsProvider(next);
    if (!isSettingsBaseUrlDirty) {
      setSettingsBaseUrl(DEFAULT_BASE_URLS[next]);
    }
    setSettingsModel(getDefaultModel(next));
    setSettingsCustomModel('');
  };

  const handleSettingsSubmit = async () => {
    const trimmedKey = settingsApiKey.trim();
    const trimmedUrl = settingsBaseUrl.trim();
    const trimmedModel = settingsModel === CUSTOM_MODEL_VALUE ? settingsCustomModel.trim() : settingsModel.trim();
    if (!trimmedKey || !trimmedUrl || !trimmedModel) {
      setSettingsError('Please fill API Key, API Base URL, and model');
      return;
    }
    setSettingsSubmitting(true);
    setSettingsError(null);
    try {
      await postLlmConfig(settingsProvider, trimmedKey, trimmedUrl, trimmedModel);
      setConfigured(settingsProvider, trimmedUrl, trimmedModel);
      setSettingsOpen(false);
    } catch (err) {
      if (err instanceof ApiError) {
        setSettingsError(err.payload.message);
      } else {
        setSettingsError(err instanceof Error ? err.message : '配置测试或保存失败');
      }
    } finally {
      setSettingsSubmitting(false);
    }
  };

  // ─── Session lifecycle ────────────────────────────────────────────────
  const [sessionId, setSessionId] = useState<string>('');
  const [sessionLoading, setSessionLoading] = useState(true);
  const [sessionError, setSessionError] = useState<string | null>(null);
  const [decisionAssistant, setDecisionAssistant] = useState(true);
  const [isArchived, setIsArchived] = useState(false);

  // ─── Chat hook ────────────────────────────────────────────────────────
  const { messages, setMessages, sendMessage, status, error, isStreaming } = useSseChat(sessionId);

  useEffect(() => {
    let cancelled = false;

    const urlParams = new URLSearchParams(window.location.search);
    const urlSessionId = urlParams.get('session_id');

    const loadSessionPromise = urlSessionId
      ? getSession(urlSessionId)
      : createSession();

    loadSessionPromise
      .then((session) => {
        if (cancelled) return;
        setSessionId(session.id);
        setDecisionAssistant(session.settings.decision_assistant);
        setDatasets(session.datasets || []);
        setIsArchived(session.status === 'Archived');

        // Convert backend messages to frontend ChatMessages
        const chatMessages: ChatMessage[] = (session.messages || []).map((msg) => {
          if ('User' in msg) {
            const userMsg = msg.User;
            let content = '';
            if ('Text' in userMsg.content) {
              content = userMsg.content.Text;
            } else if ('AudioTranscript' in userMsg.content) {
              content = userMsg.content.AudioTranscript.text;
            } else if ('ChoiceAnswer' in userMsg.content) {
              const answer = userMsg.content.ChoiceAnswer;
              const parts: string[] = [];
              if (answer.options.length > 0) {
                parts.push(`已选择: ${answer.options.join(', ')}`);
              }
              if (answer.custom_text) {
                parts.push(answer.custom_text);
              }
              content = parts.join(' | ') || '继续';
            }
            return {
              id: userMsg.id,
              role: 'user',
              content,
              timestamp: new Date(userMsg.created_at),
            };
          } else {
            const agentMsg = msg.Agent;
            let content = '';
            let choicePrompt: ChoicePrompt | undefined;
            let skillResult: SkillResult | undefined;
            let interpretation: string | undefined;

            for (const block of agentMsg.blocks) {
              if ('Text' in block) {
                content += (content ? '\n' : '') + block.Text;
              } else if ('ChoicePrompt' in block) {
                choicePrompt = block.ChoicePrompt;
              } else if ('SkillResult' in block) {
                skillResult = block.SkillResult.result;
              } else if ('Interpretation' in block) {
                interpretation = block.Interpretation;
              }
            }

            return {
              id: agentMsg.id,
              role: 'agent',
              content,
              choicePrompt,
              skillResult,
              interpretation,
              timestamp: new Date(agentMsg.created_at),
            };
          }
        });

        setMessages(chatMessages);
        setSessionLoading(false);
      })
      .catch((err) => {
        if (cancelled) return;
        setSessionError(err instanceof Error ? err.message : '初始化会话失败');
        setSessionLoading(false);
      });

    return () => {
      cancelled = true;
    };
  }, [setMessages]);

  // ─── Datasets ─────────────────────────────────────────────────────────
  const [datasets, setDatasets] = useState<DatasetSummary[]>([]);
  const [uploaderOpen, setUploaderOpen] = useState(false);
  const handleDatasetUploaded = useCallback((summary: DatasetSummary) => {
    setDatasets((prev) => [...prev, summary]);
  }, []);

  // ─── Input box ────────────────────────────────────────────────────────
  const [inputValue, setInputValue] = useState('');
  const lastSentRef = useRef<string>('');
  const textAreaRef = useRef<{ resizableTextArea?: { textArea: HTMLTextAreaElement } }>(null);

  const handleSend = useCallback(
    (overrideText?: string) => {
      const text = (overrideText ?? inputValue).trim();
      if (!text || isStreaming) return;
      lastSentRef.current = text;
      sendMessage(text);
      if (overrideText === undefined) {
        setInputValue('');
      }
    },
    [inputValue, isStreaming, sendMessage],
  );

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) {
        e.preventDefault();
        handleSend();
      }
    },
    [handleSend],
  );

  // ─── Voice transcript handler ─────────────────────────────────────────
  const handleVoiceTranscript = useCallback(
    (text: string) => {
      handleSend(text);
    },
    [handleSend],
  );

  // ─── Choice prompt answer handler ─────────────────────────────────────
  const handleChoiceSubmit = useCallback(
    (answer: ChoiceAnswer) => {
      // Convert the answer into a text message for the orchestrator.
      const parts: string[] = [];
      if (answer.options.length > 0) {
        parts.push(`已选择: ${answer.options.join(', ')}`);
      }
      if (answer.custom_text) {
        parts.push(answer.custom_text);
      }
      const text = parts.join(' | ') || '继续';
      handleSend(text);
    },
    [handleSend],
  );

  // ─── Retry handler (LLM_UNAVAILABLE) ──────────────────────────────────
  const handleRetry = useCallback(() => {
    if (lastSentRef.current) {
      handleSend(lastSentRef.current);
    }
  }, [handleSend]);

  // ─── Render: loading ──────────────────────────────────────────────────
  if (sessionLoading) {
    return (
      <div
        style={{
          display: 'flex',
          justifyContent: 'center',
          alignItems: 'center',
          height: '100vh',
          background: token.colorBgLayout,
        }}
      >
        <Spin
          indicator={<LoadingOutlined style={{ fontSize: 36 }} spin />}
          tip={<span style={{ marginLeft: 8 }}>正在创建会话...</span>}
        />
      </div>
    );
  }

  // ─── Render: session error ────────────────────────────────────────────
  if (sessionError) {
    return (
      <Result
        status="error"
        title="会话创建失败"
        subTitle={sessionError}
        extra={
          <Button type="primary" onClick={() => window.location.reload()}>
            重新加载
          </Button>
        }
      />
    );
  }

  // ─── Render: main layout ──────────────────────────────────────────────
  return (
    <Layout style={{ height: '100vh' }}>
      {/* Sidebar — datasets + history */}
      <Sider
        width={SIDER_WIDTH}
        style={{
          background: token.colorBgContainer,
          borderRight: `1px solid ${token.colorBorderSecondary}`,
          padding: 16,
          overflowY: 'auto',
        }}
        breakpoint="md"
        collapsedWidth={0}
      >
        <Space direction="vertical" size={16} style={{ width: '100%' }}>
          <Title level={4} style={{ margin: 0 }}>
            <ThunderboltOutlined style={{ color: token.colorPrimary }} /> Stats 智能分析
          </Title>

          <Button
            type="dashed"
            block
            icon={<DatabaseOutlined />}
            onClick={() => setUploaderOpen(true)}
            disabled={isArchived}
          >
            上传数据集
          </Button>

          {/* Uploaded datasets list */}
          <div>
            <Text strong style={{ fontSize: 13 }}>
              当前数据集 ({datasets.length})
            </Text>
            {datasets.length === 0 ? (
              <Empty
                image={Empty.PRESENTED_IMAGE_SIMPLE}
                description="尚未上传"
                style={{ marginTop: 12 }}
              />
            ) : (
              <Space direction="vertical" size={6} style={{ width: '100%', marginTop: 8 }}>
                {datasets.map((ds) => (
                  <div
                    key={ds.dataset_id}
                    style={{
                      padding: '6px 10px',
                      background: token.colorFillTertiary,
                      borderRadius: 6,
                      fontSize: 12,
                    }}
                  >
                    <Text strong style={{ fontSize: 12 }}>
                      {ds.file_name}
                    </Text>
                    <div style={{ marginTop: 4 }}>
                      <Tag color="blue">{ds.row_count} 行</Tag>
                      <Tag>{ds.columns.length} 列</Tag>
                    </div>
                  </div>
                ))}
              </Space>
            )}
          </div>
        </Space>
      </Sider>

      <Layout>
        {/* Header */}
        <Header
          style={{
            background: token.colorBgContainer,
            borderBottom: `1px solid ${token.colorBorderSecondary}`,
            padding: '0 24px',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            height: 56,
            lineHeight: '56px',
          }}
        >
          <ConnectionIndicator status={status} />
          <Space size={16} align="center">
            <DecisionAssistantToggle
              value={decisionAssistant}
              onChange={setDecisionAssistant}
              sessionId={sessionId}
            />
            {currentModel ? <Tag color="blue">{currentModel}</Tag> : null}
            <Button
              type="text"
              icon={<SettingOutlined />}
              onClick={() => setSettingsOpen(true)}
              title="API 设置"
              aria-label="API 设置"
            />
          </Space>
        </Header>

        {/* Message area */}
        <Content
          style={{
            display: 'flex',
            flexDirection: 'column',
            padding: 16,
            background: token.colorBgLayout,
            overflow: 'hidden',
          }}
        >
          {isArchived && (
            <Alert
              message="此会话已归档 (只读模式)"
              description="该会话处于只读状态。您无法再发送消息、上传数据集或进行选择。"
              type="warning"
              showIcon
              style={{ marginBottom: 16, maxWidth: 880, width: '100%', marginLeft: 'auto', marginRight: 'auto' }}
            />
          )}
          <div
            style={{
              flex: 1,
              overflowY: 'auto',
              paddingRight: 8,
              maxWidth: 880,
              width: '100%',
              margin: '0 auto',
            }}
          >
            <MessageList messages={messages} onChoiceSubmit={handleChoiceSubmit} disabled={isArchived} />
            <ErrorBanner error={error} onRetry={handleRetry} />
          </div>

          {/* Input bar */}
          <div
            style={{
              maxWidth: 880,
              width: '100%',
              margin: '12px auto 0',
              padding: 12,
              background: token.colorBgContainer,
              borderRadius: token.borderRadiusLG,
              boxShadow: token.boxShadowTertiary,
              border: `1px solid ${token.colorBorderSecondary}`,
            }}
          >
            <div style={{ display: 'flex', alignItems: 'flex-end', gap: 8 }}>
              <VoiceRecorder
                sessionId={sessionId}
                onTranscript={handleVoiceTranscript}
                disabled={isStreaming || isArchived}
              />
              <TextArea
                ref={textAreaRef as never}
                value={inputValue}
                onChange={(e) => setInputValue(e.target.value)}
                onKeyDown={handleKeyDown}
                placeholder={isArchived ? "当前会话已归档，无法发送消息" : "输入统计分析问题，Enter 发送，Shift+Enter 换行"}
                autoSize={{ minRows: 1, maxRows: 6 }}
                disabled={isStreaming || isArchived}
                style={{ flex: 1, resize: 'none' }}
                aria-label="消息输入框"
              />
              <Button
                type="primary"
                icon={isStreaming ? <LoadingOutlined spin /> : <SendOutlined />}
                onClick={() => handleSend()}
                disabled={!inputValue.trim() || isStreaming || isArchived}
                size="large"
              >
                发送
              </Button>
            </div>
            <div
              style={{
                marginTop: 6,
                fontSize: 11,
                color: token.colorTextTertiary,
                textAlign: 'right',
              }}
            >
              结果由 AI 生成，请结合专业判断
            </div>
          </div>
        </Content>
      </Layout>

      {/* Dataset uploader drawer */}
      <Drawer
        title="上传数据集"
        placement="right"
        width={420}
        open={uploaderOpen}
        onClose={() => setUploaderOpen(false)}
      >
        <DatasetUploader
          sessionId={sessionId}
          onUploadComplete={(summary) => {
            handleDatasetUploaded(summary);
          }}
        />
      </Drawer>

      {/* Onboarding overlay: blocks UI until LLM is configured */}
      {!configured && (
        <OnboardingCard
          onSubmit={handleLlmSubmit}
          submitting={llmSubmitting}
          errorMessage={llmError}
        />
      )}

      {/* API Settings Drawer */}
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
            loading={settingsSubmitting}
            disabled={
              !settingsApiKey.trim() ||
              !settingsBaseUrl.trim() ||
              !(settingsModel === CUSTOM_MODEL_VALUE ? settingsCustomModel.trim() : settingsModel.trim())
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
              value={settingsProvider}
              onChange={handleSettingsProviderChange}
              options={[
                { value: 'deepseek', label: 'DeepSeek' },
                { value: 'openai', label: 'OpenAI' },
              ]}
              disabled={settingsSubmitting}
              style={{ width: '100%' }}
              aria-label="LLM 提供商"
            />
          </label>

          <label style={{ display: 'block' }}>
            <Paragraph style={{ marginBottom: 4, fontSize: 13 }}>API Base URL</Paragraph>
            <Input
              value={settingsBaseUrl}
              onChange={(e) => {
                setSettingsBaseUrl(e.target.value);
                setIsSettingsBaseUrlDirty(true);
              }}
              placeholder={settingsProvider === 'deepseek' ? 'https://api.deepseek.com/v1' : 'https://api.openai.com/v1'}
              disabled={settingsSubmitting}
              aria-label="API Base URL"
            />
          </label>

          <label style={{ display: 'block' }}>
            <Paragraph style={{ marginBottom: 4, fontSize: 13 }}>Model</Paragraph>
            <Select<string>
              value={settingsModel}
              onChange={setSettingsModel}
              options={getModelOptions(settingsProvider)}
              disabled={settingsSubmitting}
              style={{ width: '100%' }}
              aria-label="LLM model"
            />
          </label>

          {settingsModel === CUSTOM_MODEL_VALUE ? (
            <label style={{ display: 'block' }}>
              <Paragraph style={{ marginBottom: 4, fontSize: 13 }}>Custom model</Paragraph>
              <Input
                value={settingsCustomModel}
                onChange={(e) => setSettingsCustomModel(e.target.value)}
                placeholder={settingsProvider === 'deepseek' ? 'deepseek-chat' : 'gpt-5.4'}
                disabled={settingsSubmitting}
                aria-label="Custom LLM model"
              />
            </label>
          ) : null}

          <label style={{ display: 'block' }}>
            <Paragraph style={{ marginBottom: 4, fontSize: 13 }}>API Key</Paragraph>
            <Input.Password
              value={settingsApiKey}
              onChange={(e) => setSettingsApiKey(e.target.value)}
              placeholder="输入新的 API Key"
              autoComplete="off"
              disabled={settingsSubmitting}
              aria-label="API Key"
            />
          </label>

          {settingsError ? (
            <Alert
              type="error"
              showIcon
              role="alert"
              message={
                <span style={{ color: '#cf1322', fontSize: 13 }}>{settingsError}</span>
              }
            />
          ) : null}
        </Space>
      </Drawer>
    </Layout>
  );
}

// ─── Connection status indicator ─────────────────────────────────────────

function ConnectionIndicator({ status }: { status: string }) {
  const config: Record<string, { color: string; text: string }> = {
    idle: { color: '#52c41a', text: '已连接' },
    connecting: { color: '#faad14', text: '连接中...' },
    streaming: { color: '#1677ff', text: '接收中...' },
    error: { color: '#ff4d4f', text: '连接异常' },
  };

  const fallback = { color: '#52c41a', text: '已连接' };
  const entry = config[status] ?? fallback;
  const { color, text } = entry;

  return (
    <Space size={6} align="center">
      <span
        style={{
          display: 'inline-block',
          width: 8,
          height: 8,
          borderRadius: '50%',
          background: color,
          boxShadow: `0 0 6px ${color}`,
        }}
      />
      <Text type="secondary" style={{ fontSize: 12 }}>
        {text}
      </Text>
    </Space>
  );
}

export default ChatPage;

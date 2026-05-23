import React, { useState, useEffect, useCallback, useMemo, useRef } from 'react';
import {
  Layout,
  Steps,
  Button,
  Space,
  Typography,
  Card,
  Drawer,
  Empty,
  Tag,
  Result,
  Input,
  theme as antdTheme,
  Spin,
} from 'antd';
import {
  DatabaseOutlined,
  SettingOutlined,
  LoadingOutlined,
  FileTextOutlined,
  MessageOutlined,
  DoubleRightOutlined,
  DoubleLeftOutlined,
  ThunderboltOutlined,
  SendOutlined,
  UndoOutlined,
} from '@ant-design/icons';

import { createSession, postLlmConfig, ApiError } from '../api/client';
import { useSseChat } from '../hooks/useSseChat';
import { useLlmStatus } from '../hooks/useLlmStatus';
import { DatasetUploader } from '../components/DatasetUploader';
import { DecisionAssistantToggle } from '../components/DecisionAssistantToggle';
import { ErrorBanner } from '../components/ErrorBanner';
import { OnboardingCard } from '../components/OnboardingCard';
import { VoiceRecorder } from '../components/VoiceRecorder';
import { MessageList } from '../components/MessageList';

import { DataExplorer } from '../components/DataExplorer';
import { AnalysisConfigurator } from '../components/AnalysisConfigurator';
import { EngineRunner } from '../components/EngineRunner';
import { ThreeLineTable } from '../components/ThreeLineTable';
import { StatsChartRenderer } from '../components/StatsChartRenderer';
import type { DatasetSummary, LlmProvider, ChoiceAnswer } from '../api/types';

const { Sider, Header, Content } = Layout;
const { Text, Title, Paragraph } = Typography;
const { TextArea } = Input;

const SIDER_WIDTH = 280;

export function WorkflowPage() {
  const { token } = antdTheme.useToken();

  // ─── LLM config state (Onboarding Card) ───────────────────────────────
  const { configured, setConfigured } = useLlmStatus();
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

  // ─── Session lifecycle ────────────────────────────────────────────────
  const [sessionId, setSessionId] = useState<string>('');
  const [sessionLoading, setSessionLoading] = useState(true);
  const [sessionError, setSessionError] = useState<string | null>(null);
  const [decisionAssistant, setDecisionAssistant] = useState(true);

  useEffect(() => {
    let cancelled = false;
    createSession()
      .then((session) => {
        if (cancelled) return;
        setSessionId(session.id);
        setDecisionAssistant(session.settings.decision_assistant);
        setSessionLoading(false);
      })
      .catch((err) => {
        if (cancelled) return;
        setSessionError(err instanceof Error ? err.message : '创建会话失败');
        setSessionLoading(false);
      });
    return () => {
      cancelled = cancelled;
    };
  }, []);

  // ─── Chat hook ────────────────────────────────────────────────────────
  const { messages, sendMessage, status, error, isStreaming } = useSseChat(sessionId);

  // ─── Datasets ─────────────────────────────────────────────────────────
  const [datasets, setDatasets] = useState<DatasetSummary[]>([]);
  const [selectedDataset, setSelectedDataset] = useState<DatasetSummary | null>(null);
  const [uploaderOpen, setUploaderOpen] = useState(false);

  const handleDatasetUploaded = useCallback((summary: DatasetSummary) => {
    setDatasets((prev) => {
      // Avoid duplicate datasets in local state list
      if (prev.some((d) => d.dataset_id === summary.dataset_id)) return prev;
      return [...prev, summary];
    });
    setSelectedDataset(summary);
    setUploaderOpen(false);
  }, []);

  // ─── Steps flow ────────────────────────────────────────────────────────
  const [currentStep, setCurrentStep] = useState(0);

  // Auto transition from Step 3 (Execution) to Step 4 (Report) when completed
  useEffect(() => {
    if (currentStep === 2 && status === 'idle' && !isStreaming && messages.length > 0) {
      // Check if we have received an agent response with a result or text
      const latestMessage = messages[messages.length - 1];
      if (latestMessage && latestMessage.role === 'agent') {
        setCurrentStep(3);
      }
    }
  }, [currentStep, status, isStreaming, messages]);

  // Extract the latest analytical result and interpretation
  const latestAnalysisResult = useMemo(() => {
    // Find the latest agent message containing a skill result
    for (let i = messages.length - 1; i >= 0; i--) {
      const msg = messages[i];
      if (msg && msg.role === 'agent' && msg.skillResult) {
        return msg.skillResult;
      }
    }
    return null;
  }, [messages]);

  const latestAgentMessage = useMemo(() => {
    for (let i = messages.length - 1; i >= 0; i--) {
      const msg = messages[i];
      if (msg && msg.role === 'agent') {
        return msg;
      }
    }
    return null;
  }, [messages]);

  // Extract running/interpretation logs from stream for the progress page
  const runLogs = useMemo(() => {
    const logsList: string[] = [];
    if (messages.length > 0) {
      const latest = messages[messages.length - 1];
      if (latest && latest.role === 'agent') {
        if (latest.content) {
          logsList.push(latest.content.slice(-200));
        }
        if (latest.interpretation) {
          logsList.push(`[解释]: ${latest.interpretation}`);
        }
      }
    }
    return logsList;
  }, [messages]);

  // Handle visual statistical form submission (Transitions into Step 3 Execution)
  const handleConfigSubmit = (compiledPrompt: string) => {
    setCurrentStep(2); // Go to Engine Running step
    sendMessage(compiledPrompt);
  };

  // ─── Diagnosis Chat Area Input Box ────────────────────────────────────
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

  const handleVoiceTranscript = useCallback(
    (text: string) => {
      handleSend(text);
    },
    [handleSend],
  );

  const handleChoiceSubmit = useCallback(
    (answer: ChoiceAnswer) => {
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

  const handleRetry = useCallback(() => {
    if (lastSentRef.current) {
      handleSend(lastSentRef.current);
    }
  }, [handleSend]);

  const startNewAnalysis = () => {
    setCurrentStep(0);
    // Keep the datasets uploaded, but clear the selection to let user choose fresh
    setSelectedDataset(null);
  };

  // ─── Render loaders ──────────────────────────────────────────────────
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
          indicator={<LoadingOutlined style={{ fontSize: 36, color: '#38618c' }} spin />}
          tip={<span style={{ marginLeft: 8, color: '#2b3a4a' }}>正在初始化学术分析工作空间...</span>}
        />
      </div>
    );
  }

  if (sessionError) {
    return (
      <Result
        status="error"
        title="工作空间加载失败"
        subTitle={sessionError}
        extra={
          <Button type="primary" onClick={() => window.location.reload()}>
            重新初始化
          </Button>
        }
      />
    );
  }

  const stepsItems = [
    { title: '数据探索', icon: <DatabaseOutlined /> },
    { title: '分析配置', icon: <SettingOutlined /> },
    { title: '引擎运行', icon: isStreaming ? <LoadingOutlined spin /> : <ThunderboltOutlined /> },
    { title: '结果报告', icon: <FileTextOutlined /> },
    { title: 'AI 诊断咨询', icon: <MessageOutlined /> },
  ];

  return (
    <Layout style={{ height: '100vh' }}>
      {/* Sider — dataset list & controls */}
      <Sider
        width={SIDER_WIDTH}
        style={{
          background: 'rgba(250, 249, 245, 0.95)',
          borderRight: `1px solid ${token.colorBorderSecondary}`,
          padding: 16,
          display: 'flex',
          flexDirection: 'column',
          overflowY: 'auto',
        }}
        breakpoint="md"
        collapsedWidth={0}
      >
        <Space direction="vertical" size={18} style={{ width: '100%' }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: '8px', paddingBottom: '12px', borderBottom: '1px solid rgba(56, 97, 140, 0.1)' }}>
            <ThunderboltOutlined style={{ color: '#38618c', fontSize: '20px' }} />
            <Title level={4} style={{ margin: 0, fontSize: '16px', color: '#2b3a4a' }}>
              Stats 智能科研分析
            </Title>
          </div>

          <Button
            type="primary"
            block
            icon={<UndoOutlined />}
            onClick={startNewAnalysis}
            style={{
              background: '#38618c',
              borderColor: '#38618c',
              borderRadius: '6px',
            }}
          >
            开启新探索
          </Button>

          <Button
            type="dashed"
            block
            icon={<DatabaseOutlined />}
            onClick={() => setUploaderOpen(true)}
            style={{ borderRadius: '6px' }}
          >
            导入新数据集
          </Button>

          {/* Dataset selection section */}
          <div style={{ marginTop: '8px' }}>
            <Text strong style={{ fontSize: '13px', color: '#5a6e85', display: 'block', marginBottom: '8px' }}>
              已载入的数据集 ({datasets.length})
            </Text>
            {datasets.length === 0 ? (
              <Empty
                image={Empty.PRESENTED_IMAGE_SIMPLE}
                description="暂无数据集"
                style={{ marginTop: 12 }}
              />
            ) : (
              <Space direction="vertical" size={8} style={{ width: '100%' }}>
                {datasets.map((ds) => {
                  const isSel = selectedDataset?.dataset_id === ds.dataset_id;
                  return (
                    <div
                      key={ds.dataset_id}
                      onClick={() => {
                        setSelectedDataset(ds);
                        setCurrentStep(0); // Return to explorer when switching datasets
                      }}
                      style={{
                        padding: '10px 12px',
                        background: isSel ? 'rgba(56, 97, 140, 0.08)' : 'rgba(255,255,255,0.5)',
                        border: `1px solid ${isSel ? '#38618c' : 'rgba(0,0,0,0.05)'}`,
                        borderRadius: '8px',
                        cursor: 'pointer',
                        transition: 'all 0.2s',
                      }}
                    >
                      <Text strong style={{ fontSize: '12px', color: isSel ? '#38618c' : '#2b3a4a', display: 'block' }} ellipsis>
                        {ds.file_name}
                      </Text>
                      <div style={{ marginTop: 6, display: 'flex', gap: '4px' }}>
                        <Tag color={isSel ? 'blue' : 'default'} style={{ fontSize: '10px', padding: '0 4px', margin: 0 }}>
                          {ds.row_count} 行
                        </Tag>
                        <Tag style={{ fontSize: '10px', padding: '0 4px', margin: 0 }}>
                          {ds.columns.length} 变量
                        </Tag>
                      </div>
                    </div>
                  );
                })}
              </Space>
            )}
          </div>
        </Space>
      </Sider>

      <Layout>
        {/* Header - contains the workflow steps indicator */}
        <Header
          style={{
            background: 'rgba(250, 249, 245, 0.85)',
            borderBottom: `1px solid ${token.colorBorderSecondary}`,
            padding: '0 24px',
            display: 'flex',
            alignItems: 'center',
            height: 64,
            lineHeight: '64px',
            gap: '24px',
          }}
        >
          <div style={{ flex: 1, minWidth: 0 }}>
            <Steps
              current={currentStep}
              onChange={(step) => {
                // Prevent navigating past configure/explore if no dataset
                if (step > 0 && !selectedDataset) return;
                // Prevent moving to run/report manually unless we already ran it
                if (step >= 2 && messages.length === 0) return;
                setCurrentStep(step);
              }}
              size="small"
              items={stepsItems}
              style={{ padding: '8px 0' }}
            />
          </div>

          <div style={{ display: 'flex', alignItems: 'center', gap: '16px' }}>
            <ConnectionIndicator status={status} />
            <DecisionAssistantToggle
              value={decisionAssistant}
              onChange={setDecisionAssistant}
              sessionId={sessionId}
            />
          </div>
        </Header>

        {/* Main Work Area */}
        <Content
          style={{
            display: 'flex',
            flexDirection: 'column',
            padding: '20px 24px',
            background: 'linear-gradient(135deg, #fdfdfb 0%, #f4f2ea 100%)',
            overflowY: 'auto',
          }}
        >
          <div style={{ flex: 1, width: '100%', maxWidth: 1000, margin: '0 auto' }}>
            {/* Step 1: Data Explorer */}
            {currentStep === 0 && (
              <Space direction="vertical" size={16} style={{ width: '100%' }}>
                {selectedDataset ? (
                  <>
                    <DataExplorer summary={selectedDataset} />
                    <div style={{ display: 'flex', justifyContent: 'flex-end', marginTop: 12 }}>
                      <Button
                        type="primary"
                        size="large"
                        icon={<DoubleRightOutlined />}
                        onClick={() => setCurrentStep(1)}
                        style={{
                          background: '#38618c',
                          borderColor: '#38618c',
                          borderRadius: '6px',
                        }}
                      >
                        下一步：配置分析参数
                      </Button>
                    </div>
                  </>
                ) : (
                  <Card className="glass-panel" style={{ textAlign: 'center', padding: '48px 16px' }}>
                    <DatabaseOutlined style={{ fontSize: 48, color: '#38618c', marginBottom: 16 }} />
                    <Title level={4} style={{ color: '#2b3a4a' }}>欢迎使用 Stats 统计分析工作流</Title>
                    <Paragraph type="secondary" style={{ maxWidth: 460, margin: '0 auto 24px' }}>
                      请先导入您的医疗或学术数据集文件 (.csv, .tsv, .xlsx)，系统将为您生成详尽的变量结构画像。
                    </Paragraph>
                    <div style={{ maxWidth: 360, margin: '0 auto' }}>
                      <DatasetUploader
                        sessionId={sessionId}
                        onUploadComplete={handleDatasetUploaded}
                      />
                    </div>
                  </Card>
                )}
              </Space>
            )}

            {/* Step 2: Configure Parameters */}
            {currentStep === 1 && (
              <Space direction="vertical" size={16} style={{ width: '100%' }}>
                <AnalysisConfigurator
                  summary={selectedDataset}
                  onSubmit={handleConfigSubmit}
                  disabled={isStreaming}
                />
                <div style={{ display: 'flex', justifyContent: 'space-between', marginTop: 12 }}>
                  <Button
                    icon={<DoubleLeftOutlined />}
                    size="large"
                    onClick={() => setCurrentStep(0)}
                    style={{ borderRadius: '6px' }}
                  >
                    返回数据预览
                  </Button>
                </div>
              </Space>
            )}

            {/* Step 3: Computation Engine Running */}
            {currentStep === 2 && (
              <EngineRunner status={status} isStreaming={isStreaming} logs={runLogs} />
            )}

            {/* Step 4: Outcome Academic Report */}
            {currentStep === 3 && (
              <Space direction="vertical" size={20} style={{ width: '100%', paddingBottom: '24px' }}>
                <Card
                  className="glass-panel"
                  title={
                    <Title level={5} style={{ margin: 0, color: '#2b3a4a' }}>
                      <FileTextOutlined style={{ marginRight: '6px', color: '#38618c' }} />
                      分析报告结果
                    </Title>
                  }
                  bodyStyle={{ padding: '20px' }}
                >
                  {/* Text descriptions */}
                  {latestAgentMessage && latestAgentMessage.content && (
                    <Paragraph style={{ fontSize: '14px', lineHeight: 1.7, whiteSpace: 'pre-wrap', color: '#2b3a4a' }}>
                      {latestAgentMessage.content.replace(/\[正在执行:.*?\]/g, '').trim()}
                    </Paragraph>
                  )}

                  {/* Academic Three-Line Table */}
                  {latestAnalysisResult && (
                    <ThreeLineTable skillResult={latestAnalysisResult} />
                  )}

                  {/* Interpretation Cards */}
                  {latestAgentMessage && latestAgentMessage.interpretation && (
                    <Card
                      size="small"
                      style={{
                        marginTop: 18,
                        background: 'rgba(82, 196, 26, 0.04)',
                        borderColor: 'rgba(82, 196, 26, 0.2)',
                        borderRadius: '8px',
                      }}
                      bodyStyle={{ padding: '14px 18px' }}
                    >
                      <Space size={6} align="start" style={{ marginBottom: 6 }}>
                        <span style={{ fontSize: '16px' }}>💡</span>
                        <Text strong style={{ color: '#276749' }}>AI 临床统计解读</Text>
                      </Space>
                      <Paragraph style={{ marginBottom: 0, color: '#2f855a', fontSize: '13px', lineHeight: 1.6 }}>
                        {latestAgentMessage.interpretation}
                      </Paragraph>
                    </Card>
                  )}
                </Card>

                {/* Academic interactive charts */}
                {latestAnalysisResult && (
                  <StatsChartRenderer skillResult={latestAnalysisResult} />
                )}

                <div style={{ display: 'flex', justifyContent: 'space-between', marginTop: 12 }}>
                  <Button
                    icon={<DoubleLeftOutlined />}
                    size="large"
                    onClick={() => setCurrentStep(1)}
                    style={{ borderRadius: '6px' }}
                  >
                    重新配置参数
                  </Button>
                  <Button
                    type="primary"
                    size="large"
                    icon={<MessageOutlined />}
                    onClick={() => setCurrentStep(4)}
                    style={{
                      background: '#38618c',
                      borderColor: '#38618c',
                      borderRadius: '6px',
                    }}
                  >
                    进入 AI 深度问答咨询
                  </Button>
                </div>
              </Space>
            )}

            {/* Step 5: Follow up chat and diagnosis */}
            {currentStep === 4 && (
              <div style={{ display: 'flex', flexDirection: 'column', height: 'calc(100vh - 120px)' }}>
                {/* Chat Message History */}
                <div style={{ flex: 1, overflowY: 'auto', paddingRight: 8, paddingBottom: 16 }}>
                  <MessageList messages={messages} onChoiceSubmit={handleChoiceSubmit} />
                  <ErrorBanner error={error} onRetry={handleRetry} />
                </div>

                {/* Direct question quick bubbles */}
                <div style={{ display: 'flex', gap: '8px', flexWrap: 'wrap', marginBottom: '8px' }}>
                  <Button
                    size="small"
                    onClick={() => handleSend('请从医学统计学的角度详细解释 p 值的临床意义。')}
                    disabled={isStreaming}
                    style={{ fontSize: '12px', background: 'rgba(255,255,255,0.6)', borderRadius: '12px' }}
                  >
                    💬 解释 p 值的临床意义
                  </Button>
                  <Button
                    size="small"
                    onClick={() => handleSend('此模型的变量是否存在多重共线性(Collinearity)？需要剔除哪些？')}
                    disabled={isStreaming}
                    style={{ fontSize: '12px', background: 'rgba(255,255,255,0.6)', borderRadius: '12px' }}
                  >
                    💬 检查多重共线性
                  </Button>
                  <Button
                    size="small"
                    onClick={() => handleSend('根据以上模型拟合效果，如何调整自变量以提升模型稳健性？')}
                    disabled={isStreaming}
                    style={{ fontSize: '12px', background: 'rgba(255,255,255,0.6)', borderRadius: '12px' }}
                  >
                    💬 模型参数优化建议
                  </Button>
                </div>

                {/* Input Bar */}
                <div
                  style={{
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
                      disabled={isStreaming}
                    />
                    <TextArea
                      ref={textAreaRef as never}
                      value={inputValue}
                      onChange={(e) => setInputValue(e.target.value)}
                      onKeyDown={handleKeyDown}
                      placeholder="针对当前统计报告进一步追问，如：'调整协变量后的模型结果？'"
                      autoSize={{ minRows: 1, maxRows: 5 }}
                      disabled={isStreaming}
                      style={{ flex: 1, resize: 'none' }}
                    />
                    <Button
                      type="primary"
                      icon={isStreaming ? <LoadingOutlined spin /> : <SendOutlined />}
                      onClick={() => handleSend()}
                      disabled={!inputValue.trim() || isStreaming}
                      size="large"
                      style={{
                        background: '#38618c',
                        borderColor: '#38618c',
                      }}
                    >
                      发送
                    </Button>
                  </div>
                </div>
              </div>
            )}
          </div>
        </Content>
      </Layout>

      {/* Upload datasets drawer from the sider buttons */}
      <Drawer
        title="导入数据集"
        placement="right"
        width={400}
        open={uploaderOpen}
        onClose={() => setUploaderOpen(false)}
      >
        <DatasetUploader
          sessionId={sessionId}
          onUploadComplete={handleDatasetUploaded}
        />
      </Drawer>

      {/* Onboarding key setup */}
      {!configured && (
        <OnboardingCard
          onSubmit={handleLlmSubmit}
          submitting={llmSubmitting}
          errorMessage={llmError}
        />
      )}
    </Layout>
  );
}

// ─── Connection status indicator ─────────────────────────────────────────

function ConnectionIndicator({ status }: { status: string }) {
  const config: Record<string, { color: string; text: string }> = {
    idle: { color: '#52c41a', text: '在线' },
    connecting: { color: '#faad14', text: '连接中...' },
    streaming: { color: '#38618c', text: '计算流传输中...' },
    error: { color: '#ff4d4f', text: '连接断开' },
  };

  const fallback = { color: '#52c41a', text: '在线' };
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

export default WorkflowPage;

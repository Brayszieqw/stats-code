/**
 * SimpleModeView — 简易模式视图（对齐 MiroFish 参考图 2）。
 *
 * 左侧米色 SimpleSidebar（导航 + 分组历史 + 用量卡片），右侧白色主区：
 *   - 欢迎态（messages.length === 0）：居中 WelcomeHero（大标题 + 圆角输入框）。
 *   - 对话态：上方 MessageList + ErrorBanner，下方 ChatInputBar。
 * 右上角放置低调的 ModeToggle。Archived 时禁用写操作，ModeToggle 仍可用。
 *
 * Validates: Requirements 2.3, 3.1, 3.2, 3.3, 3.4, 3.5, 3.6, 3.7, 9.3, 9.4
 */

import { useEffect, useMemo, useState } from 'react';
import { Layout, Alert, Button, Drawer, Empty, Space, Tag, Typography } from 'antd';
import { SimpleSidebar } from './simple/SimpleSidebar';
import { WelcomeHero } from './simple/WelcomeHero';
import { ModeToggle } from '../components/ModeToggle';
import { MessageList } from '../components/MessageList';
import { ErrorBanner } from '../components/ErrorBanner';
import { ChatInputBar } from '../components/ChatInputBar';
import { DatasetUploader } from '../components/DatasetUploader';
import { VoiceRecorder } from '../components/VoiceRecorder';
import type { SessionController } from '../hooks/useSessionController';
import type { UseSseChatReturn } from '../hooks/useSseChat';
import type { UseSessionListReturn } from '../hooks/useSessionList';
import type { ViewMode } from '../hooks/useModePreference';
import type { ChoiceAnswer, DatasetSummary } from '../api/types';

const { Sider, Content } = Layout;
const { Text } = Typography;

const SIDER_WIDTH = 260;

export interface SimpleModeViewProps {
  controller: SessionController;
  chat: UseSseChatReturn;
  sessionList: UseSessionListReturn;
  mode: ViewMode;
  onModeChange: (m: ViewMode) => void;
  onSend: (text: string) => void;
  onChoiceSubmit: (a: ChoiceAnswer) => void;
  onRetry: () => void;
  onVoiceTranscript: (t: string) => void;
  modelLabel?: string | null;
  onOpenSettings?: () => void;
  onDeleteSession?: (sessionId: string) => void | Promise<void>;
  onPurgeEmptySessions?: () => void | Promise<void>;
}

export function SimpleModeView({
  controller,
  chat,
  sessionList,
  mode,
  onModeChange,
  onSend,
  onChoiceSubmit,
  onRetry,
  onVoiceTranscript,
  modelLabel,
  onOpenSettings,
  onDeleteSession,
  onPurgeEmptySessions,
}: SimpleModeViewProps) {
  const { messages, error, isStreaming } = chat;
  const { isArchived, sessionId, datasets, addDataset } = controller;
  const [datasetDrawerOpen, setDatasetDrawerOpen] = useState(false);
  const [voiceDrawerOpen, setVoiceDrawerOpen] = useState(false);
  const [selectedDatasetId, setSelectedDatasetId] = useState<string | null>(null);

  const isWelcome = messages.length === 0;
  const selectedDataset = useMemo(
    () => datasets.find((dataset) => dataset.dataset_id === selectedDatasetId) ?? null,
    [datasets, selectedDatasetId],
  );

  // Single dataset → auto-select so the picker label is meaningful.
  useEffect(() => {
    if (datasets.length === 1) {
      setSelectedDatasetId(datasets[0]!.dataset_id);
    }
  }, [datasets, sessionId]);

  const handleDatasetUploaded = (summary: DatasetSummary) => {
    addDataset(summary);
    setSelectedDatasetId(summary.dataset_id);
    setDatasetDrawerOpen(false);
    void sessionList.refresh();
  };

  return (
    <Layout className="stats-shell stats-shell--simple">
      <Sider
        className="stats-sidebar"
        width={SIDER_WIDTH}
        style={{ background: '#f7f3ea', borderRight: '1px solid #cbc3b3' }}
        breakpoint="md"
        collapsedWidth={0}
      >
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
          onOpenDatasetUpload={() => setDatasetDrawerOpen(true)}
          onOpenSettings={onOpenSettings}
          onOpenProMode={() => onModeChange('pro')}
          onUseTemplate={onSend}
          onPurgeEmptySessions={onPurgeEmptySessions}
          onDeleteSession={onDeleteSession}
        />
      </Sider>

      <Content className="stats-canvas">
        {/* 右上角模式切换 */}
        <div className="stats-mode-toggle">
          <ModeToggle mode={mode} onChange={onModeChange} />
        </div>

        {isArchived && (
          <Alert
            message="此会话已归档 (只读模式)"
            description="该会话处于只读状态。您无法再发送消息或进行选择。"
            type="warning"
            showIcon
            style={{ margin: '52px auto 0', maxWidth: 760, width: '100%', flex: '0 0 auto' }}
          />
        )}

        {isWelcome ? (
          <div className="stats-welcome" aria-label="欢迎区">
            <div className="stats-welcome__inner">
              <WelcomeHero
                onSend={onSend}
                disabled={isArchived}
                datasets={datasets}
                selectedDatasetId={selectedDatasetId}
                modelLabel={modelLabel}
                onOpenDatasetPicker={() => setDatasetDrawerOpen(true)}
                onOpenSettings={onOpenSettings}
                onOpenVoiceInput={() => setVoiceDrawerOpen(true)}
              />
            </div>
          </div>
        ) : (
          <div className="stats-conversation" aria-label="对话区">
            <div className="stats-conversation__stream" aria-label="消息列表">
              <MessageList messages={messages} onChoiceSubmit={onChoiceSubmit} disabled={isArchived} />
              <ErrorBanner error={error} onRetry={onRetry} />
            </div>

            <div className="stats-conversation__composer">
              <ChatInputBar
                sessionId={sessionId}
                isStreaming={isStreaming}
                isArchived={isArchived}
                onSend={onSend}
                onVoiceTranscript={onVoiceTranscript}
                footer="AI 仅辅助解释 · 统计数值由本机确定性引擎生成"
                datasets={datasets}
                selectedDatasetId={selectedDatasetId}
                modelLabel={modelLabel}
                onOpenDatasetPicker={() => setDatasetDrawerOpen(true)}
                onOpenSettings={onOpenSettings}
              />
            </div>
          </div>
        )}

        <Drawer
          title="选择 / 上传数据集"
          placement="left"
          width={420}
          open={datasetDrawerOpen}
          onClose={() => setDatasetDrawerOpen(false)}
        >
          <Space direction="vertical" size={14} style={{ width: '100%' }}>
            {datasets.length === 0 ? (
              <Empty description="当前会话还没有数据集" />
            ) : (
              <Space direction="vertical" size={8} style={{ width: '100%' }}>
                <Text strong>当前数据集</Text>
                {datasets.map((dataset) => {
                  const selected = selectedDataset?.dataset_id === dataset.dataset_id;
                  return (
                    <button
                      key={dataset.dataset_id}
                      type="button"
                      onClick={() => setSelectedDatasetId(dataset.dataset_id)}
                      style={{
                        width: '100%',
                        border: `1px solid ${selected ? '#38618c' : '#e3e1d8'}`,
                        background: selected ? 'rgba(56,97,140,0.08)' : '#fff',
                        borderRadius: 8,
                        padding: '10px 12px',
                        textAlign: 'left',
                        cursor: 'pointer',
                      }}
                    >
                      <Text strong>{dataset.file_name}</Text>
                      <div style={{ marginTop: 4 }}>
                        <Tag>{dataset.row_count} 行</Tag>
                        <Tag>{dataset.columns.length} 列</Tag>
                      </div>
                    </button>
                  );
                })}
              </Space>
            )}
            {selectedDataset ? (
              <Button onClick={() => setDatasetDrawerOpen(false)} type="primary" block>
                使用 {selectedDataset.file_name}
              </Button>
            ) : null}
            <DatasetUploader sessionId={sessionId} onUploadComplete={handleDatasetUploaded} />
          </Space>
        </Drawer>

        <Drawer
          title="语音输入"
          placement="right"
          width={360}
          open={voiceDrawerOpen}
          onClose={() => setVoiceDrawerOpen(false)}
        >
          <Space direction="vertical" size={12} style={{ width: '100%' }}>
            <Text type="secondary">
              录音完成后会自动把转写文本发送到当前会话；低置信度结果会先让你确认。
            </Text>
            <VoiceRecorder
              sessionId={sessionId}
              disabled={isArchived}
              onTranscript={(text) => {
                onVoiceTranscript(text);
                setVoiceDrawerOpen(false);
              }}
            />
          </Space>
        </Drawer>
      </Content>
    </Layout>
  );
}

export default SimpleModeView;

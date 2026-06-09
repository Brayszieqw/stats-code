/**
 * ProModeView — 专业模式视图（参考 IDE 多面板布局）。
 *
 * AntD Layout（Sider + 嵌套 Layout）组装：
 *   TopBar（顶）/ ExplorerPanel（左 Sider）/ ReportViewer（中上）/
 *   CodePanel（右 Sider）/ AssistantPanel（中下）。
 * 响应式：视口低于 lg(992px) 优先折叠 ExplorerPanel，保留 CodePanel 可见。
 *
 * Validates: Requirements 4.1, 4.2, 4.3, 4.4
 */

import { useEffect, useState } from 'react';
import { Grid, Layout, theme as antdTheme } from 'antd';
import { TopBar } from './pro/TopBar';
import { ExplorerPanel } from './pro/ExplorerPanel';
import { ReportViewer } from './pro/ReportViewer';
import { CodePanel } from './pro/CodePanel';
import { AssistantPanel } from './pro/AssistantPanel';
import { useLatestAnalysis } from '../hooks/useLatestAnalysis';
import type { SessionController } from '../hooks/useSessionController';
import type { UseSseChatReturn } from '../hooks/useSseChat';
import type { ViewMode } from '../hooks/useModePreference';
import type { ChoiceAnswer, DatasetSummary } from '../api/types';

const { Sider, Content } = Layout;
const { useBreakpoint } = Grid;

const EXPLORER_WIDTH = 260;
const CODE_WIDTH = 380;

export interface ProModeViewProps {
  controller: SessionController;
  chat: UseSseChatReturn;
  mode: ViewMode;
  onModeChange: (m: ViewMode) => void;
  onSend: (text: string) => void;
  onChoiceSubmit: (a: ChoiceAnswer) => void;
  onRetry: () => void;
  onVoiceTranscript: (t: string) => void;
  model?: string | null;
  onOpenSettings?: () => void;
}

export function ProModeView({
  controller,
  chat,
  mode,
  onModeChange,
  onSend,
  onChoiceSubmit,
  onRetry,
  onVoiceTranscript,
  model,
  onOpenSettings,
}: ProModeViewProps) {
  const { token } = antdTheme.useToken();
  const screens = useBreakpoint();
  const { sessionId, datasets, isArchived, addDataset } = controller;

  // 选中数据集 + 保留上次画像（取消选中不清空 lastProfiled）。
  const [selectedDataset, setSelectedDataset] = useState<DatasetSummary | null>(null);
  const [lastProfiledDataset, setLastProfiledDataset] = useState<DatasetSummary | null>(null);

  // 响应式：低于 lg 折叠 ExplorerPanel。
  const [explorerCollapsed, setExplorerCollapsed] = useState(false);
  useEffect(() => {
    // screens.lg 为 true 表示 >= 992px。
    setExplorerCollapsed(!screens.lg);
  }, [screens.lg]);

  const { result } = useLatestAnalysis(chat.messages);
  const analysis = result?.analysis ?? null;

  const handleSelect = (ds: DatasetSummary | null) => {
    setSelectedDataset(ds);
    if (ds) setLastProfiledDataset(ds);
  };

  return (
    <Layout style={{ height: '100vh' }}>
      <TopBar
        title="Stats 智能科研分析"
        model={model}
        mode={mode}
        onModeChange={onModeChange}
        onOpenSettings={onOpenSettings}
      />
      <Layout>
        <Sider
          width={EXPLORER_WIDTH}
          collapsible
          collapsed={explorerCollapsed}
          collapsedWidth={0}
          trigger={null}
          breakpoint="lg"
          onBreakpoint={(broken) => setExplorerCollapsed(broken)}
          style={{
            background: token.colorBgContainer,
            borderRight: `1px solid ${token.colorBorderSecondary}`,
            padding: explorerCollapsed ? 0 : 16,
            overflowY: 'auto',
          }}
        >
          <ExplorerPanel
            datasets={datasets}
            sessionId={sessionId}
            selectedDatasetId={selectedDataset?.dataset_id ?? null}
            onSelect={handleSelect}
            onUploadComplete={(s) => {
              addDataset(s);
              handleSelect(s);
            }}
            disabled={isArchived}
          />
        </Sider>

        <Content
          style={{
            display: 'flex',
            flexDirection: 'column',
            padding: 16,
            background: token.colorBgLayout,
            overflow: 'hidden',
          }}
        >
          <div style={{ flex: '1 1 55%', overflowY: 'auto', minHeight: 0 }}>
            <ReportViewer
              messages={chat.messages}
              selectedDataset={selectedDataset ?? lastProfiledDataset}
            />
          </div>
          <div
            style={{
              flex: '1 1 45%',
              minHeight: 0,
              marginTop: 12,
              borderTop: `1px solid ${token.colorBorderSecondary}`,
              paddingTop: 12,
            }}
          >
            <AssistantPanel
              sessionId={sessionId}
              chat={chat}
              isArchived={isArchived}
              onSend={onSend}
              onChoiceSubmit={onChoiceSubmit}
              onRetry={onRetry}
              onVoiceTranscript={onVoiceTranscript}
            />
          </div>
        </Content>

        <Sider
          width={CODE_WIDTH}
          theme="light"
          style={{
            background: token.colorBgContainer,
            borderLeft: `1px solid ${token.colorBorderSecondary}`,
            padding: 16,
            overflowY: 'auto',
          }}
        >
          <CodePanel sessionId={sessionId} analysis={analysis} disabled={isArchived} />
        </Sider>
      </Layout>
    </Layout>
  );
}

export default ProModeView;

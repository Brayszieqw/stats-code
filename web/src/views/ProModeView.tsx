/**
 * ProModeView — 专业模式视图（布局对齐参考图 1 的 IDE 工作台，视觉为 Stats Code
 * 原创，不照搬具体产品）。
 *
 * 结构：TopBar（macOS 窗口栏）/ 左侧活动图标栏 + EXPLORER 文件树 /
 * 中部文档标签页 + ReportViewer / 右侧 CodePanel（R|SAS|Python|SPSS + 运行控制）/
 * 中下 AssistantPanel。响应式：低于 lg(992px) 折叠 EXPLORER，保留 CodePanel。
 *
 * Validates: Requirements 4.1, 4.2, 4.3, 4.4
 */

import { useEffect, useState } from 'react';
import { Grid, Layout } from 'antd';
import {
  FileOutlined,
  SearchOutlined,
  BranchesOutlined,
  PlayCircleOutlined,
  AppstoreOutlined,
  UserOutlined,
  SettingOutlined,
  CloseOutlined,
  FileTextOutlined,
} from '@ant-design/icons';
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

const RAIL_WIDTH = 48;
const EXPLORER_WIDTH = 230;
const CODE_WIDTH = 400;
const PANEL_BG = '#fbfaf7';
const BORDER = '1px solid #e3e1d8';
const PRIMARY = '#38618c';

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

function ActivityRail() {
  const items = [
    { icon: <FileOutlined />, active: true, label: '资源管理器' },
    { icon: <SearchOutlined />, active: false, label: '搜索' },
    { icon: <BranchesOutlined />, active: false, label: '版本' },
    { icon: <PlayCircleOutlined />, active: false, label: '运行' },
    { icon: <AppstoreOutlined />, active: false, label: '扩展' },
  ];
  return (
    <div
      style={{
        width: RAIL_WIDTH,
        background: '#f0eee8',
        borderRight: BORDER,
        display: 'flex',
        flexDirection: 'column',
        alignItems: 'center',
        paddingTop: 10,
        gap: 4,
      }}
    >
      {items.map((it) => (
        <div
          key={it.label}
          title={it.label}
          style={{
            width: 40,
            height: 40,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            fontSize: 18,
            color: it.active ? PRIMARY : '#8a93a0',
            borderLeft: it.active ? `2px solid ${PRIMARY}` : '2px solid transparent',
          }}
        >
          {it.icon}
        </div>
      ))}
      <div style={{ flex: 1 }} />
      <div style={{ width: 40, height: 40, display: 'flex', alignItems: 'center', justifyContent: 'center', color: '#8a93a0', fontSize: 18 }}>
        <UserOutlined />
      </div>
      <div style={{ width: 40, height: 40, display: 'flex', alignItems: 'center', justifyContent: 'center', color: '#8a93a0', fontSize: 18 }}>
        <SettingOutlined />
      </div>
    </div>
  );
}

function DocumentTab({ title }: { title: string }) {
  return (
    <div style={{ display: 'flex', height: 36, background: '#f0eee8', borderBottom: BORDER, alignItems: 'stretch' }}>
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 8,
          padding: '0 14px',
          background: '#fff',
          borderRight: BORDER,
          borderTop: `2px solid ${PRIMARY}`,
          fontSize: 13,
          color: '#2b3a4a',
        }}
      >
        <FileTextOutlined style={{ color: PRIMARY, fontSize: 13 }} />
        {title}
        <CloseOutlined style={{ fontSize: 10, color: '#9aa7b4' }} />
      </div>
    </div>
  );
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
  const screens = useBreakpoint();
  const { sessionId, datasets, isArchived, addDataset } = controller;

  const [selectedDataset, setSelectedDataset] = useState<DatasetSummary | null>(null);
  const [lastProfiledDataset, setLastProfiledDataset] = useState<DatasetSummary | null>(null);

  const [explorerCollapsed, setExplorerCollapsed] = useState(false);
  useEffect(() => {
    setExplorerCollapsed(!screens.lg);
  }, [screens.lg]);

  const { result } = useLatestAnalysis(chat.messages);
  const analysis = result?.analysis ?? null;

  const handleSelect = (ds: DatasetSummary | null) => {
    setSelectedDataset(ds);
    if (ds) setLastProfiledDataset(ds);
  };

  const docTitle = (selectedDataset ?? lastProfiledDataset)?.file_name ?? '分析报告';

  return (
    <Layout style={{ height: '100vh' }}>
      <TopBar title="MediStat 工作台 | 患者数据分析" model={model} mode={mode} onModeChange={onModeChange} onOpenSettings={onOpenSettings} />
      <Layout style={{ background: PANEL_BG }}>
        {/* 活动图标栏 */}
        <div style={{ display: 'flex' }}>
          <ActivityRail />
        </div>

        {/* EXPLORER 文件树 */}
        <Sider
          width={EXPLORER_WIDTH}
          collapsible
          collapsed={explorerCollapsed}
          collapsedWidth={0}
          trigger={null}
          breakpoint="lg"
          onBreakpoint={(broken) => setExplorerCollapsed(broken)}
          style={{ background: PANEL_BG, borderRight: BORDER, overflowY: 'auto' }}
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

        {/* 中部：文档标签 + 报告 + 助手 */}
        <Content style={{ display: 'flex', flexDirection: 'column', background: '#fff', overflow: 'hidden' }}>
          <DocumentTab title={docTitle} />
          <div style={{ flex: '1 1 56%', overflowY: 'auto', minHeight: 0, padding: 20 }}>
            <ReportViewer messages={chat.messages} selectedDataset={selectedDataset ?? lastProfiledDataset} />
          </div>
          <div
            style={{
              flex: '1 1 44%',
              minHeight: 0,
              borderTop: BORDER,
              background: PANEL_BG,
              padding: 12,
            }}
          >
            <div style={{ fontSize: 12, color: '#6a7a8c', marginBottom: 8, fontWeight: 600 }}>
              AI 助手 · Stats 分析顾问
            </div>
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

        {/* 右侧代码面板 */}
        <Sider width={CODE_WIDTH} theme="light" style={{ background: PANEL_BG, borderLeft: BORDER, overflowY: 'auto', padding: 12 }}>
          <CodePanel sessionId={sessionId} analysis={analysis} disabled={isArchived} />
        </Sider>
      </Layout>
    </Layout>
  );
}

export default ProModeView;

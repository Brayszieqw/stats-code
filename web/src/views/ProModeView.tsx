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

import { useEffect, useState } from 'react';
import { Grid, Layout, Button, Typography, Tag } from 'antd';
import { UploadOutlined, FileTextOutlined, CloseOutlined } from '@ant-design/icons';
import { TopBar } from './pro/TopBar';
import { SimpleSidebar } from './simple/SimpleSidebar';
import { ReportViewer } from './pro/ReportViewer';
import { CodePanel } from './pro/CodePanel';
import { AssistantPanel } from './pro/AssistantPanel';
import { DatasetUploader } from '../components/DatasetUploader';
import { useLatestAnalysis } from '../hooks/useLatestAnalysis';
import { Drawer } from 'antd';
import type { SessionController } from '../hooks/useSessionController';
import type { UseSseChatReturn } from '../hooks/useSseChat';
import type { UseSessionListReturn } from '../hooks/useSessionList';
import type { ViewMode } from '../hooks/useModePreference';
import type { ChoiceAnswer, DatasetSummary } from '../api/types';

const { Sider, Content } = Layout;
const { useBreakpoint } = Grid;
const { Text } = Typography;

const SIDEBAR_WIDTH = 240;
const CODE_WIDTH = 400;
const PANEL_BG = '#fbfaf7';
const BORDER = '1px solid #e3e1d8';
const PRIMARY = '#38618c';

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
  onOpenSettings?: () => void;
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
  sessionList,
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
  const [uploaderOpen, setUploaderOpen] = useState(false);

  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  useEffect(() => {
    setSidebarCollapsed(!screens.lg);
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
        {/* 左侧：与简易模式一致的 Stats 智能分析导航 */}
        <Sider
          width={SIDEBAR_WIDTH}
          collapsible
          collapsed={sidebarCollapsed}
          collapsedWidth={0}
          trigger={null}
          breakpoint="lg"
          onBreakpoint={(broken) => setSidebarCollapsed(broken)}
          style={{ background: '#f7f6f3', borderRight: BORDER, overflowY: 'auto' }}
        >
          <SimpleSidebar
            sessionList={sessionList}
            activeSessionId={sessionId}
            onNewSession={() => {
              void controller.startNewSession();
            }}
            onSelectSession={(sid) => {
              void controller.loadSession(sid);
            }}
          />
        </Sider>

        {/* 中部：文档标签 + 数据集条 + 报告 + 助手 */}
        <Content style={{ display: 'flex', flexDirection: 'column', background: '#fff', overflow: 'hidden' }}>
          <DocumentTab title={docTitle} />

          {/* 数据集工具条 */}
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 10,
              padding: '8px 16px',
              borderBottom: BORDER,
              background: PANEL_BG,
              flexWrap: 'wrap',
            }}
          >
            <Button
              size="small"
              icon={<UploadOutlined />}
              onClick={() => setUploaderOpen(true)}
              disabled={isArchived}
              aria-label="上传数据集"
            >
              上传数据集
            </Button>
            <Text type="secondary" style={{ fontSize: 12 }}>
              已载入 {datasets.length}
            </Text>
            {datasets.map((ds) => {
              const isSel = (selectedDataset?.dataset_id ?? null) === ds.dataset_id;
              return (
                <Tag.CheckableTag
                  key={ds.dataset_id}
                  checked={isSel}
                  onChange={() => {
                    if (isArchived) return;
                    handleSelect(isSel ? null : ds);
                  }}
                  style={{
                    border: `1px solid ${isSel ? PRIMARY : '#d9d9d9'}`,
                    padding: '2px 10px',
                    fontSize: 12,
                  }}
                  aria-label={`数据集: ${ds.file_name}`}
                >
                  {ds.file_name} · {ds.row_count}行
                </Tag.CheckableTag>
              );
            })}
          </div>

          <div style={{ flex: '1 1 56%', overflowY: 'auto', minHeight: 0, padding: 20 }}>
            <ReportViewer messages={chat.messages} selectedDataset={selectedDataset ?? lastProfiledDataset} />
          </div>
          <div style={{ flex: '1 1 44%', minHeight: 0, borderTop: BORDER, background: PANEL_BG, padding: 12 }}>
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

      {/* 上传数据集抽屉 */}
      <Drawer title="上传数据集" placement="left" width={420} open={uploaderOpen} onClose={() => setUploaderOpen(false)}>
        <DatasetUploader
          sessionId={sessionId}
          onUploadComplete={(s) => {
            addDataset(s);
            handleSelect(s);
            setUploaderOpen(false);
          }}
        />
      </Drawer>
    </Layout>
  );
}

export default ProModeView;

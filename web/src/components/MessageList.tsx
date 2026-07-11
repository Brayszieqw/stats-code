/**
 * MessageList — 渲染聊天消息列表（ChatGPT/Claude 风格气泡）
 *
 * 根据消息角色（user / agent）渲染不同样式的消息气泡。
 * Agent 消息支持：流式文本、SkillResult 表格、Interpretation 解读卡片、
 *                 ChoicePrompt 结构化选择题、AnalysisResultView 信任凭证层。
 *
 * Validates: Requirements 1.1, 3.1, 3.2, 3.3, 3.5, 3.7, 3.8, 7.5
 */

import { useEffect, useRef, useState } from 'react';
import { Button, Card, Table, Tag, Typography, Space, theme as antdTheme } from 'antd';
import {
  AreaChartOutlined,
  BulbOutlined,
  CheckCircleOutlined,
  FileTextOutlined,
  RobotOutlined,
  UserOutlined,
} from '@ant-design/icons';
import type { ChatMessage } from '../hooks/useSseChat';
import type { SkillResult, ChoiceAnswer } from '../api/types';
import { ChoicePromptCard } from './ChoicePromptCard';
import { ThreeLineTable } from './ThreeLineTable';
import { StatsChartRenderer } from './StatsChartRenderer';
import { RiskSignalTags } from './RiskSignalTags';
import { ErrorBoundary } from './ErrorBoundary';
import { shouldMountAnalysisResultView } from '../lib/analysisResultMount';
import { AnalysisResultView } from './AnalysisResultView';
import { useCoverageMatrix } from '../lib/coverageMatrixContext';

const { Text, Paragraph } = Typography;

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface MessageListProps {
  messages: ChatMessage[];
  onChoiceSubmit?: (answer: ChoiceAnswer) => void;
  disabled?: boolean;
  /** Pro workspace keeps full tables/charts in the artifact pane. */
  resultPresentation?: 'inline' | 'reference';
  onOpenResult?: (view: 'report' | 'chart') => void;
}

function hasVisibleMessageContent(message: ChatMessage): boolean {
  return (
    message.content.trim().length > 0 ||
    Boolean(message.choicePrompt) ||
    Boolean(message.skillResult) ||
    Boolean(message.interpretation)
  );
}

// ---------------------------------------------------------------------------
// Sub-components
// ---------------------------------------------------------------------------

function GenericKVTable({ payload }: { payload: any }) {
  const entries =
    payload && typeof payload === 'object' && !Array.isArray(payload)
      ? Object.entries(payload as Record<string, unknown>)
      : [];

  const columns = [
    { title: '指标', dataIndex: 'key', key: 'key', width: '40%' },
    {
      title: '值',
      dataIndex: 'value',
      key: 'value',
      render: (val: unknown) => {
        if (val === null || val === undefined) return '-';
        if (typeof val === 'number') {
          return Number.isInteger(val) ? String(val) : val.toFixed(4);
        }
        if (typeof val === 'string') return val;
        return JSON.stringify(val);
      },
    },
  ];

  const dataSource = entries.map(([key, value]) => ({ key, value }));

  if (dataSource.length === 0) return null;

  return (
    <Table
      columns={columns}
      dataSource={dataSource}
      pagination={false}
      size="small"
      style={{ marginBottom: 8 }}
    />
  );
}

/**
 * Prompt the user for a file-save destination via the File System Access API
 * (`showSaveFilePicker`). Falls back to a generated filename when the API is
 * unavailable or the user cancels.
 *
 * Returns the chosen path string (or a default filename for download-link
 * fallback environments). Returns `null` when the user cancels the dialog.
 *
 * Exported for use by the export flow and component tests (Requirement 3.8).
 */
export async function pickSnapshotDestination(runId: string): Promise<string | null> {
  // Use the File System Access API when available (Chromium-based browsers)
  if ('showSaveFilePicker' in window) {
    try {
      const handle = await (window as any).showSaveFilePicker({
        suggestedName: `snapshot-${runId}.zip`,
        types: [
          {
            description: 'Audit Snapshot',
            accept: { 'application/zip': ['.zip'] },
          },
        ],
      });
      return handle.name;
    } catch {
      // User cancelled the dialog
      return null;
    }
  }
  // Fallback: use a default filename (the server will produce a download)
  return `snapshot-${runId}.zip`;
}

function SkillResultView({ result }: { result: SkillResult }) {
  const { payload, risk_signals, analysis } = result;
  const { matrix } = useCoverageMatrix();
  const [snapshotDestination, setSnapshotDestination] = useState<string>('');

  const isStructuredTable =
    payload &&
    typeof payload === 'object' &&
    (('rows' in payload && 'group_levels' in payload) ||
     ('coefficients' in payload && Array.isArray(payload.coefficients)) ||
     // Engine tableone: groups[] with continuous/categorical summaries
     (Array.isArray((payload as { groups?: unknown }).groups) &&
       (payload as { groups: Array<Record<string, unknown>> }).groups.some(
         (g) => Array.isArray(g?.continuous) || Array.isArray(g?.categorical),
       )));

  const shouldMount = shouldMountAnalysisResultView(analysis);

  // Obtain releaseVersion from the coverage-matrix context (Requirement 3.2)
  const releaseVersion = matrix?.release_version ?? '';

  // Set default snapshot destination from run_id. In environments that support
  // the File System Access API, `pickSnapshotDestination` can be called to let
  // the user choose a custom save location (Requirement 3.8).
  useEffect(() => {
    if (analysis?.run_id && !snapshotDestination) {
      setSnapshotDestination(`snapshot-${analysis.run_id}.zip`);
    }
  }, [analysis?.run_id, snapshotDestination]);

  return (
    <div style={{ marginTop: 10 }}>
      <ErrorBoundary title="结果表格渲染失败" resetKey={analysis?.run_id ?? 'msg-table'}>
        {isStructuredTable ? (
          <div style={{ margin: '8px 0 16px' }}>
            <ThreeLineTable skillResult={result} />
          </div>
        ) : (
          <GenericKVTable payload={payload} />
        )}
      </ErrorBoundary>

      <ErrorBoundary title="图表渲染失败" resetKey={analysis?.run_id ?? 'msg-chart'}>
        <StatsChartRenderer skillResult={result} />
      </ErrorBoundary>

      <RiskSignalTags signals={risk_signals} />

      {/* Mount AnalysisResultView when meta qualifies (Requirement 3.1, 3.2, 3.3) */}
      {shouldMount && analysis && (
        <AnalysisResultView
          algorithmId={analysis.algorithm_id}
          params={analysis.params as Record<string, unknown>}
          columns={analysis.columns}
          datasetSha256={analysis.dataset_sha256!}
          runId={analysis.run_id}
          runStatus={analysis.run_status}
          releaseVersion={releaseVersion}
          snapshotDestination={snapshotDestination}
        />
      )}
    </div>
  );
}

function AnalysisResultReference({
  result,
  onOpenResult,
}: {
  result: SkillResult;
  onOpenResult?: (view: 'report' | 'chart') => void;
}) {
  const analysis = result.analysis;
  const algorithmLabel = analysis?.algorithm_id
    ? analysis.algorithm_id.replace(/^model_/, '').replace(/_/g, ' ')
    : '统计分析';

  return (
    <section className="analysis-result-reference" data-testid="analysis-result-reference">
      <div className="analysis-result-reference__heading">
        <span className="analysis-result-reference__icon" aria-hidden>
          <CheckCircleOutlined />
        </span>
        <div>
          <Text strong>分析工件已更新</Text>
          <Text type="secondary">
            {algorithmLabel}
            {analysis?.run_id ? ` · ${analysis.run_id.slice(0, 8)}` : ''}
          </Text>
        </div>
        <Tag color={analysis?.run_status === 'completed' ? 'success' : 'processing'}>
          {analysis?.run_status === 'completed' ? '已完成' : '已生成'}
        </Tag>
      </div>
      <Text className="analysis-result-reference__summary" type="secondary">
        报告、统计图与数据依据已放入右侧分析工件；对话中保留这条可追溯引用。
      </Text>
      <div className="analysis-result-reference__actions">
        <Button
          size="small"
          icon={<FileTextOutlined />}
          aria-label="查看报告"
          onClick={() => onOpenResult?.('report')}
        >
          查看报告
        </Button>
        <Button
          size="small"
          icon={<AreaChartOutlined />}
          aria-label="查看图表"
          onClick={() => onOpenResult?.('chart')}
        >
          查看图表
        </Button>
      </div>
    </section>
  );
}

function InterpretationView({ text }: { text: string }) {
  const { token } = antdTheme.useToken();
  return (
    <Card
      size="small"
      style={{
        marginTop: 10,
        background: token.colorSuccessBg,
        borderColor: token.colorSuccessBorder,
      }}
    >
      <Space size={6} align="start" style={{ marginBottom: 4 }}>
        <BulbOutlined style={{ color: token.colorSuccess }} />
        <Text strong>AI 解读</Text>
      </Space>
      <Paragraph style={{ marginBottom: 0, whiteSpace: 'pre-wrap' }}>{text}</Paragraph>
    </Card>
  );
}

// ---------------------------------------------------------------------------
// Main component
// ---------------------------------------------------------------------------

export function MessageList({
  messages,
  onChoiceSubmit,
  disabled = false,
  resultPresentation = 'inline',
  onOpenResult,
}: MessageListProps) {
  const bottomRef = useRef<HTMLDivElement>(null);
  const visibleMessages = messages.filter(hasVisibleMessageContent);

  // 文本、结果、解读和选择题任一部分更新时都应触发近底自动滚动；
  // 用户明显上翻时仍不强制打断阅读。
  const lastMsg = visibleMessages[visibleMessages.length - 1];
  const scrollKey = [
    visibleMessages.length,
    lastMsg?.id ?? '',
    lastMsg?.content.length ?? 0,
    lastMsg?.skillResult?.analysis?.run_id ?? '',
    lastMsg?.interpretation?.length ?? 0,
    lastMsg?.choicePrompt?.prompt_id ?? '',
  ].join(':');
  useEffect(() => {
    const sentinel = bottomRef.current;
    if (!sentinel) return;
    const container = sentinel.parentElement?.parentElement; // 外层 overflowY 容器
    if (container && container.scrollHeight > container.clientHeight) {
      const distanceFromBottom = container.scrollHeight - container.scrollTop - container.clientHeight;
      // 距底超过 240px 视为用户在读历史，不打断。
      if (distanceFromBottom > 240) return;
    }
    sentinel.scrollIntoView({ behavior: 'smooth' });
  }, [scrollKey]);

  if (visibleMessages.length === 0) {
    return <EmptyState />;
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16, paddingBottom: 8 }}>
      {visibleMessages.map((msg) => (
        <MessageBubble
          key={msg.id}
          message={msg}
          onChoiceSubmit={onChoiceSubmit}
          disabled={disabled}
          resultPresentation={resultPresentation}
          onOpenResult={onOpenResult}
        />
      ))}
      <div ref={bottomRef} />
    </div>
  );
}

// ---------------------------------------------------------------------------
// Empty state
// ---------------------------------------------------------------------------

function EmptyState() {
  const { token } = antdTheme.useToken();
  const examples = [
    { icon: '📈', title: '线性回归', body: '帮我分析血压与年龄的关系' },
    { icon: '📊', title: 'Logistic 回归', body: '风险因素对某事件的影响' },
    { icon: '⏱️', title: '生存分析', body: '不同治疗方案的生存率差异' },
    { icon: '⚡', title: '功效分析', body: '需要多少样本量才能检测出效应' },
  ];

  return (
    <div style={{ textAlign: 'center', padding: '48px 16px', color: token.colorTextSecondary }}>
      <RobotOutlined style={{ fontSize: 56, color: token.colorPrimary, marginBottom: 16 }} />
      <Paragraph style={{ fontSize: 16, marginBottom: 8 }} strong>
        Stats 智能分析助手
      </Paragraph>
      <Paragraph type="secondary" style={{ marginBottom: 24 }}>
        上传数据文件，用自然语言描述你的研究问题，由 AI 引导你完成分析
      </Paragraph>
      <div
        style={{
          display: 'grid',
          gridTemplateColumns: 'repeat(auto-fit, minmax(180px, 1fr))',
          gap: 12,
          maxWidth: 720,
          margin: '0 auto',
        }}
      >
        {examples.map((e) => (
          <div
            key={e.title}
            style={{
              padding: 14,
              background: token.colorBgContainer,
              border: `1px solid ${token.colorBorderSecondary}`,
              borderRadius: token.borderRadiusLG,
              textAlign: 'left',
            }}
          >
            <div style={{ fontSize: 22 }}>{e.icon}</div>
            <Text strong style={{ display: 'block', marginTop: 4 }}>
              {e.title}
            </Text>
            <Text type="secondary" style={{ fontSize: 12 }}>
              {e.body}
            </Text>
          </div>
        ))}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Message bubble
// ---------------------------------------------------------------------------

function MessageBubble({
  message,
  onChoiceSubmit,
  disabled = false,
  resultPresentation,
  onOpenResult,
}: {
  message: ChatMessage;
  onChoiceSubmit?: (answer: ChoiceAnswer) => void;
  disabled?: boolean;
  resultPresentation: 'inline' | 'reference';
  onOpenResult?: (view: 'report' | 'chart') => void;
}) {
  const { token } = antdTheme.useToken();
  const isUser = message.role === 'user';

  const userBubbleStyle: React.CSSProperties = {
    background: token.colorPrimary,
    color: '#fff',
    borderRadius: '12px 12px 2px 12px',
    padding: '10px 14px',
    maxWidth: '75%',
  };

  const agentBubbleStyle: React.CSSProperties = {
    background: token.colorBgContainer,
    color: token.colorText,
    border: `1px solid ${token.colorBorderSecondary}`,
    borderRadius: '12px 12px 12px 2px',
    padding: '12px 16px',
    maxWidth: '85%',
    boxShadow: token.boxShadowTertiary,
  };

  const avatarStyle: React.CSSProperties = {
    width: 32,
    height: 32,
    borderRadius: '50%',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    flexShrink: 0,
    color: '#fff',
    fontSize: 16,
  };

  return (
    <div
      className={`msg-bubble msg-bubble--${isUser ? 'user' : 'agent'}`}
      style={{
        display: 'flex',
        justifyContent: isUser ? 'flex-end' : 'flex-start',
        alignItems: 'flex-start',
        gap: 10,
      }}
    >
      {!isUser && (
        <div className="msg-avatar msg-avatar--agent" style={{ ...avatarStyle, background: token.colorPrimary }}>
          <RobotOutlined />
        </div>
      )}

      <div className="msg-bubble__content" style={isUser ? userBubbleStyle : agentBubbleStyle}>
        {message.content && (
          <Paragraph
            style={{
              marginBottom: 0,
              color: isUser ? '#fff' : 'inherit',
              whiteSpace: 'pre-wrap',
              lineHeight: 1.6,
            }}
          >
            {message.content}
          </Paragraph>
        )}

        {message.skillResult && (
          resultPresentation === 'reference' ? (
            <AnalysisResultReference result={message.skillResult} onOpenResult={onOpenResult} />
          ) : (
            <SkillResultView result={message.skillResult} />
          )
        )}
        {message.interpretation && <InterpretationView text={message.interpretation} />}
        {message.choicePrompt && (
          <ChoicePromptCard
            prompt={message.choicePrompt}
            onSubmit={(answer) => onChoiceSubmit?.(answer)}
            disabled={disabled}
          />
        )}
      </div>

      {isUser && (
        <div className="msg-avatar msg-avatar--user" style={{ ...avatarStyle, background: token.colorSuccess }}>
          <UserOutlined />
        </div>
      )}
    </div>
  );
}

export default MessageList;

/**
 * ReportViewer — 专业模式报告查看区（中部主区）。
 *
 * 由 useLatestAnalysis 派生最新结果：渲染报告文本 / ThreeLineTable /
 * StatsChartRenderer；有 interpretation 渲染 AI 解读卡片；RiskSignalTags 可见
 * 提示风险；ExportSnapshotButton 导出（runId/runStatus 取自 result.analysis）。
 * 无结果且选中数据集 → DataExplorer；否则空态。
 *
 * Validates: Requirements 6.1, 6.2, 6.3, 6.4
 */

import { Card, Typography, Space, Empty } from 'antd';
import { FileTextOutlined } from '@ant-design/icons';
import { ThreeLineTable } from '../../components/ThreeLineTable';
import { StatsChartRenderer } from '../../components/StatsChartRenderer';
import { RiskSignalTags } from '../../components/RiskSignalTags';
import { ExportSnapshotButton } from '../../components/ExportSnapshotButton';
import { DataExplorer } from '../../components/DataExplorer';
import { useLatestAnalysis } from '../../hooks/useLatestAnalysis';
import type { ChatMessage } from '../../hooks/useSseChat';
import type { DatasetSummary } from '../../api/types';

const { Text, Title, Paragraph } = Typography;

export interface ReportViewerProps {
  messages: ChatMessage[];
  selectedDataset: DatasetSummary | null;
}

export function ReportViewer({ messages, selectedDataset }: ReportViewerProps) {
  const { result, agentMessage } = useLatestAnalysis(messages);

  // 无结果分支：选中数据集 → DataExplorer，否则空态。
  if (!result) {
    if (selectedDataset) {
      return <DataExplorer summary={selectedDataset} />;
    }
    return (
      <Card className="glass-panel" style={{ textAlign: 'center', padding: '48px 16px' }}>
        <Empty
          image={Empty.PRESENTED_IMAGE_SIMPLE}
          description={<Text type="secondary">暂无分析结果，发起分析或选择数据集查看画像</Text>}
        />
      </Card>
    );
  }

  const analysis = result.analysis;

  return (
    <Space direction="vertical" size={20} style={{ width: '100%' }}>
      <Card
        className="glass-panel"
        title={
          <Title level={5} style={{ margin: 0 }}>
            <FileTextOutlined style={{ marginRight: 6 }} /> 分析报告结果
          </Title>
        }
        extra={
          analysis ? (
            <ExportSnapshotButton
              runId={analysis.run_id}
              destination={`snapshot-${analysis.run_id}.zip`}
              runStatus={analysis.run_status}
            />
          ) : undefined
        }
      >
        {agentMessage && agentMessage.content ? (
          <Paragraph style={{ whiteSpace: 'pre-wrap', lineHeight: 1.7 }}>
            {agentMessage.content.replace(/\[正在执行:.*?\]/g, '').trim()}
          </Paragraph>
        ) : null}

        <ThreeLineTable skillResult={result} />

        <RiskSignalTags signals={result.risk_signals} />

        {agentMessage && agentMessage.interpretation ? (
          <Card
            size="small"
            style={{ marginTop: 18, background: 'rgba(82, 196, 26, 0.04)', borderColor: 'rgba(82, 196, 26, 0.2)' }}
          >
            <Space size={6} align="start" style={{ marginBottom: 6 }}>
              <span style={{ fontSize: 16 }}>💡</span>
              <Text strong style={{ color: '#276749' }}>
                AI 统计解读
              </Text>
            </Space>
            <Paragraph style={{ marginBottom: 0, color: '#2f855a', fontSize: 13, lineHeight: 1.6 }}>
              {agentMessage.interpretation}
            </Paragraph>
          </Card>
        ) : null}
      </Card>

      <StatsChartRenderer skillResult={result} />
    </Space>
  );
}

export default ReportViewer;

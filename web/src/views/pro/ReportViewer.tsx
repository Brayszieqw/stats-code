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
import { BulbOutlined, FileTextOutlined } from '@ant-design/icons';
import { ThreeLineTable } from '../../components/ThreeLineTable';
import { StatsChartRenderer } from '../../components/StatsChartRenderer';
import { RiskSignalTags } from '../../components/RiskSignalTags';
import { ExportSnapshotButton } from '../../components/ExportSnapshotButton';
import { DataExplorer } from '../../components/DataExplorer';
import { ErrorBoundary } from '../../components/ErrorBoundary';
import { useLatestAnalysis } from '../../hooks/useLatestAnalysis';
import { fmtNum, fmtP, normalizeCoefficients, termHintsFromAnalysis } from '../../lib/coeffFields';
import type { ChatMessage } from '../../hooks/useSseChat';
import type { DatasetSummary } from '../../api/types';

const { Text, Title, Paragraph } = Typography;

export interface ReportViewerProps {
  messages: ChatMessage[];
  selectedDataset: DatasetSummary | null;
  activeView?: 'report' | 'chart' | 'data';
}

function isInterceptTerm(term: string): boolean {
  const normalized = term.trim().toLowerCase().replace(/[\s()]/g, '');
  return ['β0', 'b0', 'intercept', 'const', 'constant'].includes(normalized);
}

export function ReportViewer({ messages, selectedDataset, activeView }: ReportViewerProps) {
  const { result, resultMessage } = useLatestAnalysis(messages);
  const resolvedView = activeView ?? (result ? 'report' : 'data');

  if (resolvedView === 'data') {
    if (selectedDataset) {
      return (
        <DataExplorer
          summary={selectedDataset}
          previewRows={selectedDataset.preview_rows ?? null}
        />
      );
    }
    return (
      <Card className="glass-panel folio-report" style={{ textAlign: 'center', padding: '48px 16px' }}>
        <Empty
          image={Empty.PRESENTED_IMAGE_SIMPLE}
          description={<Text type="secondary">暂无分析结果，也未选择数据集</Text>}
        />
      </Card>
    );
  }

  if (!result) {
    return (
      <Card className="glass-panel folio-report" style={{ textAlign: 'center', padding: '48px 16px' }}>
        <Empty
          image={Empty.PRESENTED_IMAGE_SIMPLE}
          description={<Text type="secondary">暂无分析结果，发起分析或选择数据集查看画像</Text>}
        />
      </Card>
    );
  }

  const analysis = result.analysis;
  const payload = result.payload && typeof result.payload === 'object'
    ? result.payload as Record<string, unknown>
    : {};
  const termHints = termHintsFromAnalysis(
    analysis?.algorithm_id,
    analysis?.params as Record<string, unknown> | undefined,
  );
  const coefficients = normalizeCoefficients(payload.coefficients, termHints);
  const primaryCoefficient = coefficients.find((coefficient) => !isInterceptTerm(coefficient.term))
    ?? coefficients[0]
    ?? null;
  const effectValue = primaryCoefficient
    ? primaryCoefficient.hazardRatio ?? primaryCoefficient.oddsRatio ?? primaryCoefficient.beta
    : null;
  const logRank = payload.log_rank && typeof payload.log_rank === 'object'
    ? payload.log_rank as Record<string, unknown>
    : null;
  const pValue = primaryCoefficient?.pValue
    ?? (typeof logRank?.p_value === 'number' ? logRank.p_value : null)
    ?? (typeof payload.p_value === 'number' ? payload.p_value : null);
  const sampleSize = selectedDataset?.row_count
    ?? (typeof payload.n === 'number' ? payload.n : null);
  const confidenceInterval = primaryCoefficient && primaryCoefficient.ciLower !== null && primaryCoefficient.ciUpper !== null
    ? `[${fmtNum(primaryCoefficient.ciLower)}, ${fmtNum(primaryCoefficient.ciUpper)}]`
    : null;

  if (resolvedView === 'chart') {
    return (
      <ErrorBoundary title="图表渲染失败" resetKey={analysis?.run_id ?? 'chart'}>
        <StatsChartRenderer skillResult={result} />
      </ErrorBoundary>
    );
  }

  return (
    <Space className="report-stack" direction="vertical" size={18} style={{ width: '100%' }}>
      <Card
        className="glass-panel folio-report"
        title={
          <Title className="report-title" level={5} style={{ margin: 0 }}>
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
        {resultMessage && resultMessage.content ? (
          <>
            <div className="report-section-label">关键结论</div>
            <Paragraph className="report-lead" style={{ whiteSpace: 'pre-wrap', lineHeight: 1.7 }}>
              {resultMessage.content.replace(/\[正在执行:.*?\]/g, '').trim()}
            </Paragraph>
          </>
        ) : null}

        {(effectValue !== null || confidenceInterval !== null || pValue !== null || sampleSize !== null) ? (
          <div className="report-metrics" aria-label="关键统计量">
            {effectValue !== null ? (
              <div className="report-metric">
                <span>效应量</span>
                <strong>{fmtNum(effectValue)}</strong>
                <small>
                  {primaryCoefficient?.term} · {primaryCoefficient?.hazardRatio !== null ? 'HR' : primaryCoefficient?.oddsRatio !== null ? 'OR' : 'Beta'}
                </small>
              </div>
            ) : null}
            {confidenceInterval !== null ? (
              <div className="report-metric report-metric--interval">
                <span>95% 置信区间</span>
                <strong>{confidenceInterval}</strong>
                <small>效应估计范围</small>
              </div>
            ) : null}
            {pValue !== null ? (
              <div className="report-metric">
                <span>P 值</span>
                <strong className={pValue < 0.05 ? 'is-significant' : undefined}>{fmtP(pValue)}</strong>
                <small>{pValue < 0.05 ? '统计学显著' : '未达显著'}</small>
              </div>
            ) : null}
            {sampleSize !== null ? (
              <div className="report-metric">
                <span>样本量</span>
                <strong>{sampleSize}</strong>
                <small>有效记录</small>
              </div>
            ) : null}
          </div>
        ) : null}

        <ErrorBoundary title="结果表格渲染失败" resetKey={analysis?.run_id ?? 'table'}>
          <ThreeLineTable skillResult={result} />
        </ErrorBoundary>

        <RiskSignalTags signals={result.risk_signals} />

        {resultMessage && resultMessage.interpretation ? (
          <Card
            className="interpretation-note"
            size="small"
            style={{ marginTop: 18, background: 'rgba(36, 79, 115, 0.04)', borderColor: 'rgba(36, 79, 115, 0.22)' }}
          >
            <Space size={6} align="start" style={{ marginBottom: 6 }}>
              <BulbOutlined style={{ color: '#244f73', marginTop: 3 }} />
              <Text strong style={{ color: '#244f73' }}>
                AI 统计解读
              </Text>
            </Space>
            <Paragraph style={{ marginBottom: 0, color: '#425a70', fontSize: 13, lineHeight: 1.7 }}>
              {resultMessage.interpretation}
            </Paragraph>
          </Card>
        ) : null}
      </Card>

    </Space>
  );
}

export default ReportViewer;

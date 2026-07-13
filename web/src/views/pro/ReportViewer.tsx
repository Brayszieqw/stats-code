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

import { Card, Typography, Space, Empty, Tag } from 'antd';
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

const AVAILABILITY_LABEL = {
  available: '已提供',
  not_computed: '未计算',
  not_applicable: '不适用',
} as const;

const CONVERGENCE_LABEL = {
  converged: '已收敛',
  failed: '未收敛',
  not_applicable: '不适用',
  unknown: '未知',
} as const;

const LOG_RANK_REASON_LABEL: Record<string, string> = {
  insufficient_groups: '至少需要两个非空分组。',
  no_events: '所有记录均为删失，无法比较事件分布。',
  degenerate_variance: '风险集方差退化，无法稳定计算检验。',
};

export function ReportViewer({ messages, selectedDataset, activeView }: ReportViewerProps) {
  const { result, resultMessage } = useLatestAnalysis(messages);
  const resolvedView = activeView ?? (result ? 'report' : 'data');

  if (resolvedView === 'data') {
    if (selectedDataset) {
      return (
        <DataExplorer
          summary={selectedDataset}
          previewRows={selectedDataset.preview_rows ?? null}
          compact
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
  const resultContract = analysis?.result_contract;
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
  const survivalGroupSummaries = Array.isArray(payload.group_summaries)
    ? payload.group_summaries as Array<Record<string, unknown>>
    : [];
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

        {survivalGroupSummaries.length > 0 ? (
          <section className="result-contract" aria-label="生存分析分组摘要">
            <div className="result-contract__header">
              <strong>生存分析分组摘要</strong>
              <span>事件、删失与中位生存时间</span>
            </div>
            <div className="result-contract__grid">
              {survivalGroupSummaries.map((summary) => (
                <div key={String(summary.group)}>
                  <span>{String(summary.group)}</span>
                  <strong>
                    n={String(summary.n)} · 事件={String(summary.event_n)} · 删失={String(summary.censored_n)}
                  </strong>
                  <small>
                    中位生存 {typeof summary.median_survival === 'number' ? fmtNum(summary.median_survival) : '未达到'}
                  </small>
                </div>
              ))}
            </div>
            {logRank ? (
              <div className="result-contract__details">
                <strong>Log-rank 组间比较</strong>
                {typeof logRank.p_value === 'number' ? (
                  <p>
                    χ²={fmtNum(typeof logRank.statistic === 'number' ? logRank.statistic : null)}
                    {' · '}df={String(logRank.degrees_of_freedom)}
                    {' · '}p={fmtP(logRank.p_value)}
                  </p>
                ) : (
                  <p>未计算：{LOG_RANK_REASON_LABEL[String(logRank.reason)] ?? '当前数据不满足检验条件。'}</p>
                )}
              </div>
            ) : null}
          </section>
        ) : null}

        {resultContract ? (
          <section className="result-contract" aria-label="标准化结果合同">
            <div className="result-contract__header">
              <strong>结果合同 v{resultContract.schema_version}</strong>
              <span>{resultContract.method.algorithm_id} · 方法版本 {resultContract.method.method_version}</span>
            </div>

            <div className="result-contract__grid">
              <div><span>有效记录</span><strong>{resultContract.counts.complete_case_n} / {resultContract.counts.input_n}</strong></div>
              <div><span>缺失记录</span><strong>{resultContract.counts.missing_n}</strong></div>
              {resultContract.counts.event_n !== null ? (
                <div><span>事件数</span><strong>{resultContract.counts.event_n}</strong></div>
              ) : null}
              {resultContract.counts.person_time !== null ? (
                <div><span>总人时</span><strong>{fmtNum(resultContract.counts.person_time)}</strong></div>
              ) : null}
              <div><span>模型收敛</span><strong>{CONVERGENCE_LABEL[resultContract.convergence.status]}</strong></div>
              <div>
                <span>分析范围</span>
                <strong>
                  未调整 {AVAILABILITY_LABEL[resultContract.analysis_availability.unadjusted]}
                  {' · '}调整后 {AVAILABILITY_LABEL[resultContract.analysis_availability.adjusted]}
                </strong>
              </div>
              <div className="result-contract__engine">
                <span>确定性引擎</span>
                <strong>{resultContract.provenance.engine_name} {resultContract.provenance.engine_version}</strong>
              </div>
            </div>

            <div className="result-contract__coverage">
              <span>验证覆盖</span>
              <Space size={[4, 4]} wrap>
                {Object.entries(resultContract.provenance.validation_coverage).map(([software, level]) => (
                  <Tag key={software}>{software}: {level}</Tag>
                ))}
              </Space>
            </div>

            {resultContract.assumption_diagnostics.length > 0 ? (
              <div className="result-contract__details">
                <strong>假设诊断</strong>
                {resultContract.assumption_diagnostics.map((diagnostic) => (
                  <p key={diagnostic.code}>{diagnostic.message}</p>
                ))}
              </div>
            ) : null}

            {resultContract.exclusions.length > 0 ? (
              <div className="result-contract__details">
                <strong>排除记录</strong>
                {resultContract.exclusions.map((exclusion, index) => (
                  <p key={`${exclusion.reason}-${index}`}>
                    {exclusion.reason}{exclusion.n === null ? '' : `（n=${exclusion.n}）`}
                  </p>
                ))}
              </div>
            ) : null}

            <div className="result-contract__details result-contract__limits">
              <strong>不支持的结论</strong>
              {resultContract.interpretation.unsupported_conclusions.map((conclusion) => (
                <p key={conclusion}>{conclusion}</p>
              ))}
              {resultContract.interpretation.statistical === null
                || resultContract.interpretation.practical_significance === null ? (
                  <p>统计解释与实际/临床意义需由合格研究者结合协议审核。</p>
                ) : null}
            </div>
          </section>
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

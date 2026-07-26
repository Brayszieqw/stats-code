/**
 * ReportViewer — 专业模式报告查看区（中部主区）。
 *
 * 默认显示 useLatestAnalysis 派生的最新结果，也可显示用户选中的历史结果。
 * 渲染报告文本 / ThreeLineTable /
 * StatsChartRenderer；有 interpretation 渲染方法学提示卡片；RiskSignalTags 可见
 * 提示风险；ExportSnapshotButton 导出（runId/runStatus 取自 result.analysis）。
 * 无结果且选中数据集 → DataExplorer；否则空态。
 *
 * Validates: Requirements 6.1, 6.2, 6.3, 6.4
 */

import { Card, Collapse, Typography, Space, Empty, Tag } from 'antd';
import { BulbOutlined, FileTextOutlined, SafetyCertificateOutlined } from '@ant-design/icons';
import { ThreeLineTable } from '../../components/ThreeLineTable';
import { StatsTable } from '../../components/StatsTable';
import { StatsChartRenderer } from '../../components/StatsChartRenderer';
import { RiskSignalTags } from '../../components/RiskSignalTags';
import { ExportSnapshotButton } from '../../components/ExportSnapshotButton';
import { DataExplorer } from '../../components/DataExplorer';
import { ErrorBoundary } from '../../components/ErrorBoundary';
import { useLatestAnalysis } from '../../hooks/useLatestAnalysis';
import { fmtNum, fmtP, normalizeCoefficients, termHintsFromAnalysis } from '../../lib/coeffFields';
import { methodShortLabel } from '../../lib/displayLabels';
import type { ChatMessage } from '../../hooks/useSseChat';
import type { DatasetSummary, SkillResult } from '../../api/types';

const { Text, Title, Paragraph } = Typography;

export interface ReportArtifact {
  resultMessage: ChatMessage & { skillResult: SkillResult };
}

export interface ReportViewerProps {
  messages: ChatMessage[];
  selectedDataset: DatasetSummary | null;
  activeView?: 'report' | 'chart' | 'data';
  pinnedArtifact?: ReportArtifact | null;
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

export function ReportViewer({ messages, selectedDataset, activeView, pinnedArtifact }: ReportViewerProps) {
  const latest = useLatestAnalysis(messages);
  const resultMessage = pinnedArtifact?.resultMessage ?? latest.resultMessage;
  const result = pinnedArtifact?.resultMessage.skillResult ?? latest.result;
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
  const flatNum = (key: string): number | null => {
    const value = payload[key];
    return typeof value === 'number' && Number.isFinite(value) ? value : null;
  };
  /**
   * 扁平载荷（t 检验 / 相关 / 功效）没有 coefficients，此前指标卡的效应量与
   * 置信区间因此整块消失，用户只看到 p 与样本量——真机验收记为「报告不完整」。
   * 这里按算法给出对应的效应量语义标签，而不是笼统写「Beta」。
   */
  const flatEffect: { value: number; term: string; unit: string } | null = (() => {
    const algorithmId = analysis?.algorithm_id;
    if (algorithmId === 'ttest') {
      const meanDiff = flatNum('mean_diff');
      if (meanDiff !== null) {
        return {
          value: meanDiff,
          term: typeof payload.test_variable === 'string' ? payload.test_variable : '组间差',
          unit: '均值差',
        };
      }
    }
    if (algorithmId === 'correlation') {
      const r = flatNum('r');
      if (r !== null) {
        const isSpearman = typeof payload.method === 'string'
          && payload.method.toLowerCase().includes('spearman');
        const x = typeof payload.x === 'string' ? payload.x : '?';
        const y = typeof payload.y === 'string' ? payload.y : '?';
        return { value: r, term: `${x} ~ ${y}`, unit: isSpearman ? 'Spearman ρ' : 'Pearson r' };
      }
    }
    if (algorithmId === 'anova') {
      const f = flatNum('f_statistic');
      if (f !== null) {
        return {
          value: f,
          term: typeof payload.test_variable === 'string' ? payload.test_variable : '组间',
          unit: 'F 统计量',
        };
      }
    }
    return null;
  })();
  const effectValue = primaryCoefficient
    ? primaryCoefficient.hazardRatio ?? primaryCoefficient.oddsRatio ?? primaryCoefficient.beta
    : flatEffect?.value ?? null;
  const logRank = payload.log_rank && typeof payload.log_rank === 'object'
    ? payload.log_rank as Record<string, unknown>
    : null;
  const survivalGroupSummaries = Array.isArray(payload.group_summaries)
    ? payload.group_summaries as Array<Record<string, unknown>>
    : [];
  const pValue = primaryCoefficient?.pValue
    ?? (typeof logRank?.p_value === 'number' ? logRank.p_value : null)
    ?? (typeof payload.p_value === 'number' ? payload.p_value : null);
  // 功效分析是设计阶段计算，不读数据集：此时「样本量」应当是算出来的所需 n，
  // 而不是当前选中数据集的行数（那与本次结果无关，会误导读者）。
  const isPowerPayload = typeof payload.required_n === 'number'
    && typeof payload.achieved_power === 'number';
  const sampleSize = isPowerPayload
    ? Math.ceil(payload.required_n as number)
    : selectedDataset?.row_count
      ?? (typeof payload.n === 'number' ? payload.n : null);
  const flatCiLower = flatNum('ci_lower');
  const flatCiUpper = flatNum('ci_upper');
  const confidenceInterval = primaryCoefficient && primaryCoefficient.ciLower !== null && primaryCoefficient.ciUpper !== null
    ? `[${fmtNum(primaryCoefficient.ciLower)}, ${fmtNum(primaryCoefficient.ciUpper)}]`
    : flatCiLower !== null && flatCiUpper !== null
      ? `[${fmtNum(flatCiLower)}, ${fmtNum(flatCiUpper)}]`
      : null;

  // Table-shell caption counts. Table One reports variables × groups; the
  // regression table reports one row per coefficient and has no strata.
  const tableGroups = Array.isArray(payload.groups) ? payload.groups : [];
  const isTableOne = tableGroups.some((group) => {
    const entry = group as Record<string, unknown> | null;
    return Boolean(entry) && (Array.isArray(entry!.continuous) || Array.isArray(entry!.categorical));
  });
  const tableOneVariableCount = isTableOne
    ? (Array.isArray(payload.continuous) ? payload.continuous.length : 0)
      + (Array.isArray(payload.categorical) ? payload.categorical.length : 0)
    : null;
  // 与 ThreeLineTable 的分支判据保持一致：认 algorithm_id，不认 payload.method
  // （后者是引擎展示名 'One-way ANOVA'，会随文案变动）。
  const isAnova = analysis?.algorithm_id === 'anova' && typeof payload.f_statistic === 'number';
  // 与 ThreeLineTable 的新增分支一一对应，否则外壳标题与内容会不一致。
  const isTtest = analysis?.algorithm_id === 'ttest' && typeof payload.t_statistic === 'number';
  const isCorrelation = analysis?.algorithm_id === 'correlation' && typeof payload.r === 'number';
  const isPower = isPowerPayload;
  const tableTitle = isTableOne
    ? '基线特征表'
    : coefficients.length > 0
      ? '回归系数表'
      : isAnova
        ? '方差分析表'
        : isTtest
          ? 't 检验结果表'
          : isCorrelation
            ? '相关分析结果表'
            : isPower
              ? '功效与样本量'
              : '结果表格';

  /**
   * ThreeLineTable 只覆盖 tableone / 回归系数 / ANOVA / markdown 四种载荷，
   * 其余（如 t 检验、相关分析的扁平结果）会返回 null。此前外壳无条件渲染，
   * 于是产生一个只有标题、没有任何内容的空「结果表格」。这里预判有无可渲染
   * 内容，没有就整块不渲染，交由上方的指标卡与下方的审计材料承载结果。
   */
  const hasRenderableTable = isTableOne
    || coefficients.length > 0
    || isAnova
    || isTtest
    || isCorrelation
    || isPower
    || (Array.isArray(payload.rows) && Boolean(payload.group_levels));

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
                  {primaryCoefficient
                    ? `${primaryCoefficient.term} · ${primaryCoefficient.hazardRatio !== null ? 'HR' : primaryCoefficient.oddsRatio !== null ? 'OR' : 'Beta'}`
                    : flatEffect
                      ? `${flatEffect.term} · ${flatEffect.unit}`
                      : ''}
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
                <span>{isPower ? '每组样本量' : '样本量'}</span>
                <strong>{sampleSize}</strong>
                <small>{isPower ? '达到目标功效所需' : '有效记录'}</small>
              </div>
            ) : null}
          </div>
        ) : null}

        {/* 表格是交付物，紧随摘要呈现；审计材料折叠在其后。
            无可渲染表格的载荷（t 检验、相关分析等扁平结果）不渲染空壳。 */}
        {hasRenderableTable ? (
          <ErrorBoundary title="结果表格渲染失败" resetKey={analysis?.run_id ?? 'table'}>
            <StatsTable
              title={tableTitle}
              {...(tableOneVariableCount !== null ? { variableCount: tableOneVariableCount } : {})}
              {...(isTableOne ? { groupCount: tableGroups.length } : {})}
              filterable={isTableOne}
            >
              {(filterKeyword) => <ThreeLineTable skillResult={result} filterKeyword={filterKeyword} />}
            </StatsTable>
          </ErrorBoundary>
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

        {/* 审计导出：报告正文之后、合同折叠之前 — 工作流「诊断与导出」终点。 */}
        {analysis ? (
          <ExportSnapshotButton
            runId={analysis.run_id}
            destination={`snapshot-${analysis.run_id}.zip`}
            runStatus={analysis.run_status}
          />
        ) : null}

        {resultContract ? (
          <Collapse
            className="report-audit-collapse"
            size="small"
            ghost
            items={[{
              key: 'result-contract',
              label: (
                <span className="report-audit-collapse__label">
                  <SafetyCertificateOutlined />
                  <span>
                    <strong>结果合同 v{resultContract.schema_version} 与审计材料</strong>
                    <small>有效记录 / 收敛 / 验证覆盖 / 假设诊断 / 能力边界</small>
                  </span>
                </span>
              ),
              children: (
          <section className="result-contract" aria-label="标准化结果合同">
            <div className="result-contract__header">
              <strong>结果合同 v{resultContract.schema_version}</strong>
              <span>{methodShortLabel(resultContract.method.algorithm_id)} · 方法版本 {resultContract.method.method_version}</span>
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
              ),
            }]}
          />
        ) : null}

        <RiskSignalTags signals={result.risk_signals} />

        {resultMessage && resultMessage.interpretation ? (
          <Card
            className="interpretation-note method-note-card"
            size="small"
            style={{ marginTop: 18, background: 'rgba(36, 79, 115, 0.04)', borderColor: 'rgba(36, 79, 115, 0.22)' }}
          >
            <Space size={6} align="start" style={{ marginBottom: 6 }}>
              <BulbOutlined style={{ color: '#244f73', marginTop: 3 }} />
              <Text strong style={{ color: '#244f73' }}>
                方法学提示
              </Text>
            </Space>
            <Text type="secondary" style={{ display: 'block', fontSize: 12, marginBottom: 6, color: '#6b7c8d' }}>
              数值以本机结果卡与结果契约为准；此处仅说明方法适用条件与风险处理方向。
            </Text>
            <Paragraph style={{ marginBottom: 0, color: '#425a70', fontSize: 13, lineHeight: 1.7, whiteSpace: 'pre-wrap' }}>
              {resultMessage.interpretation}
            </Paragraph>
          </Card>
        ) : null}
      </Card>

    </Space>
  );
}

export default ReportViewer;

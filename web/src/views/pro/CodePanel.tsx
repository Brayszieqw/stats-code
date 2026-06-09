/**
 * CodePanel — 专业模式多语言等价代码侧栏（右 Sider）。
 *
 * 复用 EquivalentCodeSidecar（R/SAS/Python/SPSS 四标签、即时切换、
 * CopyToClipboard、占位/空状态），入参来自 result.analysis 与
 * useCoverageMatrix().matrix.release_version。无 analysis 时显示占位且不挂载
 * sidecar、不发起 sidecar 请求（Property 6）；嵌入 RunControls。
 *
 * Validates: Requirements 7.1, 7.2, 7.3, 7.5
 */

import { Empty, Typography, Divider } from 'antd';
import { EquivalentCodeSidecar } from '../../components/EquivalentCodeSidecar';
import { RunControls } from './RunControls';
import { useCoverageMatrix } from '../../lib/coverageMatrixContext';
import type { SidecarColumn } from '../../hooks/useSidecar';
import type { AnalysisResultMeta, ColumnType } from '../../api/types';

const { Text } = Typography;

export interface CodePanelProps {
  sessionId: string;
  /** Latest result analysis meta; null/undefined → placeholder, no sidecar. */
  analysis: AnalysisResultMeta | null | undefined;
  disabled?: boolean;
}

const DTYPE_MAP: Record<ColumnType, string> = {
  Numeric: 'numeric',
  Categorical: 'categorical',
  Date: 'date',
  String: 'string',
};

function toSidecarColumns(analysis: AnalysisResultMeta): SidecarColumn[] {
  return analysis.columns.map((c) => ({ name: c.name, dtype: DTYPE_MAP[c.inferred_type] ?? 'string' }));
}

function toParams(analysis: AnalysisResultMeta): Record<string, string> | undefined {
  const p = analysis.params;
  if (!p || typeof p !== 'object' || Array.isArray(p)) return undefined;
  const out: Record<string, string> = {};
  for (const [k, v] of Object.entries(p as Record<string, unknown>)) {
    out[k] = typeof v === 'string' ? v : JSON.stringify(v);
  }
  return out;
}

export function CodePanel({ sessionId, analysis, disabled = false }: CodePanelProps) {
  const { matrix } = useCoverageMatrix();
  const releaseVersion = matrix?.release_version ?? '';

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
      <Text strong style={{ fontSize: 13 }}>
        等价代码
      </Text>

      {analysis ? (
        <EquivalentCodeSidecar
          algorithmId={analysis.algorithm_id}
          datasetSha256={analysis.dataset_sha256 ?? ''}
          columns={toSidecarColumns(analysis)}
          params={toParams(analysis)}
          releaseVersion={releaseVersion}
        />
      ) : (
        <Empty
          image={Empty.PRESENTED_IMAGE_SIMPLE}
          description={<Text type="secondary">暂无可展示的等价代码，请先完成一次分析</Text>}
        />
      )}

      <Divider style={{ margin: '8px 0' }} />

      <RunControls sessionId={sessionId} analysis={analysis} disabled={disabled} />
    </div>
  );
}

export default CodePanel;

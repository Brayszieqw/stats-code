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

import { useCallback, useEffect, useState } from 'react';
import { Alert, Empty, Typography } from 'antd';
import { CodeOutlined, DatabaseOutlined } from '@ant-design/icons';
import { EquivalentCodeSidecar } from '../../components/EquivalentCodeSidecar';
import { RunControls } from './RunControls';
import { useCoverageMatrix } from '../../lib/coverageMatrixContext';
import type { SidecarColumn } from '../../hooks/useSidecar';
import type { AnalysisResultMeta, ColumnType, DatasetSummary, SkillResult } from '../../api/types';

const { Text } = Typography;

export interface CodePanelProps {
  sessionId: string;
  /** Latest result analysis meta; null/undefined → placeholder, no sidecar. */
  analysis: AnalysisResultMeta | null | undefined;
  dataset?: DatasetSummary | null;
  disabled?: boolean;
  onRunComplete?: (result: SkillResult) => void;
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

function formatBytes(bytes: number): string {
  if (bytes >= 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(2)} MB`;
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(2)} KB`;
  return `${bytes} B`;
}

function formatUploadedAt(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString('zh-CN', {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    hour12: false,
  });
}

export function CodePanel({
  sessionId,
  analysis,
  dataset,
  disabled = false,
  onRunComplete,
}: CodePanelProps) {
  const { matrix } = useCoverageMatrix();
  const releaseVersion = matrix?.release_version ?? '';
  const datasetSha256 = analysis?.dataset_sha256 ?? dataset?.sha256 ?? '';
  const [snippetText, setSnippetText] = useState<string | undefined>(undefined);

  const handleSnippetTextChange = useCallback((text: string | undefined) => {
    setSnippetText(text);
  }, []);

  useEffect(() => {
    setSnippetText(undefined);
  }, [analysis?.run_id]);

  return (
    <div className="code-panel">
      <div className="code-panel__heading">
        <span><CodeOutlined style={{ marginRight: 7, color: '#244f73' }} />等价代码</span>
        <Text type="secondary" style={{ fontSize: 10 }}>REPRODUCIBLE</Text>
      </div>

      {dataset ? (
        <details className="code-panel__dataset" aria-label="数据概览">
          <summary className="code-panel__dataset-title">
            <span><DatabaseOutlined /> {dataset.file_name}</span>
            <Text type="secondary">{dataset.row_count} 行 × {dataset.columns.length} 列</Text>
          </summary>
          <dl>
            <div><dt>文件大小</dt><dd>{formatBytes(dataset.size_bytes)}</dd></div>
            <div><dt>编码</dt><dd>{dataset.encoding}</dd></div>
            <div><dt>上传时间</dt><dd>{formatUploadedAt(dataset.uploaded_at)}</dd></div>
          </dl>
        </details>
      ) : null}

      <div className="code-panel__body">
        <div className="code-panel__content">
          {analysis && !datasetSha256 ? (
            <Alert
              className="code-panel__provenance-warning"
              type="warning"
              showIcon
              message="数据指纹缺失"
              description="代码仍可参考，但无法校验它与原始数据文件的对应关系。"
            />
          ) : null}
          {analysis && datasetSha256 ? (
            <EquivalentCodeSidecar
              algorithmId={analysis.algorithm_id}
              datasetSha256={datasetSha256}
              columns={toSidecarColumns(analysis)}
              params={toParams(analysis)}
              releaseVersion={releaseVersion}
              onSnippetTextChange={handleSnippetTextChange}
              showCopy={false}
            />
          ) : (
            <Empty
              image={Empty.PRESENTED_IMAGE_SIMPLE}
              description={(
                <Text type="secondary">
                  {analysis ? '缺少数据指纹，无法生成可验证的等价代码' : '暂无可展示的等价代码，请先完成一次分析'}
                </Text>
              )}
              style={{ marginTop: 40 }}
            />
          )}
        </div>

        <div className="code-panel__controls">
          <RunControls
            sessionId={sessionId}
            analysis={analysis}
            copyText={snippetText}
            disabled={disabled}
            layout="horizontal"
            onRunComplete={onRunComplete}
          />
        </div>
      </div>
    </div>
  );
}

export default CodePanel;

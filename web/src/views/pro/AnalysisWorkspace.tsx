import { useEffect, useState } from 'react';
import { Alert, Button, Collapse, Segmented, Tag } from 'antd';
import {
  AreaChartOutlined,
  BarChartOutlined,
  CloseOutlined,
  CodeOutlined,
  DatabaseOutlined,
  FileTextOutlined,
  SettingOutlined,
} from '@ant-design/icons';
import { AnalysisConfigurator } from '../../components/AnalysisConfigurator';
import { ReportViewer } from './ReportViewer';
import { CodePanel } from './CodePanel';
import type { ChatMessage } from '../../hooks/useSseChat';
import type { AnalysisResultMeta, DatasetSummary, RunRequest, SkillResult } from '../../api/types';

export type WorkspaceView = 'report' | 'chart' | 'data' | 'code';

export interface AnalysisWorkspaceProps {
  view: WorkspaceView;
  onViewChange: (view: WorkspaceView) => void;
  onClose: () => void;
  title: string;
  messages: ChatMessage[];
  selectedDataset: DatasetSummary | null;
  artifactDataset: DatasetSummary | null;
  analysisDataset: DatasetSummary | null;
  analysis: AnalysisResultMeta | null;
  sessionId: string;
  isArchived: boolean;
  isRunning: boolean;
  runError: string | null;
  onConfiguredRun: (request: RunRequest, promptText: string) => Promise<void>;
  onInspectorRunComplete: (result: SkillResult, sessionId: string) => void;
}

function collapseKeysInclude(keys: string | string[], target: string): boolean {
  return Array.isArray(keys) ? keys.includes(target) : keys === target;
}

export function AnalysisWorkspace({
  view,
  onViewChange,
  onClose,
  title,
  messages,
  selectedDataset,
  artifactDataset,
  analysisDataset,
  analysis,
  sessionId,
  isArchived,
  isRunning,
  runError,
  onConfiguredRun,
  onInspectorRunComplete,
}: AnalysisWorkspaceProps) {
  const needsConfiguration = Boolean(
    selectedDataset && (!analysis || analysis.dataset_id !== selectedDataset.dataset_id),
  );
  const [configOpen, setConfigOpen] = useState(needsConfiguration);

  // Auto-open when a new dataset still needs its first run; collapse after a
  // successful binding so the report stays primary. Re-open is always one click.
  useEffect(() => {
    setConfigOpen(needsConfiguration);
  }, [analysis?.run_id, needsConfiguration, selectedDataset?.dataset_id]);

  const openConfiguration = () => setConfigOpen(true);

  return (
    <aside className="pro-workspace-panel" aria-label="分析检查器">
      <header className="pro-workspace-panel__header">
        <div className="pro-workspace-panel__title">
          <span className="pro-workspace-panel__eyebrow">Workspace</span>
          <strong title={title}>{title}</strong>
          <small>
            {analysis ? `${analysis.run_id.slice(0, 8)} · ${analysisDataset?.file_name ?? '数据快照不可用'}` : '等待分析结果'}
          </small>
        </div>
        <Button
          type="text"
          size="small"
          icon={<CloseOutlined />}
          aria-label="关闭分析检查器"
          onClick={onClose}
        />
      </header>

      <div className="pro-workspace-panel__switcher">
        <Segmented
          block
          size="small"
          value={view}
          aria-label="工作区视图"
          onChange={(value) => onViewChange(value as WorkspaceView)}
          options={[
            { label: '报告', value: 'report', icon: <FileTextOutlined /> },
            { label: '图表', value: 'chart', icon: <BarChartOutlined /> },
            { label: '数据', value: 'data', icon: <DatabaseOutlined /> },
            { label: '代码', value: 'code', icon: <CodeOutlined /> },
          ]}
        />
      </div>

      <div className={`pro-workspace-panel__body ${view === 'code' ? 'is-code' : ''}`}>
        {view === 'code' ? (
          <section className="pro-workspace-code" aria-label="可复现代码">
            <CodePanel
              sessionId={sessionId}
              analysis={analysis}
              dataset={analysisDataset}
              disabled={isArchived}
              onRunComplete={(result) => onInspectorRunComplete(result, sessionId)}
            />
          </section>
        ) : (
          <>
            {selectedDataset ? (
              <>
                {analysis && !configOpen ? (
                  <Button
                    block
                    type="default"
                    icon={<SettingOutlined />}
                    aria-label="调整变量或再次分析"
                    onClick={openConfiguration}
                    className="pro-reconfigure-btn"
                    style={{ marginBottom: 10 }}
                  >
                    调整变量或再次分析
                  </Button>
                ) : null}
                <Collapse
                  className="pro-configurator-collapse"
                  size="small"
                  activeKey={configOpen ? ['configuration'] : []}
                  onChange={(keys) => setConfigOpen(collapseKeysInclude(keys as string | string[], 'configuration'))}
                  items={[{
                    key: 'configuration',
                    label: (
                      <span className="pro-configurator-collapse__label">
                        <SettingOutlined />
                        <span>
                          <strong>分析设置</strong>
                          <small>{needsConfiguration ? '完成变量与模型配置' : '调整变量或再次分析'}</small>
                        </span>
                      </span>
                    ),
                    children: (
                      <AnalysisConfigurator
                        summary={selectedDataset}
                        onSubmit={onConfiguredRun}
                        disabled={isArchived || isRunning}
                      />
                    ),
                  }]}
                />
              </>
            ) : null}
            {isRunning ? (
              <Alert type="info" showIcon message="正在运行后端统计引擎" className="pro-run-alert" />
            ) : null}
            {runError ? (
              <Alert type="error" showIcon message="运行失败" description={runError} className="pro-run-alert" />
            ) : null}
            {analysis ? <Tag className="pro-workspace-run-tag"><AreaChartOutlined /> 结果已绑定当前运行</Tag> : null}
            <ReportViewer messages={messages} selectedDataset={artifactDataset} activeView={view} />
          </>
        )}
      </div>
    </aside>
  );
}

export default AnalysisWorkspace;

/**
 * RunControls — 专业模式运行控制按钮组（嵌入 CodePanel）。
 *
 * 运行 / 调试 / 清空 / 停止 / 复制（均带 aria-label），消费 useCodeRun。
 * 运行中 loading；成功展示结果摘要；失败展示错误码/信息。无 analysis 或只读态
 * 禁用「运行」。停止 abort、清空 reset、复制复用 CopyToClipboard。
 *
 * Validates: Requirements 7.4, 9.3, 10.2, 12.7
 */

import { useCallback } from 'react';
import { Button, Space, Alert, Typography } from 'antd';
import {
  PlayCircleOutlined,
  BugOutlined,
  ClearOutlined,
  StopOutlined,
  LoadingOutlined,
} from '@ant-design/icons';
import { CopyToClipboard } from '../../components/CopyToClipboard';
import { useCodeRun } from '../../hooks/useCodeRun';
import type { AnalysisResultMeta, RunRequest } from '../../api/types';

const { Text } = Typography;

export interface RunControlsProps {
  sessionId: string;
  /** Analysis meta of the latest result; null when nothing is runnable. */
  analysis: AnalysisResultMeta | null | undefined;
  /** Snippet text to copy (active code tab); undefined disables copy. */
  copyText?: string;
  /** Read-only (archived) session disables run regardless of analysis. */
  disabled?: boolean;
}

function buildRunRequest(analysis: AnalysisResultMeta): RunRequest {
  const params = analysis.params;
  const args: Record<string, unknown> =
    params && typeof params === 'object' && !Array.isArray(params)
      ? (params as Record<string, unknown>)
      : {};
  return {
    skill_id: analysis.algorithm_id,
    dataset_id: analysis.dataset_id,
    args,
  };
}

export function RunControls({ sessionId, analysis, copyText, disabled = false }: RunControlsProps) {
  const { state, run, reset, stop } = useCodeRun();

  const isRunning = state.status === 'running';
  const canRun = !disabled && !!analysis && !isRunning;

  const handleRun = useCallback(() => {
    if (!analysis) return;
    void run(sessionId, buildRunRequest(analysis));
  }, [analysis, run, sessionId]);

  return (
    <Space direction="vertical" size={8} style={{ width: '100%' }}>
      <Space size={8} wrap>
        <Button
          type="primary"
          icon={isRunning ? <LoadingOutlined spin /> : <PlayCircleOutlined />}
          loading={isRunning}
          disabled={!canRun}
          onClick={handleRun}
          aria-label="运行"
        >
          运行
        </Button>
        <Button icon={<BugOutlined />} disabled={!canRun} onClick={handleRun} aria-label="调试">
          调试
        </Button>
        <Button icon={<StopOutlined />} disabled={!isRunning} onClick={stop} aria-label="停止">
          停止
        </Button>
        <Button icon={<ClearOutlined />} disabled={state.status === 'idle'} onClick={reset} aria-label="清空">
          清空
        </Button>
        <CopyToClipboard text={copyText} label="复制" />
      </Space>

      {isRunning && (
        <Text type="secondary" style={{ fontSize: 12 }}>
          运行中…
        </Text>
      )}

      {state.status === 'success' && (
        <Alert
          type="success"
          showIcon
          message="运行完成"
          description={`schema ${state.result.schema_version}，风险信号 ${state.result.risk_signals.length} 项`}
        />
      )}

      {state.status === 'error' && (
        <Alert
          type="error"
          showIcon
          message={state.code ?? '运行失败'}
          description={state.message}
        />
      )}
    </Space>
  );
}

export default RunControls;

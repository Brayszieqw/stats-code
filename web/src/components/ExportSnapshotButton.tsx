/**
 * ExportSnapshotButton — 报告区「审计与复现」导出控件。
 *
 * 视觉对齐原 AI 设计语言：`result-contract` 面板（左色条、衬线标题、纸面底）。
 * 交互：仅 completed 可点；成功后触发浏览器下载 + 可关闭中文反馈；
 * 失败用 role="alert" 中文说明（服务端仍是权威门禁）。
 *
 * Validates: Requirements 7.1, 7.7, 7.8
 */

import { useCallback, useState, type ReactElement } from 'react';
import { Alert, Button, Space, Typography } from 'antd';
import {
  CopyOutlined,
  DownloadOutlined,
  FolderOpenOutlined,
  SafetyCertificateOutlined,
} from '@ant-design/icons';

import {
  useSnapshotExport,
  type SnapshotExportError,
} from '../hooks/useSnapshotExport';

const { Text } = Typography;

export interface ExportSnapshotButtonProps {
  /** Identifier of the analysis run to export. */
  runId: string;
  /** Server-side write destination (basename is fine; SPA also downloads the zip). */
  destination: string;
  /**
   * Status of the run as known to the SPA. The button is enabled only
   * when this is exactly `"completed"`; any other value puts it in the
   * disabled UX gate.
   */
  runStatus: string;
  /**
   * Optional injected `fetch` for tests. Production callers omit it and
   * the hook uses the global `fetch`.
   */
  fetchImpl?: typeof fetch;
}

function basenameOf(path: string): string {
  const parts = path.replace(/\\/g, '/').split('/');
  return parts[parts.length - 1] || path;
}

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
}

/**
 * 中文拒绝文案。服务端仍返回结构化字段；这里只负责呈现。
 */
function describeError(error: SnapshotExportError): string {
  if (error.errorCode === 'PayloadTooLarge') {
    if (
      error.measuredBytes !== undefined
      && error.ceilingBytes !== undefined
    ) {
      return `审计包过大：实测 ${formatBytes(error.measuredBytes)}，上限 ${formatBytes(error.ceilingBytes)}。请精简工件后重试。`;
    }
    return error.message || '审计包超过 50 MB 上限，无法导出。';
  }
  if (error.errorCode === 'RunNotCompleted') {
    if (error.actualStatus !== undefined) {
      return `当前运行状态为「${error.actualStatus}」，仅「已完成」的分析可导出审计快照。`;
    }
    return error.message || '分析尚未完成，无法导出审计快照。';
  }
  if (error.errorCode === 'RunNotFound') {
    return error.message
      || '找不到该次分析的导出记录（后端重启后会清空）。请重新运行分析后再导出。';
  }
  if (error.errorCode === 'SnapshotUnavailable') {
    return '审计导出服务未就绪，请确认后端已启动。';
  }
  if (error.errorCode === 'NetworkError') {
    const m = error.message || '';
    if (/failed to fetch|ECONNREFUSED|network/i.test(m)) {
      return '无法连接后端（8080）。请先启动 Stats 后端，再重新运行分析后导出。';
    }
    return `网络异常：${m}`;
  }
  if (error.errorCode === 'FetchUnavailable') {
    return '当前环境无法发起导出请求。';
  }
  if (error.errorCode === 'HTTP_500' || error.errorCode === 'InternalError') {
    if (/run not found/i.test(error.message || '')) {
      return '找不到该次分析的导出记录（后端重启后会清空）。请重新运行分析后再导出。';
    }
    return error.message && !/^Snapshot export failed with HTTP/i.test(error.message)
      ? error.message
      : '导出失败：服务端内部错误。请确认后端已启动，并重新运行分析后再试。';
  }
  if (error.errorCode.startsWith('HTTP_')) {
    return error.message && !/^Snapshot export failed with HTTP/i.test(error.message)
      ? error.message
      : `导出失败（HTTP ${error.errorCode.replace('HTTP_', '')}）。请确认后端在线后重试。`;
  }
  return error.message
    ? error.message
    : `导出失败（${error.errorCode}）`;
}

function disabledHint(runStatus: string): string {
  if (runStatus === 'completed') return '';
  if (!runStatus) return '等待分析完成后可导出。';
  return `当前状态「${runStatus}」· 仅已完成的运行可导出。`;
}

export function ExportSnapshotButton(
  props: ExportSnapshotButtonProps,
): ReactElement {
  const { runId, destination, runStatus, fetchImpl } = props;

  const { state, exportSnapshot, clearFeedback, redownload } = useSnapshotExport(fetchImpl);

  const isCompleted = runStatus === 'completed';
  const buttonDisabled = !isCompleted || state.loading;
  const shortName = basenameOf(destination);
  const hint = disabledHint(runStatus);
  const [copyHint, setCopyHint] = useState<string | null>(null);

  const handleClick = useCallback(() => {
    if (buttonDisabled) return;
    void exportSnapshot({ run_id: runId, destination, download: true });
  }, [buttonDisabled, exportSnapshot, runId, destination]);

  const copyText = useCallback(async (text: string, okLabel: string) => {
    if (!text || !navigator.clipboard?.writeText) {
      setCopyHint('当前环境无法写入剪贴板');
      return;
    }
    try {
      await navigator.clipboard.writeText(text);
      setCopyHint(okLabel);
      window.setTimeout(() => setCopyHint(null), 2_500);
    } catch {
      setCopyHint('复制失败，请手动选择文本');
    }
  }, []);

  const downloadName = state.downloadFilename
    || (state.result ? basenameOf(state.result.snapshot_path) : shortName);

  const buttonLabel = state.loading ? '正在打包…' : '下载审计快照';

  return (
    <section
      className="export-audit-panel result-contract"
      data-testid="export-snapshot-button-root"
      aria-label="审计与复现"
    >
      <div className="result-contract__header export-audit-panel__header">
        <strong>
          <SafetyCertificateOutlined style={{ marginRight: 6 }} />
          审计与复现
        </strong>
        <span>
          运行
          {' '}
          {(runId || '').slice(0, 8) || '—'}
          {' '}
          · 可复核 zip
        </span>
      </div>

      <p className="export-audit-panel__lead">
        导出本次运行的完整审计包（数据指纹、工作流、版本、方法覆盖与叙事材料），
        用于复核与存档。统计数值由本机确定性引擎生成，不是大模型推算。
      </p>

      <div className="export-audit-panel__actions">
        <Button
          type="primary"
          icon={<DownloadOutlined />}
          className="export-snapshot-button-control"
          data-testid="export-snapshot-button"
          disabled={buttonDisabled}
          loading={state.loading}
          aria-busy={state.loading}
          onClick={handleClick}
        >
          {buttonLabel}
        </Button>
        <Text type="secondary" className="export-audit-panel__filename">
          {shortName}
        </Text>
      </div>

      {hint ? (
        <Text type="secondary" className="export-audit-panel__hint">
          {hint}
        </Text>
      ) : null}

      {state.result !== undefined ? (
        <Alert
          className="export-snapshot-toast export-snapshot-toast-success"
          data-testid="export-snapshot-toast-success"
          type="success"
          showIcon
          closable
          onClose={clearFeedback}
          role="status"
          message={state.browserDownloaded ? '已下载到本机' : '审计包已在服务端生成'}
          description={(
            <div className="export-audit-panel__success">
              {state.browserDownloaded ? (
                <>
                  <p className="export-audit-panel__file-row">
                    <FolderOpenOutlined aria-hidden />
                    <span>
                      文件名
                      {' '}
                      <code data-testid="export-download-filename">{downloadName}</code>
                    </span>
                  </p>
                  <p>
                    浏览器出于安全限制，网页无法显示真实磁盘路径。
                    请到系统
                    <strong>「下载」</strong>
                    文件夹打开同名文件（Windows：
                    <code>%USERPROFILE%\Downloads\{downloadName}</code>
                    ），或点浏览器底部
                    <strong>下载栏</strong>
                    中的文件名。
                    不要把文件名粘贴到地址栏打开。
                  </p>
                  <p className="export-audit-panel__sha">
                    SHA-256
                    {' '}
                    <code>{state.result.sha256.slice(0, 16)}…</code>
                  </p>
                  <Space size={4} wrap>
                    <Button
                      size="small"
                      type="link"
                      icon={<CopyOutlined />}
                      onClick={() => void copyText(downloadName, '已复制文件名')}
                    >
                      复制文件名
                    </Button>
                    <Button
                      size="small"
                      type="link"
                      icon={<DownloadOutlined />}
                      onClick={redownload}
                    >
                      再次下载
                    </Button>
                    <Button
                      size="small"
                      type="link"
                      onClick={() => void copyText(
                        state.result?.snapshot_path ?? '',
                        '已复制服务端存档路径',
                      )}
                    >
                      复制服务端存档路径
                    </Button>
                  </Space>
                </>
              ) : (
                <>
                  <p>
                    文件已写入服务端运行目录（开发环境多为后端工作目录），
                    <strong>不是</strong>
                    浏览器「下载」文件夹：
                  </p>
                  <p className="export-audit-panel__path-block">
                    <code data-testid="export-server-path">{state.result.snapshot_path}</code>
                  </p>
                  <p className="export-audit-panel__sha">
                    SHA-256
                    {' '}
                    <code>{state.result.sha256.slice(0, 16)}…</code>
                  </p>
                  <Space size={4} wrap>
                    <Button
                      size="small"
                      type="link"
                      icon={<CopyOutlined />}
                      onClick={() => void copyText(
                        state.result?.snapshot_path ?? '',
                        '已复制完整路径',
                      )}
                    >
                      复制完整路径
                    </Button>
                  </Space>
                </>
              )}
              {copyHint ? (
                <Text type="success" className="export-audit-panel__copy-hint" role="status">
                  {copyHint}
                </Text>
              ) : null}
            </div>
          )}
        />
      ) : null}

      {state.error !== undefined ? (
        <Alert
          className={`export-snapshot-toast export-snapshot-toast-error export-snapshot-toast-error-${state.error.errorCode}`}
          data-testid="export-snapshot-toast-error"
          data-error-code={state.error.errorCode}
          type="error"
          showIcon
          closable
          onClose={clearFeedback}
          role="alert"
          message="导出失败"
          description={describeError(state.error)}
        />
      ) : null}
    </section>
  );
}

export default ExportSnapshotButton;

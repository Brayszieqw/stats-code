/**
 * DatasetUploader — 数据集上传组件
 *
 * 使用 Ant Design Upload.Dragger 提供拖拽上传入口。
 * 文件类型限制：.csv / .tsv
 * 显示上传进度，完成后展示 DatasetSummary 摘要卡片。
 *
 * Validates: Requirements 3.1, 3.2, 3.3
 */

import { useEffect, useRef, useState } from 'react';
import { Upload, Progress, Card, Tag, Table, Typography, Alert, Space, Button } from 'antd';
import {
  InboxOutlined,
  FileExcelOutlined,
  CheckCircleOutlined,
} from '@ant-design/icons';
import { useDatasetUpload } from '../hooks/useDatasetUpload';
import type { DatasetSummary, ColumnSummary } from '../api/types';

const { Dragger } = Upload;
const { Text } = Typography;

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface DatasetUploaderProps {
  sessionId: string;
  onUploadComplete?: (summary: DatasetSummary) => void;
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const ACCEPT = '.csv,.tsv';

const COLUMN_TYPE_COLORS: Record<string, string> = {
  Numeric: 'blue',
  Categorical: 'green',
  Date: 'orange',
  String: 'default',
};

const ENCODING_LABELS: Record<string, string> = {
  Utf8: 'UTF-8',
  Gbk: 'GBK',
  Utf16: 'UTF-16',
};

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function DatasetUploader({ sessionId, onUploadComplete }: DatasetUploaderProps) {
  const { upload, uploading, progress, result, error, reset } = useDatasetUpload(sessionId);
  const notifiedRef = useRef<string | null>(null);
  const [demoLoading, setDemoLoading] = useState(false);
  // The demo loader fetches a static asset before handing off to `upload`, so
  // its failures never reach the hook's `error`. Without this the button just
  // stopped responding — the worst place to be silent, since it is the first
  // thing a new user clicks.
  const [demoError, setDemoError] = useState<string | null>(null);

  // Notify parent when upload completes (once per result)
  useEffect(() => {
    if (result && onUploadComplete && notifiedRef.current !== result.dataset_id) {
      notifiedRef.current = result.dataset_id;
      onUploadComplete(result);
    }
  }, [result, onUploadComplete]);

  // Handle file selection — prevent Ant Design's default upload, use our hook instead
  const handleBeforeUpload = (file: File): false => {
    upload(file);
    return false; // Prevent default upload behavior
  };

  const handleLoadDemo = async () => {
    setDemoLoading(true);
    setDemoError(null);
    try {
      const response = await fetch('/demo_cohort.csv');
      if (!response.ok) {
        throw new Error(`无法获取示例数据（HTTP ${response.status} ${response.statusText}）`);
      }
      const blob = await response.blob();
      const file = new File([blob], 'demo_cohort.csv', { type: 'text/csv' });
      upload(file);
    } catch (err) {
      setDemoError(err instanceof Error ? err.message : '加载示例数据失败，请改用手动上传。');
    } finally {
      setDemoLoading(false);
    }
  };

  return (
    <div style={{ width: '100%' }}>
      {/* Upload area — hide when result is shown */}
      {!result && (
        <Space direction="vertical" style={{ width: '100%' }} size={12}>
          <Dragger
            accept={ACCEPT}
            showUploadList={false}
            beforeUpload={handleBeforeUpload}
            disabled={uploading || demoLoading}
            multiple={false}
            style={{ padding: '16px 0' }}
          >
            <p className="ant-upload-drag-icon">
              <InboxOutlined />
            </p>
            <p className="ant-upload-text">点击或拖拽文件到此区域上传</p>
            <p className="ant-upload-hint">
              支持 .csv、.tsv 格式
            </p>
          </Dragger>
          <Button
            type="link"
            onClick={handleLoadDemo}
            loading={demoLoading || uploading}
            style={{ fontSize: '13px', color: '#38618c', padding: 0 }}
          >
            💡 一键加载临床队列示例数据 (demo_cohort.csv)
          </Button>
        </Space>
      )}

      {/* Upload progress */}
      {uploading && (
        <div style={{ marginTop: 12 }}>
          <Progress percent={progress} status="active" size="small" />
          <Text type="secondary">正在上传...</Text>
        </div>
      )}

      {/* Error display */}
      {error && (
        <Alert
          type="error"
          message={error.message}
          showIcon
          closable
          onClose={reset}
          style={{ marginTop: 12 }}
        />
      )}

      {demoError && (
        <Alert
          type="error"
          message={demoError}
          showIcon
          closable
          onClose={() => setDemoError(null)}
          style={{ marginTop: 12 }}
        />
      )}

      {/* Dataset summary card */}
      {result && <DatasetSummaryCard summary={result} onReset={reset} />}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Summary sub-component
// ---------------------------------------------------------------------------

interface DatasetSummaryCardProps {
  summary: DatasetSummary;
  onReset: () => void;
}

function DatasetSummaryCard({ summary, onReset }: DatasetSummaryCardProps) {
  const columns = [
    {
      title: '列名',
      dataIndex: 'name',
      key: 'name',
      render: (name: string) => <Text strong>{name}</Text>,
    },
    {
      title: '类型',
      dataIndex: 'inferred_type',
      key: 'inferred_type',
      render: (type: string) => (
        <Tag color={COLUMN_TYPE_COLORS[type] || 'default'}>{type}</Tag>
      ),
    },
    {
      title: '缺失值',
      dataIndex: 'missing_count',
      key: 'missing_count',
      render: (count: number) =>
        count > 0 ? <Text type="warning">{count}</Text> : <Text type="secondary">0</Text>,
    },
  ];

  const dataSource = summary.columns.map((col: ColumnSummary, idx: number) => ({
    key: idx,
    ...col,
  }));

  return (
    <Card
      size="small"
      style={{ marginTop: 12, borderColor: '#b7eb8f', background: '#f6ffed' }}
      title={
        <Space>
          <CheckCircleOutlined style={{ color: '#52c41a' }} />
          <span>数据集已上传</span>
        </Space>
      }
      extra={
        <a onClick={onReset} style={{ fontSize: 12 }}>
          重新上传
        </a>
      }
    >
      {/* File meta info */}
      <Space direction="vertical" size={4} style={{ width: '100%', marginBottom: 12 }}>
        <Space>
          <FileExcelOutlined />
          <Text strong>{summary.file_name}</Text>
        </Space>
        <Space split={<Text type="secondary">·</Text>}>
          <Text type="secondary">{summary.row_count} 行</Text>
          <Text type="secondary">{summary.columns.length} 列</Text>
          <Text type="secondary">
            编码: {ENCODING_LABELS[summary.encoding] || summary.encoding}
          </Text>
        </Space>
      </Space>

      {/* Column details table */}
      {summary.columns.length > 0 && (
        <Table
          dataSource={dataSource}
          columns={columns}
          size="small"
          pagination={false}
          scroll={summary.columns.length > 10 ? { y: 300 } : undefined}
        />
      )}
    </Card>
  );
}

export default DatasetUploader;

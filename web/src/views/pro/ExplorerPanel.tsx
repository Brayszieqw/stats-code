/**
 * ExplorerPanel — 专业模式资源管理器（左 Sider）。
 *
 * 列表展示 controller.datasets（文件名/行数/列数）+ 上传入口（DatasetUploader）
 * + 选中态。选中触发 DataExplorer 展示（由父组件根据 selectedDataset 渲染）；
 * 取消选中保留上次画像（lastProfiledDataset 由父组件维护）；空态 Empty。
 * Archived 时禁用上传与选择。
 *
 * Validates: Requirements 5.1, 5.2, 5.3, 5.4, 5.5, 9.3
 */

import { useState } from 'react';
import { Button, Space, Typography, Tag, Empty, Drawer, theme as antdTheme } from 'antd';
import { DatabaseOutlined } from '@ant-design/icons';
import { DatasetUploader } from '../../components/DatasetUploader';
import type { DatasetSummary } from '../../api/types';

const { Text } = Typography;

export interface ExplorerPanelProps {
  datasets: DatasetSummary[];
  sessionId: string;
  selectedDatasetId: string | null;
  onSelect: (dataset: DatasetSummary | null) => void;
  onUploadComplete: (summary: DatasetSummary) => void;
  disabled?: boolean;
}

export function ExplorerPanel({
  datasets,
  sessionId,
  selectedDatasetId,
  onSelect,
  onUploadComplete,
  disabled = false,
}: ExplorerPanelProps) {
  const { token } = antdTheme.useToken();
  const [uploaderOpen, setUploaderOpen] = useState(false);

  return (
    <Space direction="vertical" size={16} style={{ width: '100%' }}>
      <Text strong style={{ fontSize: 13 }}>
        <DatabaseOutlined /> 资源管理器
      </Text>

      <Button
        type="dashed"
        block
        icon={<DatabaseOutlined />}
        onClick={() => setUploaderOpen(true)}
        disabled={disabled}
        aria-label="上传数据集"
      >
        上传数据集
      </Button>

      <div>
        <Text strong style={{ fontSize: 13 }}>
          已载入数据集 ({datasets.length})
        </Text>
        {datasets.length === 0 ? (
          <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂无数据集" style={{ marginTop: 12 }} />
        ) : (
          <Space direction="vertical" size={6} style={{ width: '100%', marginTop: 8 }}>
            {datasets.map((ds) => {
              const isSel = selectedDatasetId === ds.dataset_id;
              return (
                <div
                  key={ds.dataset_id}
                  role="button"
                  tabIndex={disabled ? -1 : 0}
                  aria-label={`数据集: ${ds.file_name}`}
                  aria-pressed={isSel}
                  onClick={() => {
                    if (disabled) return;
                    onSelect(isSel ? null : ds);
                  }}
                  onKeyDown={(e) => {
                    if (disabled) return;
                    if (e.key === 'Enter' || e.key === ' ') {
                      e.preventDefault();
                      onSelect(isSel ? null : ds);
                    }
                  }}
                  style={{
                    padding: '8px 10px',
                    borderRadius: 6,
                    cursor: disabled ? 'not-allowed' : 'pointer',
                    opacity: disabled ? 0.6 : 1,
                    background: isSel ? token.colorFillSecondary : token.colorFillTertiary,
                    border: `1px solid ${isSel ? token.colorPrimaryBorder : 'transparent'}`,
                  }}
                >
                  <Text strong style={{ fontSize: 12 }} ellipsis>
                    {ds.file_name}
                  </Text>
                  <div style={{ marginTop: 4 }}>
                    <Tag color="blue" style={{ fontSize: 10 }}>
                      {ds.row_count} 行
                    </Tag>
                    <Tag style={{ fontSize: 10 }}>{ds.columns.length} 列</Tag>
                  </div>
                </div>
              );
            })}
          </Space>
        )}
      </div>

      <Drawer
        title="上传数据集"
        placement="left"
        width={420}
        open={uploaderOpen}
        onClose={() => setUploaderOpen(false)}
      >
        <DatasetUploader
          sessionId={sessionId}
          onUploadComplete={(summary) => {
            onUploadComplete(summary);
            setUploaderOpen(false);
          }}
        />
      </Drawer>
    </Space>
  );
}

export default ExplorerPanel;

/**
 * ExplorerPanel — 专业模式资源管理器（左侧栏）。
 *
 * 顶部 EXPLORER 标题条与文档标签条对齐（36px），下方为上传入口 + 已载入数据集
 * 列表/空态。选中触发画像展示（父组件据 selectedDataset 渲染）；取消选中保留
 * 上次画像；Archived 时禁用上传与选择。内容紧贴顶部，无多余空白。
 *
 * Validates: Requirements 5.1, 5.2, 5.3, 5.4, 5.5, 9.3
 */

import { useState } from 'react';
import { Button, Typography, Tag, Empty, Drawer } from 'antd';
import { DatabaseOutlined, UploadOutlined } from '@ant-design/icons';
import { DatasetUploader } from '../../components/DatasetUploader';
import type { DatasetSummary } from '../../api/types';

const { Text } = Typography;

const PRIMARY = '#38618c';
const BORDER = '1px solid #e3e1d8';

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
  const [uploaderOpen, setUploaderOpen] = useState(false);

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      {/* 标题条：与文档标签条同高 (36px)，消除顶部空白 */}
      <div
        style={{
          height: 36,
          display: 'flex',
          alignItems: 'center',
          gap: 6,
          padding: '0 12px',
          borderBottom: BORDER,
          fontSize: 11,
          letterSpacing: 0.6,
          textTransform: 'uppercase',
          color: '#8a93a0',
          flexShrink: 0,
        }}
      >
        <DatabaseOutlined /> 资源管理器
      </div>

      <div style={{ padding: 10, flex: 1, overflowY: 'auto' }}>
        <Button
          type="dashed"
          block
          icon={<UploadOutlined />}
          onClick={() => setUploaderOpen(true)}
          disabled={disabled}
          aria-label="上传数据集"
          style={{ marginBottom: 12 }}
        >
          上传数据集
        </Button>

        <Text strong style={{ fontSize: 12, color: '#3a4654' }}>
          已载入数据集 ({datasets.length})
        </Text>

        {datasets.length === 0 ? (
          <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂无数据集" style={{ marginTop: 24 }} />
        ) : (
          <div style={{ marginTop: 8, display: 'flex', flexDirection: 'column', gap: 6 }}>
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
                    background: isSel ? 'rgba(56,97,140,0.1)' : 'transparent',
                    border: `1px solid ${isSel ? PRIMARY : 'transparent'}`,
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
          </div>
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
    </div>
  );
}

export default ExplorerPanel;

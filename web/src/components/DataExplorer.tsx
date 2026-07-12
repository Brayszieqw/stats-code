import { useMemo } from 'react';
import { Table, Card, Row, Col, Statistic, Progress, Typography, Space, Tag, Empty, Alert } from 'antd';
import {
  FileTextOutlined,
  DatabaseOutlined,
  WarningOutlined,
  CheckCircleOutlined,
  InfoCircleOutlined,
} from '@ant-design/icons';
import type { DatasetSummary, ColumnSummary } from '../api/types';

const { Text, Title, Paragraph } = Typography;

export interface DataExplorerProps {
  summary: DatasetSummary | null;
  /** Optional real preview rows parsed from the uploaded file */
  previewRows?: Record<string, any>[] | null;
  /** Stack dense sections when rendered inside the narrow analysis inspector. */
  compact?: boolean;
}

const TYPE_COLORS: Record<string, string> = {
  Numeric: 'cyan',
  Categorical: 'purple',
  Date: 'gold',
  String: 'blue',
};

const TYPE_LABELS: Record<string, string> = {
  Numeric: '数值型 (Numeric)',
  Categorical: '分类 (Categorical)',
  Date: '日期型 (Date)',
  String: '文本型 (String)',
};

export function DataExplorer({ summary, previewRows, compact = false }: DataExplorerProps) {
  if (!summary) {
    return (
      <Card className="glass-panel" style={{ textAlign: 'center', padding: '40px 0' }}>
        <Empty
          image={Empty.PRESENTED_IMAGE_SIMPLE}
          description={<Text type="secondary">暂无选择的数据集，请在侧边栏上传或选择数据集</Text>}
        />
      </Card>
    );
  }

  // File size formatting
  const formattedSize = useMemo(() => {
    const bytes = summary.size_bytes;
    if (bytes === 0) return '0 Bytes';
    const k = 1024;
    const sizes = ['Bytes', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
  }, [summary.size_bytes]);

  // General dataset statistics
  const totalMissing = useMemo(() => {
    return summary.columns.reduce((sum, col) => sum + col.missing_count, 0);
  }, [summary.columns]);

  const totalCells = summary.row_count * summary.columns.length;
  const overallCompleteness = useMemo(() => {
    if (totalCells === 0) return 100;
    return parseFloat((((totalCells - totalMissing) / totalCells) * 100).toFixed(2));
  }, [totalCells, totalMissing]);

  // Columns definition for the fields metadata table
  const metaColumns = [
    {
      title: '字段名',
      dataIndex: 'name',
      key: 'name',
      render: (text: string) => <Text code style={{ fontWeight: 600 }}>{text}</Text>,
    },
    {
      title: '推断类型',
      dataIndex: 'inferred_type',
      key: 'inferred_type',
      render: (type: string) => (
        <Space size={4}>
          <Tag color={TYPE_COLORS[type] || 'default'}>{TYPE_LABELS[type] || type}</Tag>
        </Space>
      ),
    },
    {
      title: '缺失数量',
      dataIndex: 'missing_count',
      key: 'missing_count',
      render: (count: number) =>
        count > 0 ? (
          <Space size={4}>
            <WarningOutlined style={{ color: '#faad14' }} />
            <Text type="warning" strong>{count}</Text>
          </Space>
        ) : (
          <Text type="secondary">0</Text>
        ),
    },
    {
      title: '数据完整率',
      key: 'completeness',
      render: (_: any, record: ColumnSummary) => {
        const rate = summary.row_count > 0
          ? Math.max(0, Math.min(100, Math.round(((summary.row_count - record.missing_count) / summary.row_count) * 100)))
          : 100;
        let strokeColor = '#52c41a'; // Green
        if (rate < 80) strokeColor = '#ff4d4f'; // Red
        else if (rate < 95) strokeColor = '#faad14'; // Orange

        return (
          <div style={{ display: 'flex', alignItems: 'center', gap: '8px', minWidth: '150px' }}>
            <Progress
              percent={rate}
              size="small"
              strokeColor={strokeColor}
              showInfo={false}
              style={{ flex: 1, margin: 0 }}
            />
            <Text style={{ fontSize: '12px', minWidth: '35px', textAlign: 'right' }} strong>
              {rate}%
            </Text>
          </div>
        );
      },
    },
  ];

  // Resolve spreadsheet data preview columns and data source
  const previewDataColumns = useMemo(() => {
    return summary.columns.map((col) => ({
      title: col.name,
      dataIndex: col.name,
      key: col.name,
      ellipsis: true,
      render: (val: any) => {
        if (val === null || val === undefined || val === '') {
          return <span style={{ color: '#d9d9d9', fontStyle: 'italic' }}>N/A</span>;
        }
        if (typeof val === 'number') {
          return <span style={{ fontFamily: 'SFMono-Regular, Consolas, Monaco, monospace' }}>{val}</span>;
        }
        return String(val);
      },
    }));
  }, [summary.columns]);

  const previewDataSource = useMemo(() => {
    if (previewRows && previewRows.length > 0) {
      return previewRows.slice(0, 10).map((row, idx) => ({ key: idx, ...row }));
    }
    return [];
  }, [previewRows]);
  const hasPreviewRows = previewDataSource.length > 0;

  return (
    <Space className="data-explorer" direction="vertical" size={20} style={{ width: '100%' }}>
      {/* File summary details alert */}
      <Alert
        className="glass-panel"
        message={
          <Text strong style={{ fontSize: '15px' }}>
            数据集已装载：{summary.file_name}
          </Text>
        }
        description={
          <div style={{ marginTop: '4px' }}>
            <Space split={<span style={{ color: '#d9d9d9' }}>|</span>} size={12}>
              <Text type="secondary">文件大小: {formattedSize}</Text>
              <Text type="secondary">数据编码: {summary.encoding}</Text>
              <Text type="secondary">上传时间: {new Date(summary.uploaded_at).toLocaleString()}</Text>
            </Space>
          </div>
        }
        type="success"
        showIcon
        icon={<CheckCircleOutlined style={{ color: '#52c41a' }} />}
        style={{ background: 'rgba(82, 196, 26, 0.04)', borderColor: 'rgba(82, 196, 26, 0.2)' }}
      />

      {/* Grid of basic health metrics */}
      <Row gutter={[16, 16]}>
        <Col xs={12} sm={compact ? 12 : 6}>
          <Card className="glass-panel" size="small" styles={{ body: { padding: '16px' } }}>
            <Statistic
              title={<Text type="secondary">样本数量 (行数)</Text>}
              value={summary.row_count}
              valueStyle={{ color: '#38618c', fontWeight: 700 }}
              prefix={<DatabaseOutlined style={{ fontSize: '18px', marginRight: '4px' }} />}
            />
          </Card>
        </Col>
        <Col xs={12} sm={compact ? 12 : 6}>
          <Card className="glass-panel" size="small" styles={{ body: { padding: '16px' } }}>
            <Statistic
              title={<Text type="secondary">变量个数 (列数)</Text>}
              value={summary.columns.length}
              valueStyle={{ color: '#38618c', fontWeight: 700 }}
              prefix={<FileTextOutlined style={{ fontSize: '18px', marginRight: '4px' }} />}
            />
          </Card>
        </Col>
        <Col xs={12} sm={compact ? 12 : 6}>
          <Card className="glass-panel" size="small" styles={{ body: { padding: '16px' } }}>
            <Statistic
              title={<Text type="secondary">缺失值总量</Text>}
              value={totalMissing}
              valueStyle={{ color: totalMissing > 0 ? '#faad14' : '#52c41a', fontWeight: 700 }}
              prefix={totalMissing > 0 ? <WarningOutlined style={{ fontSize: '18px', marginRight: '4px' }} /> : <CheckCircleOutlined style={{ fontSize: '18px', marginRight: '4px' }} />}
            />
          </Card>
        </Col>
        <Col xs={12} sm={compact ? 12 : 6}>
          <Card className="glass-panel" size="small" styles={{ body: { padding: '16px' } }}>
            <Statistic
              title={<Text type="secondary">数据完整率</Text>}
              value={overallCompleteness}
              precision={2}
              suffix="%"
              valueStyle={{ color: overallCompleteness > 90 ? '#52c41a' : '#faad14', fontWeight: 700 }}
            />
          </Card>
        </Col>
      </Row>

      {/* Grid containing Columns Meta and Data Preview */}
      <Row className="data-explorer-details" gutter={[20, 20]}>
        {/* Left column - variables definitions */}
        <Col xs={24} lg={compact ? 24 : 11}>
          <Card
            className="glass-panel"
            title={
              <Title level={5} style={{ margin: 0, color: '#2b3a4a' }}>
                <InfoCircleOutlined style={{ marginRight: '6px', color: '#38618c' }} />
                数据结构与字段描述
              </Title>
            }
            styles={{ body: { padding: '12px' } }}
          >
            <Table
              dataSource={summary.columns.map((col, idx) => ({ ...col, key: idx }))}
              columns={metaColumns}
              pagination={summary.columns.length > 8 ? { pageSize: 8, showSizeChanger: false, size: 'small' } : false}
              size="small"
              bordered={false}
              scroll={compact ? { x: 'max-content' } : undefined}
              style={{ background: 'transparent' }}
            />
          </Card>
        </Col>

        {/* Right column - Excel-like spreadsheet preview */}
        <Col xs={24} lg={compact ? 24 : 13}>
          <Card
            className="glass-panel"
            title={
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                <Title level={5} style={{ margin: 0, color: '#2b3a4a' }}>
                  <DatabaseOutlined style={{ marginRight: '6px', color: '#38618c' }} />
                  数据样本预览 (前 10 行)
                </Title>
                {hasPreviewRows ? (
                  <Tag color="green" style={{ margin: 0 }}>
                    真实数据预览
                  </Tag>
                ) : (
                  <Tag color="orange" style={{ margin: 0 }}>
                    未缓存原始行
                  </Tag>
                )}
              </div>
            }
            styles={{ body: { padding: '12px' } }}
          >
            <Paragraph style={{ fontSize: '12px', color: '#687b90', marginBottom: '12px' }}>
              {hasPreviewRows
                ? '学术规范预览：表格标题统一居上，使用精简的医学/统计学网格规范，缺失单元格标记为 N/A。'
                : '当前会话没有缓存原始预览行；这里仅展示字段结构和缺失情况，不生成可能误导的模拟数据。'}
            </Paragraph>
            {hasPreviewRows ? (
              <Table
                dataSource={previewDataSource}
                columns={previewDataColumns}
                pagination={false}
                size="small"
                bordered
                scroll={{ x: 'max-content' }}
                style={{ background: 'transparent' }}
                rowClassName={(_, idx) => (idx % 2 === 1 ? 'table-row-alternate' : '')}
              />
            ) : (
              <Empty
                image={Empty.PRESENTED_IMAGE_SIMPLE}
                description={<Text type="secondary">暂无真实数据行预览</Text>}
              />
            )}
          </Card>
        </Col>
      </Row>
    </Space>
  );
}

export default DataExplorer;

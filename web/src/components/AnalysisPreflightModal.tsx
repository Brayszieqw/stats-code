import { Alert, Button, Modal, Space, Tag, Typography } from 'antd';
import { SafetyCertificateOutlined, WarningOutlined } from '@ant-design/icons';
import type { DatasetSummary, RunRequest } from '../api/types';
import { buildAnalysisPreflight } from '../lib/analysisPreflight';

const { Text } = Typography;

export interface AnalysisPreflightModalProps {
  open: boolean;
  dataset: DatasetSummary;
  request: RunRequest;
  promptText: string;
  confirming?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

export function AnalysisPreflightModal({
  open,
  dataset,
  request,
  promptText,
  confirming = false,
  onConfirm,
  onCancel,
}: AnalysisPreflightModalProps) {
  const preflight = buildAnalysisPreflight(dataset, request, promptText);

  return (
    <Modal
      open={open}
      title="执行前确认"
      width={620}
      onCancel={onCancel}
      footer={[
        <Button key="cancel" onClick={onCancel} disabled={confirming}>
          取消
        </Button>,
        <Button key="confirm" type="primary" onClick={onConfirm} loading={confirming}>
          确认并运行
        </Button>,
      ]}
      className="analysis-preflight-modal"
      destroyOnHidden
    >
      <div className="analysis-preflight-modal__summary">
        <div><Text type="secondary">方法</Text><strong>{preflight.methodLabel}</strong></div>
        <div><Text type="secondary">数据集</Text><strong>{preflight.datasetName}</strong></div>
        <div><Text type="secondary">有效范围</Text><strong>n = {preflight.rowCount}</strong></div>
      </div>

      <section className="analysis-preflight-modal__section" aria-label="本次使用变量">
        <Text strong>本次使用变量</Text>
        <Space size={[6, 6]} wrap>
          {preflight.variables.map((variable) => <Tag key={variable}>{variable}</Tag>)}
        </Space>
      </section>

      <section className="analysis-preflight-modal__section" aria-label="变量缺失率">
        <Text strong>变量缺失率</Text>
        <div className="analysis-preflight-modal__missing">
          {preflight.missingRates.map((item) => (
            <span key={item.variable} className={item.rate >= 20 ? 'is-high' : item.rate >= 5 ? 'is-warning' : ''}>
              {item.variable} · {item.missingCount} 例 · {item.rate.toFixed(1)}%
            </span>
          ))}
        </div>
      </section>

      {preflight.warnings.length > 0 ? (
        <div className="analysis-preflight-modal__warnings" aria-label="统计风险提示">
          {preflight.warnings.map((warning, index) => (
            <Alert
              key={`${warning.code}-${index}`}
              type={warning.severity === 'high' ? 'error' : 'warning'}
              showIcon
              icon={<WarningOutlined />}
              message={warning.message}
            />
          ))}
        </div>
      ) : (
        <Alert type="success" showIcon message="未发现预设的样本量、缺失或设计错配风险。" />
      )}

      <div className="analysis-preflight-modal__trust">
        <SafetyCertificateOutlined aria-hidden />
        <span>{preflight.trustStatement}</span>
      </div>
    </Modal>
  );
}

export default AnalysisPreflightModal;

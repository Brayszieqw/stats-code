import { Alert, Button, Modal, Space, Tag, Typography } from 'antd';
import { SafetyCertificateOutlined, WarningOutlined } from '@ant-design/icons';
import type { DatasetAudit, DatasetSummary, ResearchProtocol, RunRequest } from '../api/types';
import { buildAnalysisPreflight } from '../lib/analysisPreflight';

const { Text } = Typography;

export interface AnalysisPreflightModalProps {
  open: boolean;
  dataset: DatasetSummary;
  request: RunRequest;
  promptText: string;
  protocol: ResearchProtocol | null;
  audit: DatasetAudit | null;
  auditLoading?: boolean;
  auditError?: string | null;
  confirming?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
  onEditProtocol: () => void;
}

export function AnalysisPreflightModal({
  open,
  dataset,
  request,
  promptText,
  protocol,
  audit,
  auditLoading = false,
  auditError = null,
  confirming = false,
  onConfirm,
  onCancel,
  onEditProtocol,
}: AnalysisPreflightModalProps) {
  const preflight = buildAnalysisPreflight(dataset, request, promptText, protocol);
  const protocolApproved = protocol?.status === 'Approved';
  const auditBlocked = audit?.status === 'blocked';
  const canApprove = protocolApproved && Boolean(audit) && !auditLoading && !auditError && !auditBlocked;

  return (
    <Modal
      open={open}
      title="分析方案审批"
      width={620}
      onCancel={onCancel}
      footer={[
        <Button key="cancel" onClick={onCancel} disabled={confirming}>
          取消
        </Button>,
        !protocolApproved ? (
          <Button key="protocol" onClick={onEditProtocol} disabled={confirming}>
            完善并审批协议
          </Button>
        ) : null,
        <Button
          key="confirm"
          type="primary"
          onClick={onConfirm}
          loading={confirming}
          disabled={!canApprove}
        >
          批准方案并运行
        </Button>,
      ].filter(Boolean)}
      className="analysis-preflight-modal"
      destroyOnHidden
    >
      {protocolApproved ? (
        <section className="analysis-preflight-modal__protocol" aria-label="已审批研究协议">
          <Alert type="success" showIcon message="研究协议已审批，本次运行将绑定以下研究目标。" />
          <dl>
            <div><dt>研究问题</dt><dd>{protocol.research_question}</dd></div>
            <div><dt>结局</dt><dd>{protocol.outcome}</dd></div>
            <div><dt>目标估计量</dt><dd>{protocol.estimand}</dd></div>
          </dl>
        </section>
      ) : (
        <Alert
          type="error"
          showIcon
          message="研究协议尚未审批"
          description="请先固定研究问题、时间零点、结局和目标估计量；未审批时禁止执行正式分析。"
          style={{ marginBottom: 16 }}
        />
      )}

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

      <section className="analysis-preflight-modal__section" aria-label="数据质量卡">
        <Text strong>数据质量卡（必须审阅）</Text>
        <div className="analysis-preflight-modal__missing">
          {preflight.missingRates.map((item) => (
            <span key={item.variable} className={item.rate >= 20 ? 'is-high' : item.rate >= 5 ? 'is-warning' : ''}>
              {item.variable} · {item.missingCount} 例 · {item.rate.toFixed(1)}%
            </span>
          ))}
        </div>
      </section>

      <section className="analysis-preflight-modal__section" aria-label="服务端数据审计">
        <Text strong>服务端数据审计（执行门禁）</Text>
        {auditLoading ? (
          <Alert type="info" showIcon message="正在由服务端复核原始数据与研究设计…" />
        ) : auditError ? (
          <Alert type="error" showIcon message="服务端审计失败" description={auditError} />
        ) : audit ? (
          <>
            <Alert
              type={audit.status === 'blocked' ? 'error' : audit.status === 'warning' ? 'warning' : 'success'}
              showIcon
              message={audit.status === 'blocked'
                ? `发现 ${audit.findings.filter((finding) => finding.severity === 'blocker').length} 个阻断项，禁止审批和运行`
                : audit.status === 'warning'
                  ? '服务端审计通过，但需审阅警告'
                  : '服务端审计通过，未发现阻断项'}
              description={`审计指纹 ${audit.audit_sha256.slice(0, 12)}… · 协议 v${audit.protocol_version}`}
            />
            {audit.findings.length > 0 ? (
              <ul className="analysis-preflight-modal__server-findings">
                {audit.findings.map((finding, index) => (
                  <li key={`${finding.code}-${index}`} className={`is-${finding.severity}`}>
                    <Tag color={finding.severity === 'blocker' ? 'red' : 'gold'}>{finding.severity === 'blocker' ? '阻断' : '警告'}</Tag>
                    <strong>{finding.code}</strong>
                    <span>{finding.message}</span>
                    {finding.sample_row_numbers.length > 0 ? (
                      <Text type="secondary">示例数据行：{finding.sample_row_numbers.join('、')}</Text>
                    ) : null}
                  </li>
                ))}
              </ul>
            ) : null}
          </>
        ) : (
          <Alert type="warning" showIcon message="尚未取得服务端审计结果，禁止审批。" />
        )}
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

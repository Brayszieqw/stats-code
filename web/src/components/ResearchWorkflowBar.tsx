import { CheckCircleFilled, ClockCircleOutlined, LockOutlined } from '@ant-design/icons';
import type { DatasetAuditStatus, ResearchProtocol } from '../api/types';

type StepState = 'done' | 'active' | 'pending';

export interface ResearchWorkflowBarProps {
  protocol: ResearchProtocol | null;
  datasetReady: boolean;
  auditStatus: DatasetAuditStatus | null;
  planApproved: boolean;
  isRunning: boolean;
  resultReady: boolean;
  onOpenProtocol: () => void;
}

export function ResearchWorkflowBar({
  protocol,
  datasetReady,
  auditStatus,
  planApproved,
  isRunning,
  resultReady,
  onOpenProtocol,
}: ResearchWorkflowBarProps) {
  const protocolApproved = protocol?.status === 'Approved';
  const auditPassed = auditStatus === 'passed' || auditStatus === 'warning';
  const steps: Array<{ key: string; label: string; state: StepState }> = [
    { key: 'protocol', label: '研究协议', state: protocolApproved ? 'done' : 'active' },
    {
      key: 'quality',
      label: auditStatus === 'blocked' ? '数据质控（已阻断）' : '数据质控',
      state: auditPassed ? 'done' : datasetReady ? 'active' : 'pending',
    },
    { key: 'plan', label: '方案审批', state: planApproved ? 'done' : 'pending' },
    { key: 'run', label: '确定性执行', state: resultReady ? 'done' : isRunning ? 'active' : 'pending' },
    { key: 'export', label: '诊断与导出', state: resultReady ? 'active' : 'pending' },
  ];

  return (
    <nav className="research-workflow" aria-label="研究工作流">
      {steps.map((step, index) => {
        const icon = step.state === 'done'
          ? <CheckCircleFilled aria-hidden />
          : step.state === 'active'
            ? <ClockCircleOutlined aria-hidden />
            : <LockOutlined aria-hidden />;
        const content = <>{icon}<span>{index + 1}. {step.label}</span></>;
        return step.key === 'protocol' ? (
          <button
            key={step.key}
            type="button"
            className={`research-workflow__step is-${step.state}`}
            onClick={onOpenProtocol}
            aria-label={`研究协议：${protocolApproved ? '已审批' : protocol ? '草稿' : '未建立'}`}
          >
            {content}
          </button>
        ) : (
          <span key={step.key} className={`research-workflow__step is-${step.state}`}>
            {content}
          </span>
        );
      })}
    </nav>
  );
}

export default ResearchWorkflowBar;

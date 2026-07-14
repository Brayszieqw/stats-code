import type { CSSProperties } from 'react';
import { Alert } from 'antd';
import type { SessionIntegrityWarning } from '../api/types';

export interface SessionIntegrityAlertProps {
  warnings: SessionIntegrityWarning[];
  style?: CSSProperties;
}

export function SessionIntegrityAlert({ warnings, style }: SessionIntegrityAlertProps) {
  if (warnings.length === 0) return null;

  const downgraded = warnings.filter((warning) => warning.action === 'downgraded').length;
  const discarded = warnings.length - downgraded;
  const actions = [
    downgraded > 0 ? `${downgraded} 项已降级` : null,
    discarded > 0 ? `${discarded} 项已忽略` : null,
  ].filter(Boolean).join('，');

  return (
    <Alert
      type="warning"
      showIcon
      role="alert"
      data-testid="session-integrity-warning"
      message="会话完整性检查阻止了不可信审批数据"
      description={`检测到 ${warnings.length} 项存储异常（${actions}）。请重新核对研究协议和分析审批后再运行；原始文件会在存储可写时保留为 quarantine 副本。`}
      style={style}
    />
  );
}

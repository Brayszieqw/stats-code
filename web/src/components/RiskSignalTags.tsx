/**
 * RiskSignalTags — renders risk-signal badges (extracted from MessageList so
 * both the chat bubble and the Pro-mode ReportViewer can share it).
 *
 * Validates: Requirements 6.4, 10.1
 */

import { Space, Tag } from 'antd';
import type { RiskSignal } from '../api/types';

export interface RiskSignalTagsProps {
  signals: RiskSignal[];
}

const LABEL_MAP: Partial<Record<RiskSignal, { text: string; color: string }>> = {
  VifTooHigh: { text: 'VIF > 10', color: 'red' },
  LowPower: { text: '设计阶段功效 < 0.8', color: 'volcano' },
  CoxPhAssumptionViolated: { text: 'Cox PH 假设违反', color: 'magenta' },
  ModelConvergenceFailed: { text: '模型未收敛', color: 'red' },
  SparseData: { text: '事件/参数信息稀疏', color: 'orange' },
  CollinearityDetected: { text: '共线性诊断异常', color: 'volcano' },
};

export function RiskSignalTags({ signals }: RiskSignalTagsProps) {
  const visibleSignals = signals?.filter((signal) => signal !== 'PValueAboveAlpha') ?? [];
  if (visibleSignals.length === 0) return null;

  return (
    <Space size={4} wrap style={{ marginTop: 8 }}>
      {visibleSignals.map((signal, idx) => {
        const info = LABEL_MAP[signal] ?? { text: signal, color: 'default' };
        return (
          <Tag key={idx} color={info.color}>
            {info.text}
          </Tag>
        );
      })}
    </Space>
  );
}

export default RiskSignalTags;

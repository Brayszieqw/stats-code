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

const LABEL_MAP: Record<RiskSignal, { text: string; color: string }> = {
  PValueAboveAlpha: { text: 'P > 0.05', color: 'orange' },
  VifTooHigh: { text: 'VIF > 10', color: 'red' },
  LowPower: { text: '检验功效 < 0.8', color: 'volcano' },
  CoxPhAssumptionViolated: { text: 'Cox PH 假设违反', color: 'magenta' },
};

export function RiskSignalTags({ signals }: RiskSignalTagsProps) {
  if (!signals || signals.length === 0) return null;

  return (
    <Space size={4} wrap style={{ marginTop: 8 }}>
      {signals.map((signal, idx) => {
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

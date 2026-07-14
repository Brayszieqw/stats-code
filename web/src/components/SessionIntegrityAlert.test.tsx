import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { SessionIntegrityAlert } from './SessionIntegrityAlert';

describe('SessionIntegrityAlert', () => {
  it('summarizes fail-closed recovery actions and tells the user to re-approve', () => {
    render(
      <SessionIntegrityAlert
        warnings={[
          {
            event: 'file_session_integrity_warning',
            action: 'downgraded',
            record_type: 'research_protocol',
            session_id: '11111111-1111-4111-8111-111111111111',
            reason: 'state_hash_mismatch',
          },
          {
            event: 'file_session_integrity_warning',
            action: 'discarded',
            record_type: 'analysis_plan_approval',
            session_id: '11111111-1111-4111-8111-111111111111',
            reason: 'protocol_not_approved',
          },
        ]}
      />,
    );

    const alert = screen.getByRole('alert');
    expect(alert).toHaveTextContent('会话完整性检查阻止了不可信审批数据');
    expect(alert).toHaveTextContent('1 项已降级，1 项已忽略');
    expect(alert).toHaveTextContent('重新核对研究协议和分析审批');
  });
});

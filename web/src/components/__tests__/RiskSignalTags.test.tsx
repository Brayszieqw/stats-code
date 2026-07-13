/**
 * Tests for RiskSignalTags.
 *
 * Validates: Requirements 6.4
 */

import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { RiskSignalTags } from '../RiskSignalTags';

describe('RiskSignalTags (Requirement 6.4)', () => {
  it('renders nothing for an empty signal list', () => {
    const { container } = render(<RiskSignalTags signals={[]} />);
    expect(container.firstChild).toBeNull();
  });

  it('hides the legacy PValueAboveAlpha signal and keeps actionable diagnostics', () => {
    render(<RiskSignalTags signals={['PValueAboveAlpha', 'VifTooHigh']} />);
    expect(screen.queryByText('P > 0.05')).not.toBeInTheDocument();
    expect(screen.getByText('VIF > 10')).toBeInTheDocument();
  });

  it('renders nothing for the legacy PValueAboveAlpha signal alone', () => {
    const { container } = render(<RiskSignalTags signals={['PValueAboveAlpha']} />);
    expect(container.firstChild).toBeNull();
  });

  it('renders LowPower and Cox PH labels', () => {
    render(<RiskSignalTags signals={['LowPower', 'CoxPhAssumptionViolated']} />);
    expect(screen.getByText('设计阶段功效 < 0.8')).toBeInTheDocument();
    expect(screen.getByText('Cox PH 假设违反')).toBeInTheDocument();
  });

  it('renders convergence, sparse-information, and collinearity guards', () => {
    render(<RiskSignalTags signals={['ModelConvergenceFailed', 'SparseData', 'CollinearityDetected']} />);
    expect(screen.getByText('模型未收敛')).toBeInTheDocument();
    expect(screen.getByText('事件/参数信息稀疏')).toBeInTheDocument();
    expect(screen.getByText('共线性诊断异常')).toBeInTheDocument();
  });
});

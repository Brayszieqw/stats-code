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

  it('renders the PValueAboveAlpha and VifTooHigh labels', () => {
    render(<RiskSignalTags signals={['PValueAboveAlpha', 'VifTooHigh']} />);
    expect(screen.getByText('P > 0.05')).toBeInTheDocument();
    expect(screen.getByText('VIF > 10')).toBeInTheDocument();
  });

  it('renders LowPower and Cox PH labels', () => {
    render(<RiskSignalTags signals={['LowPower', 'CoxPhAssumptionViolated']} />);
    expect(screen.getByText('检验功效 < 0.8')).toBeInTheDocument();
    expect(screen.getByText('Cox PH 假设违反')).toBeInTheDocument();
  });
});

import { describe, expect, it } from 'vitest';
import { render, screen, within } from '@testing-library/react';

import { ValidationCertificate } from './ValidationCertificate';
import type { AlgorithmEntry } from '../lib/coverageMatrix';

const entry: AlgorithmEntry = {
  id: 'cox',
  display_name: 'Cox Proportional Hazards',
  iterative: true,
  coverage: { R: 'live', SAS: 'recorded', Python: 'sidecar_only', SPSS: 'none' },
  reference: {
    R: { callable: 'survival::coxph', package: 'survival', version: '3.7-0' },
    SAS: { callable: 'PROC PHREG', version: '9.4M8' },
    Python: { callable: 'lifelines.CoxPHFitter', package: 'lifelines', version: '0.28.0' },
    SPSS: { callable: 'COXREG', version: '29.0.1' },
  },
};

describe('ValidationCertificate', () => {
  it('shows engine version, every coverage state, pinned references, and limitations', () => {
    render(
      <ValidationCertificate
        algorithmId="cox"
        entry={entry}
        releaseVersion="0.5.0"
        matrixSchemaVersion={1}
      />,
    );

    const certificate = screen.getByLabelText('验证证书');
    expect(within(certificate).getByText('@stats-code/engine 0.5.0')).toBeInTheDocument();
    expect(within(certificate).getByText('live parity 测试面')).toBeInTheDocument();
    expect(within(certificate).getByText('recorded 金样 parity')).toBeInTheDocument();
    expect(within(certificate).getByText('仅代码 · 未自动 parity')).toBeInTheDocument();
    expect(within(certificate).getByText('未覆盖')).toBeInTheDocument();
    expect(within(certificate).getByText('survival::coxph · 3.7-0')).toBeInTheDocument();
    expect(within(certificate).getByText(/不代表本次运行已调用外部软件实时复算/)).toBeInTheDocument();
    expect(within(certificate).getByText(/数值覆盖不替代研究设计/)).toBeInTheDocument();
  });

  it('does not claim parity for an unregistered algorithm', () => {
    render(
      <ValidationCertificate
        algorithmId="unknown"
        releaseVersion="0.5.0"
        matrixSchemaVersion={1}
      />,
    );
    expect(screen.getByText(/不能声明 parity 或金样验证状态/)).toBeInTheDocument();
  });
});
